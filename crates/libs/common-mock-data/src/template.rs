//! Message templates: text with `{{type option=value}}` placeholders that each render to a
//! fresh value.
//!
//! A placeholder names a data type by its command (`{{person.email}}`) and takes options as
//! space-separated words (`{{random.integer min=1 max=6}}`, `{{random.date past}}`). `\{{`
//! writes a literal `{{`; everything else outside a placeholder, `}}` included, is copied as
//! written. Values are inserted as generated, with no escaping unless the placeholder has
//! `escape=json`, which escapes the value for the inside of a JSON string (RFC 8259 section
//! 7: `"`, `\`, and U+0000 to U+001F). `{{now}}`, `{{utc-now}}` and `{{sequence}}` take no
//! options but `escape` and render a value passed to [`Template::render`]: the clock pair the
//! time as RFC 3339 to the millisecond (local time with its UTC offset, or UTC ending in
//! `Z`), and `{{sequence}}` the `sequence` value in decimal.
//!
//! Compiling checks every placeholder the way [`crate::generate_mock_data`] would, so a
//! template that compiles renders without option errors. Every error starts with the
//! placeholder's line and column ("line 3, column 14: unknown type 'person.fullname'") and
//! quotes at most [`QUOTE_LIMIT`] characters of the template's own text.

use crate::models::{DataType, Locale, MockOptions};
use crate::{generate_mock_data, locale_is_inert, validate};
use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, FixedOffset, Local, SecondsFormat, Utc};
use std::fmt;

/// The most characters of the template's text an error quotes.
pub const QUOTE_LIMIT: usize = 40;

/// The most placeholders one template holds. Bounds the compiled template's memory and the
/// work of one render.
pub const MAX_PLACEHOLDERS: usize = 100_000;

/// The option words a placeholder takes and the types that honor them, one per line with no
/// column padding, so a narrow view can wrap each line.
pub const OPTIONS_HELP: &str = "\
min=N max=N: random.integer, random.float (whole numbers, both included)
precision=N: random.float (decimal places, up to 100)
length=N: internet.password, commerce.product-description (up to 1048576)
age=N: person.birthday (years)
range=N: random.date, random.datetime, random.timestamp (years, 1 to 100000)
past, future: random.date, random.time, random.datetime, random.timestamp
locale=CODE: person.* except birthday, internet.username, commerce.company, commerce.job-title, commerce.industry
escape=json: every placeholder (escapes \", \\, and U+0000 to U+001F inside a JSON string)
now, utc-now: escape only; the current time, RFC 3339 in ms (now: local offset, utc-now: Z)
sequence: escape only; a count of the events that show it, from 0 at app start (not the event hub's sequence number)";

/// A compiled template. `Debug` shows its counts, never its text.
pub struct Template {
    segments: Vec<Segment>,
    placeholders: usize,
    notes: Vec<String>,
    uses_sequence: bool,
}

/// One piece of a compiled template. `json` is true when the placeholder has `escape=json`.
enum Segment {
    Literal(String),
    Mock {
        options: MockOptions,
        at: Position,
        json: bool,
    },
    Given {
        given: Given,
        at: Position,
        json: bool,
    },
}

/// A placeholder that renders a value passed to [`Template::render`]: the time, in the local
/// time zone or in UTC, or the `sequence` value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Given {
    Local,
    Utc,
    Sequence,
}

impl Given {
    const ALL: [Given; 3] = [Given::Local, Given::Utc, Given::Sequence];

    fn name(self) -> &'static str {
        match self {
            Given::Local => "now",
            Given::Utc => "utc-now",
            Given::Sequence => "sequence",
        }
    }

    /// The [`placeholder_groups`] group the placeholder is listed under.
    fn group(self) -> &'static str {
        match self {
            Given::Local | Given::Utc => "clock",
            Given::Sequence => "counter",
        }
    }
}

/// `now` at `offset` as RFC 3339 to the millisecond, always with a numeric offset.
fn local_text(now: DateTime<Utc>, offset: FixedOffset) -> String {
    now.with_timezone(&offset)
        .to_rfc3339_opts(SecondsFormat::Millis, false)
}

/// `now` as RFC 3339 to the millisecond, ending in `Z`.
fn utc_text(now: DateTime<Utc>) -> String {
    now.to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Where a placeholder starts: 1-based line and column, the column counted in characters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Position {
    line: u32,
    column: u32,
}

impl Position {
    const START: Self = Self { line: 1, column: 1 };

    fn advance(&mut self, text: &str) {
        for c in text.chars() {
            if c == '\n' {
                self.line = self.line.saturating_add(1);
                self.column = 1;
            } else {
                self.column = self.column.saturating_add(1);
            }
        }
    }
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}, column {}", self.line, self.column)
    }
}

impl fmt::Debug for Template {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Template")
            .field("segments", &self.segments.len())
            .field("placeholders", &self.placeholders)
            .finish()
    }
}

impl Template {
    /// Compiles `source`, checking every placeholder's type and options.
    ///
    /// # Errors
    /// Fails on the first problem, naming its line and column: a `{{` with no `}}` after it,
    /// an empty placeholder, an unknown type, option, or escape, an option value that does not
    /// parse, an option given twice, a value on `past` or `future`, every option combination
    /// [`crate::generate_mock_data`] refuses, and more than [`MAX_PLACEHOLDERS`]
    /// placeholders.
    pub fn compile(source: &str) -> Result<Self> {
        let mut segments = Vec::new();
        let mut placeholders = 0;
        let mut notes = Vec::new();
        let mut uses_sequence = false;
        // One note per type and locale pair, however often the pair repeats.
        let mut noted: Vec<(&'static str, Locale)> = Vec::new();
        let mut literal = String::new();
        let mut at = Position::START;
        let mut rest = source;
        while !rest.is_empty() {
            if let Some(after) = rest.strip_prefix("\\{{") {
                literal.push_str("{{");
                at.advance("\\{{");
                rest = after;
            } else if let Some(after) = rest.strip_prefix("{{") {
                let Some(end) = after.find("}}") else {
                    bail!("{at}: '{{{{' is not closed");
                };
                if placeholders == MAX_PLACEHOLDERS {
                    bail!("{at}: the template has more than {MAX_PLACEHOLDERS} placeholders");
                }
                let words = &after[..end];
                let segment = match given(words, at)? {
                    Some((given, json)) => {
                        uses_sequence |= given == Given::Sequence;
                        Segment::Given { given, at, json }
                    }
                    None => {
                        let (options, json) = placeholder(words, at)?;
                        let inert = (options.data_type.command(), options.locale);
                        if locale_is_inert(options.locale, &options.data_type)
                            && !noted.contains(&inert)
                        {
                            notes.push(format!(
                                "{at}: locale={} has no effect on {}, which is not locale-aware",
                                options.locale,
                                options.data_type.command()
                            ));
                            noted.push(inert);
                        }
                        Segment::Mock { options, at, json }
                    }
                };
                if !literal.is_empty() {
                    segments.push(Segment::Literal(std::mem::take(&mut literal)));
                }
                segments.push(segment);
                placeholders += 1;
                at.advance("{{");
                at.advance(words);
                at.advance("}}");
                rest = &after[end + 2..];
            } else {
                // The first character is copied even when it is a `{` or `\` that starts
                // nothing; the run ends before the next one that might.
                let first = rest.chars().next().map_or(1, char::len_utf8);
                let run = rest[first..]
                    .find(['{', '\\'])
                    .map_or(rest.len(), |index| index + first);
                literal.push_str(&rest[..run]);
                at.advance(&rest[..run]);
                rest = &rest[run..];
            }
        }
        if !literal.is_empty() {
            segments.push(Segment::Literal(literal));
        }
        Ok(Self {
            segments,
            placeholders,
            notes,
            uses_sequence,
        })
    }

    /// One instance of the template: a fresh mock value for every type placeholder, `now` for
    /// every `{{now}}` (in the local time zone) and `{{utc-now}}`, and `sequence` in decimal
    /// for every `{{sequence}}`.
    ///
    /// # Errors
    /// Fails when a generator fails, and once the output passes `max_bytes`. The error names
    /// the cap and, when a placeholder's value crossed it, that placeholder by its position and
    /// type; it never holds the template's own text.
    pub fn render(&self, max_bytes: usize, now: DateTime<Utc>, sequence: u64) -> Result<String> {
        let mut out = String::new();
        // Each given value is formatted once, so one render shows one instant, one offset,
        // and one number.
        let mut local = None;
        let mut utc = None;
        let mut number = None;
        for segment in &self.segments {
            match segment {
                Segment::Literal(text) => push_value(&mut out, text, false, max_bytes)?,
                Segment::Mock { options, at, json } => {
                    let name = options.data_type.command();
                    let value =
                        generate_mock_data(options).with_context(|| format!("{at}: {name}"))?;
                    push_value(&mut out, &value, *json, max_bytes)
                        .with_context(|| format!("{at}: {name}"))?;
                }
                Segment::Given { given, at, json } => {
                    let value = match given {
                        Given::Local => local.get_or_insert_with(|| {
                            local_text(now, *now.with_timezone(&Local).offset())
                        }),
                        Given::Utc => utc.get_or_insert_with(|| utc_text(now)),
                        Given::Sequence => number.get_or_insert_with(|| sequence.to_string()),
                    };
                    push_value(&mut out, value, *json, max_bytes)
                        .with_context(|| format!("{at}: {}", given.name()))?;
                }
            }
        }
        Ok(out)
    }

    /// How many placeholders the template holds.
    pub fn placeholder_count(&self) -> usize {
        self.placeholders
    }

    /// True when the template holds a `{{sequence}}` placeholder.
    pub fn uses_sequence(&self) -> bool {
        self.uses_sequence
    }

    /// One note per type that some placeholder gives a non-default locale it ignores (per
    /// type and locale pair, at the first such placeholder).
    pub fn inert_locale_notes(&self) -> &[String] {
        &self.notes
    }
}

/// The data types by prefix (`person`, `internet`, `random`, `commerce`), each with its
/// commands in help order, then the `clock` group (`now`, `utc-now`) and the `counter` group
/// (`sequence`).
pub fn placeholder_groups() -> Vec<(&'static str, Vec<&'static str>)> {
    let types = DataType::ALL.iter().map(|data_type| {
        let command = data_type.command();
        let prefix = command
            .split_once('.')
            .map_or(command, |(prefix, _)| prefix);
        (prefix, command)
    });
    let given = Given::ALL
        .into_iter()
        .map(|given| (given.group(), given.name()));
    let mut groups: Vec<(&'static str, Vec<&'static str>)> = Vec::new();
    for (group, name) in types.chain(given) {
        match groups.last_mut() {
            Some((last, names)) if *last == group => names.push(name),
            Some(_) | None => groups.push((group, vec![name])),
        }
    }
    groups
}

/// The given value one placeholder's `words` name and whether it has `escape=json`, or `None`
/// when they name something else (an empty placeholder included). `escape` is the only option
/// of a given value.
fn given(words: &str, at: Position) -> Result<Option<(Given, bool)>> {
    let mut words = words.split_whitespace();
    let Some(given) = words
        .next()
        .and_then(|name| Given::ALL.into_iter().find(|given| given.name() == name))
    else {
        return Ok(None);
    };
    let mut json = false;
    for word in words {
        let (key, value) = option_word(word);
        if key != "escape" {
            bail!("{at}: unknown option '{}' for {}", quote(key), given.name());
        }
        if json {
            bail!("{at}: 'escape' is given twice");
        }
        check_escape(value, given.name()).with_context(|| at.to_string())?;
        json = true;
    }
    Ok(Some((given, json)))
}

/// The options one placeholder's `words` (the text between its braces) describe, and whether
/// it has `escape=json`.
fn placeholder(words: &str, at: Position) -> Result<(MockOptions, bool)> {
    let mut words = words.split_whitespace();
    let Some(name) = words.next() else {
        bail!("{at}: the placeholder is empty");
    };
    let data_type = DataType::from_command(name)
        .map_err(|_| anyhow!("{at}: unknown type '{}'", quote(name)))?;
    let mut options = MockOptions {
        data_type,
        min: None,
        max: None,
        length: None,
        precision: None,
        age: None,
        past: false,
        future: false,
        range: None,
        locale: Locale::En,
    };
    let mut json = false;
    let mut given: Vec<&str> = Vec::new();
    for word in words {
        let (key, value) = option_word(word);
        if given.contains(&key) {
            bail!("{at}: '{}' is given twice", quote(key));
        }
        if key == "escape" {
            check_escape(value, options.data_type.command()).with_context(|| at.to_string())?;
            json = true;
        } else {
            set_option(&mut options, key, value).with_context(|| at.to_string())?;
        }
        given.push(key);
    }
    validate(&options, &options.data_type).with_context(|| at.to_string())?;
    Ok((options, json))
}

/// An option word split into its key and its value (`None` for a bare word).
fn option_word(word: &str) -> (&str, Option<&str>) {
    match word.split_once('=') {
        Some((key, value)) => (key, Some(value)),
        None => (word, None),
    }
}

/// Checks the value of `escape` on the placeholder `name`; `json`, in any ASCII case, is the
/// one escape. The error is the problem only; the caller puts the position in front.
fn check_escape(value: Option<&str>, name: &str) -> Result<()> {
    match value {
        Some(value) if value.eq_ignore_ascii_case("json") => Ok(()),
        Some(other) => bail!("unknown escape '{}' for {name}; use json", quote(other)),
        None => bail!("'escape' needs a value, as in escape=json"),
    }
}

/// Appends `value` to `out`, escaped for a JSON string when `json`, then fails when `out` has
/// passed `max_bytes`. The error names the cap only; the caller names the placeholder.
fn push_value(out: &mut String, value: &str, json: bool, max_bytes: usize) -> Result<()> {
    if json {
        push_json_escaped(out, value);
    } else {
        out.push_str(value);
    }
    if out.len() > max_bytes {
        bail!("the rendered body passes the {max_bytes}-byte limit");
    }
    Ok(())
}

/// Appends `value` to `out` escaped for the inside of a JSON string (RFC 8259 section 7): the
/// quotation mark, the reverse solidus, and U+0000 to U+001F, which are the characters JSON
/// requires escaped. DEL, C1 controls, `/`, and non-ASCII text are copied as they are, which
/// JSON allows.
fn push_json_escaped(out: &mut String, value: &str) {
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\u{0}'..='\u{1f}' => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            _ => out.push(c),
        }
    }
}

/// Sets option `key` to `value` (`None` for a bare word). The error is the problem only;
/// the caller puts the position in front.
fn set_option(options: &mut MockOptions, key: &str, value: Option<&str>) -> Result<()> {
    match (key, value) {
        ("past", None) => options.past = true,
        ("future", None) => options.future = true,
        ("past" | "future", Some(_)) => bail!("'{key}' takes no value"),
        ("min", Some(value)) => options.min = Some(number(key, value)?),
        ("max", Some(value)) => options.max = Some(number(key, value)?),
        ("length", Some(value)) => options.length = Some(number(key, value)?),
        ("precision", Some(value)) => options.precision = Some(number(key, value)?),
        ("age", Some(value)) => options.age = Some(number(key, value)?),
        ("range", Some(value)) => options.range = Some(number(key, value)?),
        ("locale", Some(value)) => {
            options.locale = Locale::from_code(value).map_err(|_| {
                anyhow!(
                    "unknown locale '{}'; the codes are {}",
                    quote(value),
                    Locale::all_codes().join(", ")
                )
            })?;
        }
        ("min" | "max" | "length" | "precision" | "age" | "range" | "locale", None) => {
            bail!("'{key}' needs a value, as in {key}=...");
        }
        _ => bail!(
            "unknown option '{}' for {}",
            quote(key),
            options.data_type.command()
        ),
    }
    Ok(())
}

fn number<T: std::str::FromStr>(key: &str, value: &str) -> Result<T> {
    value
        .parse()
        .map_err(|_| anyhow!("'{key}' needs a whole number, not '{}'", quote(value)))
}

/// `text`, cut to [`QUOTE_LIMIT`] characters with "…" when longer.
fn quote(text: &str) -> String {
    match text.char_indices().nth(QUOTE_LIMIT) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text.to_owned(),
    }
}

#[cfg(test)]
mod tests;

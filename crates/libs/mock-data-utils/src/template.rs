//! Message templates: text with `{{type option=value}}` placeholders that each render to a
//! fresh mock value.
//!
//! A placeholder names a data type by its command (`{{person.email}}`) and takes options as
//! space-separated words (`{{random.integer min=1 max=6}}`, `{{random.date past}}`). `\{{`
//! writes a literal `{{`; everything else outside a placeholder, `}}` included, is copied as
//! written. Values are inserted as generated, with no escaping.
//!
//! Compiling checks every placeholder the way [`crate::generate_mock_data`] would, so a
//! template that compiles renders without option errors. Every error starts with the
//! placeholder's line and column ("line 3, column 14: unknown type 'person.fullname'") and
//! quotes at most [`QUOTE_LIMIT`] characters of the template's own text.

use crate::models::{DataType, Locale, MockOptions};
use crate::{generate_mock_data, locale_is_inert, validate};
use anyhow::{Context, Result, anyhow, bail};
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
locale=CODE: person.* except birthday, internet.username, commerce.company, commerce.job-title, commerce.industry";

/// A compiled template. `Debug` shows its counts, never its text.
pub struct Template {
    segments: Vec<Segment>,
    placeholders: usize,
    notes: Vec<String>,
}

enum Segment {
    Literal(String),
    Mock { options: MockOptions, at: Position },
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
    /// an empty placeholder, an unknown type or option, an option value that does not parse,
    /// an option given twice, a value on `past` or `future`, every option combination
    /// [`crate::generate_mock_data`] refuses, and more than [`MAX_PLACEHOLDERS`]
    /// placeholders.
    pub fn compile(source: &str) -> Result<Self> {
        let mut segments = Vec::new();
        let mut placeholders = 0;
        let mut notes = Vec::new();
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
                let options = placeholder(words, at)?;
                let inert = (options.data_type.command(), options.locale);
                if locale_is_inert(options.locale, &options.data_type) && !noted.contains(&inert) {
                    notes.push(format!(
                        "{at}: locale={} has no effect on {}, which is not locale-aware",
                        options.locale,
                        options.data_type.command()
                    ));
                    noted.push(inert);
                }
                if !literal.is_empty() {
                    segments.push(Segment::Literal(std::mem::take(&mut literal)));
                }
                segments.push(Segment::Mock { options, at });
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
        })
    }

    /// One instance of the template, with a fresh value for every placeholder.
    ///
    /// # Errors
    /// Fails when a generator fails, and once the output passes `max_bytes`. The error names
    /// the cap and, when a placeholder's value crossed it, that placeholder by its position and
    /// type; it never holds the template's own text.
    pub fn render(&self, max_bytes: usize) -> Result<String> {
        let mut out = String::new();
        for segment in &self.segments {
            match segment {
                Segment::Literal(text) => {
                    out.push_str(text);
                    if out.len() > max_bytes {
                        bail!("the rendered body passes the {max_bytes}-byte limit");
                    }
                }
                Segment::Mock { options, at } => {
                    let name = options.data_type.command();
                    let value =
                        generate_mock_data(options).with_context(|| format!("{at}: {name}"))?;
                    out.push_str(&value);
                    if out.len() > max_bytes {
                        bail!("{at}: {name}: the rendered body passes the {max_bytes}-byte limit");
                    }
                }
            }
        }
        Ok(out)
    }

    /// How many placeholders the template holds.
    pub fn placeholder_count(&self) -> usize {
        self.placeholders
    }

    /// One note per type that some placeholder gives a non-default locale it ignores (per
    /// type and locale pair, at the first such placeholder).
    pub fn inert_locale_notes(&self) -> &[String] {
        &self.notes
    }
}

/// The data types by prefix (`person`, `internet`, `random`, `commerce`), each with its
/// commands in help order.
pub fn placeholder_groups() -> Vec<(&'static str, Vec<&'static str>)> {
    let mut groups: Vec<(&'static str, Vec<&'static str>)> = Vec::new();
    for data_type in &DataType::ALL {
        let command = data_type.command();
        let prefix = command
            .split_once('.')
            .map_or(command, |(prefix, _)| prefix);
        match groups.last_mut() {
            Some((last, commands)) if *last == prefix => commands.push(command),
            Some(_) | None => groups.push((prefix, vec![command])),
        }
    }
    groups
}

/// The options one placeholder's `words` (the text between its braces) describe.
fn placeholder(words: &str, at: Position) -> Result<MockOptions> {
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
    let mut given: Vec<&str> = Vec::new();
    for word in words {
        let (key, value) = match word.split_once('=') {
            Some((key, value)) => (key, Some(value)),
            None => (word, None),
        };
        if given.contains(&key) {
            bail!("{at}: '{}' is given twice", quote(key));
        }
        set_option(&mut options, key, value).with_context(|| at.to_string())?;
        given.push(key);
    }
    validate(&options, &options.data_type).with_context(|| at.to_string())?;
    Ok(options)
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

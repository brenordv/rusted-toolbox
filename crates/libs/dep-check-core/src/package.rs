//! Package input: one `name` or `name@version` per line or argument.

use crate::ecosystem::Ecosystem;
use common_utils::string_utils::display_line;
use std::collections::HashSet;

/// One package to check, as the user wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageSpec {
    /// The line or argument it came from, trimmed.
    pub input: String,
    pub name: String,
    pub version: Option<String>,
}

/// Why the input was refused: the 1-based line (or argument) number, 0 for the whole input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputIssue {
    pub line: usize,
    pub message: String,
}

/// The most packages one check takes: OSV.dev's batch maximum and GitHub's `affects` maximum.
pub const MAX_PACKAGES: usize = 1000;
/// The longest package name accepted, in characters.
pub const MAX_NAME_CHARS: usize = 256;
/// The longest version accepted, in characters.
pub const MAX_VERSION_CHARS: usize = 256;
/// How much of a refused text an issue quotes, in characters.
const QUOTE_CHARS: usize = 60;

/// Parses one `name` or `name@version`.
///
/// The text splits at its last `@` that is not the first character, so npm's `@scope/name`
/// keeps its scope. Every character of a name and a version comes from a short ASCII set that
/// starts with a letter or a digit, which keeps every value safe to place in a URL.
///
/// # Errors
/// A sentence saying what is wrong with the text, naming any character that is not allowed.
pub fn parse_spec(ecosystem: Ecosystem, text: &str) -> Result<PackageSpec, String> {
    let input = text.trim();
    let (name, version) = split_version(input);
    check_name(ecosystem, name)?;
    let version = match version {
        Some("") => return Err("nothing after @".to_owned()),
        Some(version) => {
            check_version(version)?;
            Some(version.to_owned())
        }
        None => None,
    };
    Ok(PackageSpec {
        input: input.to_owned(),
        name: name.to_owned(),
        version,
    })
}

/// Parses every non-blank line, numbering lines from 1 with blank ones counted.
///
/// A later line naming the same package (by the ecosystem's name rule) at the same version is
/// dropped, so each package is checked once, in first-seen order.
///
/// # Errors
/// The first line that does not parse, the line of the package past [`MAX_PACKAGES`], or line
/// 0 when no line holds a package. The message quotes the line on one line, cut at 60
/// characters.
pub fn parse_specs<'a>(
    ecosystem: Ecosystem,
    lines: impl IntoIterator<Item = &'a str>,
) -> Result<Vec<PackageSpec>, InputIssue> {
    let mut specs = Vec::new();
    let mut seen = HashSet::new();
    for (index, line) in lines.into_iter().enumerate() {
        let number = index + 1;
        if line.trim().is_empty() {
            continue;
        }
        let spec = parse_spec(ecosystem, line).map_err(|reason| InputIssue {
            line: number,
            message: format!("{}: {reason}", quote_input(line.trim())),
        })?;
        let key = (ecosystem.normalize_name(&spec.name), spec.version.clone());
        if !seen.insert(key) {
            continue;
        }
        if specs.len() == MAX_PACKAGES {
            return Err(InputIssue {
                line: number,
                message: format!("more than {MAX_PACKAGES} packages; check them in smaller lists"),
            });
        }
        specs.push(spec);
    }
    if specs.is_empty() {
        return Err(InputIssue {
            line: 0,
            message: "no packages were given".to_owned(),
        });
    }
    Ok(specs)
}

/// The version rule: ASCII letters, digits, and `-._~+!`, starting with a letter or a digit, at
/// most [`MAX_VERSION_CHARS`] characters. `+` covers build metadata and PEP 440 local versions,
/// `!` PEP 440 epochs; the first-character rule keeps a version from being a `.` or `..` path
/// segment.
///
/// # Errors
/// A sentence saying what is wrong with the version.
pub fn check_version(version: &str) -> Result<(), String> {
    if version.is_empty() {
        return Err("the version is empty".to_owned());
    }
    if version.chars().count() > MAX_VERSION_CHARS {
        return Err(format!(
            "the version is longer than {MAX_VERSION_CHARS} characters"
        ));
    }
    if let Some(c) = version.chars().find(|c| !is_version_char(*c)) {
        return Err(format!(
            "the version holds {}, which is not allowed",
            describe_char(c)
        ));
    }
    if !starts_alphanumeric(version) {
        return Err("the version must start with a letter or a digit".to_owned());
    }
    Ok(())
}

/// `text` in double quotes on one line (control characters as spaces), cut at 60 characters
/// with "…".
pub(crate) fn quote_input(text: &str) -> String {
    let mut quoted = display_line(text.chars().take(QUOTE_CHARS));
    if text.chars().count() > QUOTE_CHARS {
        quoted.push('…');
    }
    format!("\"{quoted}\"")
}

fn split_version(text: &str) -> (&str, Option<&str>) {
    match text.rfind('@') {
        Some(at) if at > 0 => (&text[..at], Some(&text[at + 1..])),
        Some(_) | None => (text, None),
    }
}

fn check_name(ecosystem: Ecosystem, name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("the name is empty".to_owned());
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(format!(
            "the name is longer than {MAX_NAME_CHARS} characters"
        ));
    }
    if ecosystem == Ecosystem::Npm
        && let Some(scoped) = name.strip_prefix('@')
    {
        return check_scoped_name(scoped);
    }
    check_name_part(name, NameStart::LetterOrDigit)
}

/// npm's `@scope/name`, with the leading `@` already removed.
fn check_scoped_name(scoped: &str) -> Result<(), String> {
    let Some((scope, name)) = scoped.split_once('/') else {
        return Err("a scoped npm name needs /name after the scope".to_owned());
    };
    if scope.is_empty() {
        return Err("the scope after @ is empty".to_owned());
    }
    if name.is_empty() {
        return Err("the name after the scope is empty".to_owned());
    }
    check_name_part(scope, NameStart::LetterOrDigit)?;
    check_name_part(name, NameStart::AlsoDotOrUnderscore)?;
    // A name of dots only would be a `.` or `..` path segment wherever the parts are used
    // apart.
    if name.chars().all(|c| c == '.') {
        return Err("the name after the scope cannot be only dots".to_owned());
    }
    Ok(())
}

/// Which first characters a name part may have.
#[derive(Clone, Copy)]
enum NameStart {
    LetterOrDigit,
    /// npm: "The names of scoped packages can begin with a dot or an underscore."
    AlsoDotOrUnderscore,
}

fn check_name_part(part: &str, start: NameStart) -> Result<(), String> {
    if let Some(c) = part.chars().find(|c| !is_name_char(*c)) {
        return Err(format!(
            "the name holds {}, which is not allowed",
            describe_char(c)
        ));
    }
    let first_allowed = match start {
        NameStart::LetterOrDigit => starts_alphanumeric(part),
        NameStart::AlsoDotOrUnderscore => starts_alphanumeric(part) || part.starts_with(['.', '_']),
    };
    if !first_allowed {
        return Err("the name must start with a letter or a digit".to_owned());
    }
    Ok(())
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '~')
}

fn is_version_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '~' | '+' | '!')
}

fn starts_alphanumeric(text: &str) -> bool {
    text.chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric())
}

/// A refused character as a message can show it: quoted when it is visible ASCII, else by its
/// code point, so no control character reaches the terminal.
fn describe_char(c: char) -> String {
    match c {
        ' ' => "a space".to_owned(),
        '\'' => "\"'\"".to_owned(),
        c if c.is_ascii_graphic() => format!("'{c}'"),
        c => format!("U+{:04X}", u32::from(c)),
    }
}

#[cfg(test)]
mod tests;

use chrono::Duration;
use std::fmt::{Debug, Display, Formatter};

/// Makes `input` safer to use as a filename, by character substitution only:
/// filesystem-reserved characters (`< > : " | ? * \ /`) and other special
/// characters become `_`, spaces become `-`, and leading/trailing dots are
/// trimmed.
///
/// This does not handle Windows reserved device names (`CON`, `NUL`, `PRN`,
/// ...), and an input consisting only of dots collapses to an empty string;
/// callers that need those guarantees must add their own checks.
pub fn sanitize_string_for_filename(input: &str) -> String {
    input
        .chars()
        .map(|c| match c {
            // Replace filesystem-unsafe characters with underscores
            '<' | '>' | ':' | '"' | '|' | '?' | '*' | '\\' | '/' => '_',
            // Replace spaces with hyphens for better readability
            ' ' => '-',
            // Keep alphanumeric, dots, hyphens, and underscores
            c if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' => c,
            // Replace any other special characters with underscores
            _ => '_',
        })
        .collect::<String>()
        // Ensure the filename doesn't start or end with dots (problematic on some systems)
        .trim_matches('.')
        .to_string()
}

/// Escapes `input` for terminal display: control characters (C0, DEL, C1) and
/// Unicode bidirectional-control characters render as `escape_debug` sequences
/// so untrusted text cannot inject terminal escape sequences or visually
/// reorder output. Every other character passes through unchanged, so plain
/// text and JSON stay byte-identical for piping.
pub fn escape_for_terminal_display(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len());
    for c in input.chars() {
        if c.is_control() || is_bidi_control(c) {
            escaped.extend(c.escape_debug());
        } else {
            escaped.push(c);
        }
    }
    escaped
}

/// Unicode bidirectional-control characters (the embedding/override set, the
/// isolate set, and the implicit marks LRM/RLM/ALM) can visually reorder
/// terminal output, so display escaping treats them like control characters.
fn is_bidi_control(c: char) -> bool {
    matches!(
        c,
        '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{200E}' | '\u{200F}' | '\u{061C}'
    )
}

/// Whether `c` must not reach a one-line display: control characters, line and paragraph
/// separators, and the bidirectional controls that can make shown text read differently from
/// what copy and search see.
fn is_unsafe_in_a_line(c: char) -> bool {
    c.is_control() || matches!(c, '\u{2028}' | '\u{2029}') || is_bidi_control(c)
}

/// `chars` on one line, with control characters, line and paragraph separators, and
/// bidirectional controls each turned into a space.
pub fn display_line(chars: impl Iterator<Item = char>) -> String {
    chars
        .map(|c| if is_unsafe_in_a_line(c) { ' ' } else { c })
        .collect()
}

/// The most characters of a failure text a surface shows in place.
pub const ERROR_PREVIEW_CHARS: usize = 140;

/// A failure text as a surface shows it in place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ErrorPreview {
    /// At most [`ERROR_PREVIEW_CHARS`] characters on one line, ending in "…" when cut.
    pub shown: String,
    /// Whether characters were left out, so the surface offers the whole text.
    pub cut: bool,
}

/// `text` on one line (control and bidirectional characters as spaces), cut after
/// [`ERROR_PREVIEW_CHARS`] characters with "…". The cut drops trailing spaces and never splits
/// a character.
pub fn error_preview(text: &str) -> ErrorPreview {
    let mut shown = display_line(text.chars().take(ERROR_PREVIEW_CHARS + 1));
    let cut = shown.chars().count() > ERROR_PREVIEW_CHARS;
    if cut {
        shown = shown.chars().take(ERROR_PREVIEW_CHARS).collect();
        shown.truncate(shown.trim_end().len());
        shown.push('…');
    }
    ErrorPreview { shown, cut }
}

/// Converts a `Duration` object into a formatted time string representation in the format `HH:MM:SS.mmm`.
///
/// This function processes an ` Duration ` and formats its total time components into a human-readable string.
/// The output string follows a fixed format structure where:
/// - `HH` represents hours (padded to 2 digits),
/// - `MM` represents minutes (padded to 2 digits),
/// - `SS` represents seconds (padded to 2 digits),
/// - `mmm` represents milliseconds (padded to 3 digits).
///
/// # Arguments
///
/// * `duration` - A `Duration` object representing the time span to be formatted.
///
/// # Returns
///
/// A `String` containing the formatted duration as `HH:MM:SS.mmm`.
pub fn format_duration_to_string(duration: Duration) -> String {
    let total_milliseconds = duration.num_milliseconds().abs();

    let hours = total_milliseconds / 3_600_000;
    let minutes = (total_milliseconds % 3_600_000) / 60_000;
    let seconds = (total_milliseconds % 60_000) / 1_000;
    let milliseconds = total_milliseconds % 1_000;

    format!("{hours:02}:{minutes:02}:{seconds:02}.{milliseconds:03}")
}

/// Formats a given byte value into a human-readable string representation with appropriate units.
///
/// This function converts a byte value into a string that includes the corresponding size suffix
/// (e.g., bytes, KB, MB, GB, or TB) and formats the numeric value with two decimal points precision.
/// It also includes a thousand separators (commas) in the integer part of the number for readability.
///
/// # Arguments
///
/// * `bytes` - A `u64` representing the size in bytes to be formatted.
///
/// # Returns
///
/// * A `String` representing the formatted byte size with the appropriate unit suffix.
///
/// The formatting works as follows:
/// - If the value is less than 1024, it remains in `bytes`.
/// - For values 1024 and above, it is converted iteratively into KB, MB, GB, or TB as appropriate.
/// - Decimal values are shown for units larger than `bytes`, with two digits of precision.
pub fn format_bytes_to_string(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["bytes", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = &UNITS[0];

    for next_unit in &UNITS[1..] {
        if value >= 1024.0 {
            value /= 1024.0;
            unit = next_unit;
        } else {
            break;
        }
    }

    let formatted_number = format!("{value:.2}");
    let parts: Vec<&str> = formatted_number.split('.').collect();
    let whole_part = parts[0];
    let decimal_part = parts[1];

    let formatted_with_commas = group_thousands(whole_part);

    let decimal_suffix = if *unit == "bytes" {
        "".to_string()
    } else {
        format!(".{decimal_part}")
    };

    format!("{formatted_with_commas}{decimal_suffix} {unit}")
}

/// `value` with a comma between each group of three digits, for counts shown to people:
/// `12403` becomes `12,403`.
pub fn format_thousands(value: u64) -> String {
    group_thousands(&value.to_string())
}

/// `text` with the whitespace around it removed, or `None` when nothing is left.
pub fn non_blank(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// `digits` (a run of ASCII digits) with a comma between each group of three, from the right.
fn group_thousands(digits: &str) -> String {
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i != 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(c);
    }
    grouped
}

/// A secret value (connection string, token, key) that cannot leak through
/// formatting: `Debug` and `Display` both render `<redacted>`. The raw value
/// is reachable only through [`expose`](Self::expose).
///
/// Deliberately implements no serde traits, and `common-utils` carries no
/// serde dependency: a struct that embeds a `SecretString` and derives
/// `Serialize` fails to compile, so a secret can never reach serialized
/// output by accident. A carrier that must deserialize a secret converts at
/// its own boundary with `#[serde(from = "String")]` and `From<String>`.
#[derive(Clone, PartialEq)]
pub struct SecretString(String);

impl SecretString {
    /// The raw secret. Keep the result out of logs, error messages, and
    /// serialized output; call sites of this method are the audit surface.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl Display for SecretString {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "<redacted>")
    }
}

impl Debug for SecretString {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "<redacted>")
    }
}

impl From<String> for SecretString {
    fn from(value: String) -> Self {
        Self(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::clean_name_passes_through("hello_world-1.txt", "hello_world-1.txt")]
    #[case::reserved_characters_replaced("a/b:c*d?", "a_b_c_d_")]
    #[case::spaces_become_hyphens("my file.txt", "my-file.txt")]
    #[case::surrounding_dots_trimmed(".hidden.", "hidden")]
    #[case::only_dots_collapse_to_empty("...", "")]
    #[case::empty_stays_empty("", "")]
    fn sanitize_filename_cases(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(sanitize_string_for_filename(input), expected);
    }

    #[rstest]
    #[case::plain_text_passes_through("host-1.example.com", "host-1.example.com")]
    #[case::json_stays_byte_identical(r#"{"a":1,"b":"x y"}"#, r#"{"a":1,"b":"x y"}"#)]
    #[case::c0_escape_byte("evil\u{1b}[2Jname", "evil\\u{1b}[2Jname")]
    #[case::c1_csi("\u{9b}x", "\\u{9b}x")]
    #[case::bidi_override("a\u{202E}b", "a\\u{202e}b")]
    #[case::implicit_bidi_marks("a\u{200E}b\u{200F}c\u{061C}d", "a\\u{200e}b\\u{200f}c\\u{61c}d")]
    #[case::newline_escaped_to_one_line("line1\nline2", "line1\\nline2")]
    #[case::empty_stays_empty("", "")]
    fn escape_for_terminal_display_cases(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(escape_for_terminal_display(input), expected);
    }

    #[test]
    fn display_line_keeps_ordinary_text() {
        assert_eq!(display_line("café ✓".chars()), "café ✓");
    }

    #[rstest]
    #[case::line_break("a\nb", "a b")]
    #[case::carriage_return_and_tab("a\r\tb", "a  b")]
    #[case::escape("a\u{1b}b", "a b")]
    #[case::line_separator("a\u{2028}b", "a b")]
    #[case::paragraph_separator("a\u{2029}b", "a b")]
    #[case::bidi_override("a\u{202E}b", "a b")]
    #[case::bidi_isolate("a\u{2066}b", "a b")]
    #[case::implicit_marks("a\u{200E}\u{200F}\u{061C}b", "a   b")]
    fn display_line_turns_unsafe_characters_into_spaces(
        #[case] input: &str,
        #[case] expected: &str,
    ) {
        assert_eq!(display_line(input.chars()), expected);
    }

    #[rstest]
    #[case::empty(0)]
    #[case::one_short(139)]
    #[case::at_the_cap(140)]
    fn error_preview_shows_a_text_up_to_the_cap_whole(#[case] chars: usize) {
        let text = "a".repeat(chars);

        let preview = error_preview(&text);

        assert_eq!(
            preview,
            ErrorPreview {
                shown: text,
                cut: false
            }
        );
    }

    #[test]
    fn error_preview_cuts_a_text_past_the_cap_with_an_ellipsis() {
        let text = format!("{}b", "a".repeat(ERROR_PREVIEW_CHARS));

        let preview = error_preview(&text);

        assert!(preview.cut);
        assert_eq!(
            preview.shown,
            format!("{}…", "a".repeat(ERROR_PREVIEW_CHARS))
        );
    }

    #[test]
    fn error_preview_never_splits_a_character() {
        let text = "é".repeat(200);

        let preview = error_preview(&text);

        assert_eq!(
            preview.shown,
            format!("{}…", "é".repeat(ERROR_PREVIEW_CHARS))
        );
    }

    #[test]
    fn error_preview_turns_line_breaks_and_unsafe_characters_into_spaces() {
        let preview = error_preview("one\ntwo\tthree\u{2028}four\u{202E}five");

        assert_eq!(preview.shown, "one two three four five");
        assert!(!preview.cut);
    }

    #[test]
    fn error_preview_drops_the_spaces_before_the_ellipsis() {
        let text = format!("{}   tail", "a".repeat(ERROR_PREVIEW_CHARS - 2));

        let preview = error_preview(&text);

        assert_eq!(
            preview.shown,
            format!("{}…", "a".repeat(ERROR_PREVIEW_CHARS - 2))
        );
    }

    #[rstest]
    #[case::zero(Duration::zero(), "00:00:00.000")]
    #[case::subsecond_renders_milliseconds(Duration::milliseconds(250), "00:00:00.250")]
    #[case::over_an_hour(Duration::milliseconds(3_661_005), "01:01:01.005")]
    #[case::negative_uses_magnitude(Duration::milliseconds(-1_500), "00:00:01.500")]
    fn format_duration_cases(#[case] duration: Duration, #[case] expected: &str) {
        assert_eq!(format_duration_to_string(duration), expected);
    }

    // The fractional_kb case pins the `{:.2}` decimal rendering that the
    // parts[0]/parts[1] split depends on: without the decimal point the
    // fractional suffix is lost.
    #[rstest]
    #[case::zero_is_plain_bytes(0, "0 bytes")]
    #[case::below_one_kb_stays_in_bytes(512, "512 bytes")]
    #[case::thousands_separator(1000, "1,000 bytes")]
    #[case::one_kb_switches_unit_and_shows_decimals(1024, "1.00 KB")]
    #[case::fractional_kb(1536, "1.50 KB")]
    #[case::one_tb(1 << 40, "1.00 TB")]
    #[case::above_tb_stays_in_tb(1024 * (1 << 40), "1,024.00 TB")]
    fn format_bytes_cases(#[case] bytes: u64, #[case] expected: &str) {
        assert_eq!(format_bytes_to_string(bytes), expected);
    }

    #[rstest]
    #[case::zero(0, "0")]
    #[case::three_digits(999, "999")]
    #[case::four_digits(1000, "1,000")]
    #[case::six_digits(123_456, "123,456")]
    #[case::seven_digits(1_234_567, "1,234,567")]
    #[case::largest(u64::MAX, "18,446,744,073,709,551,615")]
    fn format_thousands_cases(#[case] value: u64, #[case] expected: &str) {
        assert_eq!(format_thousands(value), expected);
    }

    #[rstest]
    #[case::kept("application/json", Some("application/json"))]
    #[case::trimmed("  application/json ", Some("application/json"))]
    #[case::inner_spaces_kept(" a b ", Some("a b"))]
    #[case::empty("", None)]
    #[case::spaces("   ", None)]
    #[case::tab("\t", None)]
    fn non_blank_cases(#[case] text: &str, #[case] expected: Option<&str>) {
        assert_eq!(non_blank(text).as_deref(), expected);
    }

    #[test]
    fn secret_string_debug_and_display_redact_the_value() {
        let secret = SecretString::from("super-secret-value".to_string());

        assert_eq!(format!("{secret:?}"), "<redacted>");
        assert_eq!(format!("{secret}"), "<redacted>");
    }

    #[test]
    fn secret_string_redacts_inside_a_derived_debug_carrier() {
        #[derive(Debug)]
        struct Carrier {
            secret: SecretString,
        }

        let carrier = Carrier {
            secret: "super-secret-value".to_string().into(),
        };
        let rendered = format!("{carrier:?}");

        assert!(
            !rendered.contains("super-secret-value"),
            "leaked: {rendered}"
        );
        assert!(rendered.contains("<redacted>"));
        assert_eq!(carrier.secret.expose(), "super-secret-value");
    }

    #[test]
    fn secret_string_expose_returns_the_raw_value() {
        let secret: SecretString = "super-secret-value".to_string().into();

        assert_eq!(secret.expose(), "super-secret-value");
    }
}

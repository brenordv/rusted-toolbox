use anyhow::{Context, Result};
use chrono::{DateTime, Local, NaiveDateTime, TimeZone, Utc};
use std::str::FromStr;

/// Layouts carrying an explicit UTC offset. These must parse through
/// `DateTime::parse_from_str`: `NaiveDateTime::parse_from_str` accepts `%z`
/// but documents the offset as ignored, which would silently read the wall
/// time as local.
const OFFSET_LAYOUTS: &[&str] = &[
    "%Y-%m-%dT%H:%M:%S%z", // YYYY-MM-DDTHH:MM:SS<timezone>
];

/// Datetime format patterns for parsing various timestamp formats.
///
/// Supports ISO 8601, space-separated, and slash-separated formats, all read
/// as local wall time. Includes both full datetime and date-only variants for
/// flexible parsing.
const LAYOUTS: &[&str] = &[
    "%Y-%m-%dT%H:%M:%S", // YYYY-MM-DDTHH:MM:SS
    "%Y-%m-%d %H:%M:%S", // YYYY-MM-DD HH:MM:SS
    "%Y-%m-%d %H:%M",    // YYYY-MM-DD HH:MM
    "%Y-%m-%d",          // YYYY-MM-DD
    "%d-%m-%Y %H:%M:%S", // DD-MM-YYYY HH:MM:SS
    "%d-%m-%Y %H:%M",    // DD-MM-YYYY HH:MM
    "%d-%m-%Y",          // DD-MM-YYYY
    "%Y/%m/%d %H:%M:%S", // YYYY/MM/DD HH:MM:SS
    "%Y/%m/%d %H:%M",    // YYYY/MM/DD HH:MM
    "%Y/%m/%d",          // YYYY/MM/DD
    "%d/%m/%Y %H:%M:%S", // DD/MM/YYYY HH:MM:SS
    "%d/%m/%Y %H:%M",    // DD/MM/YYYY HH:MM
    "%d/%m/%Y",          // DD/MM/YYYY
];

/// How a numeric input is read: bare seconds, or milliseconds when the token is
/// longer than 10 digits (the milliseconds value keeps its raw form here; the
/// caller divides by 1000).
#[derive(Debug, PartialEq)]
enum NumericInterpretation {
    Seconds(i64),
    Milliseconds(i64),
}

/// Classifies a numeric input token. Returns `None` when the token is not an
/// integer. Tokens with more than 10 digits (a leading sign excluded) are
/// read as milliseconds (a 10-digit seconds value covers dates up to
/// November 2286).
fn interpret_numeric(input: &str) -> Option<NumericInterpretation> {
    let value = i64::from_str(input).ok()?;

    let digits = input.strip_prefix(['+', '-']).unwrap_or(input);
    if digits.len() > 10 {
        Some(NumericInterpretation::Milliseconds(value))
    } else {
        Some(NumericInterpretation::Seconds(value))
    }
}

/// Processes input for timestamp conversion.
///
/// Handles three scenarios:
/// - Empty input: Shows current Unix timestamp and its datetime representation
/// - Numeric input: Converts Unix timestamp to datetime (UTC and local)
/// - String input: Converts datetime string to Unix timestamp
///
/// # Errors
/// Returns an error when the input is neither a timestamp nor a datetime in a
/// supported format, and when a timestamp is out of chrono's range.
pub fn process_input(input: &str) -> Result<()> {
    if input.is_empty() {
        let now = print_current_unix_timestamp();
        print_unix_as_datetime(now)?;
        return Ok(());
    }

    match interpret_numeric(input) {
        Some(NumericInterpretation::Milliseconds(value)) => {
            println!("- Not a standard Unix timestamp. Treating it as time in milliseconds.");
            print_unix_as_datetime(value / 1000)?;
        }
        Some(NumericInterpretation::Seconds(value)) => {
            print_unix_as_datetime(value)?;
        }
        None => {
            // Not a number: treat it as a date-time string.
            let timestamp = datetime_to_unix(input)?;
            println!("Unix Timestamp: {}", timestamp);
        }
    }

    Ok(())
}

/// Gets and prints the current Unix timestamp.
///
/// The timestamp counts seconds since the Unix epoch (1970-01-01 00:00:00 UTC)
/// and is timezone-independent.
///
/// # Returns
/// Current Unix timestamp as i64
fn print_current_unix_timestamp() -> i64 {
    let current_time = Local::now().timestamp();
    println!("Unix timestamp: {}", current_time);
    current_time
}

/// Renders a Unix timestamp as the two output lines: UTC time (ISO 8601 with Z
/// suffix) and local time (with timezone offset). The local line depends on the
/// process timezone.
fn format_unix_to_datetime(unix_timestamp: i64) -> Result<(String, String)> {
    let utc_time = Utc
        .timestamp_opt(unix_timestamp, 0)
        .single()
        .context(format!("Invalid Unix timestamp: {}", unix_timestamp))?;

    let local_time = utc_time.with_timezone(&Local);

    Ok((
        format!("UTC Time: {}", utc_time.format("%Y-%m-%dT%H:%M:%SZ")),
        format!("Local Time: {}", local_time.format("%Y-%m-%dT%H:%M:%S%z")),
    ))
}

/// Prints the UTC and local datetime lines for a Unix timestamp.
fn print_unix_as_datetime(unix_timestamp: i64) -> Result<()> {
    let (utc_line, local_line) = format_unix_to_datetime(unix_timestamp)?;

    println!("{}", utc_line);
    println!("{}", local_line);

    Ok(())
}

/// Attempts to parse datetime string using multiple format patterns.
///
/// Offset-carrying layouts are tried first and honor the offset as an
/// absolute instant; then full datetime formats, then date-only formats
/// (assuming midnight), both read as local wall time.
///
/// # Errors
/// Returns error message if no format matches the input string
fn guess_datetime_format(input: &str) -> Result<i64> {
    for layout in OFFSET_LAYOUTS {
        if let Ok(dt) = DateTime::parse_from_str(input, layout) {
            return Ok(dt.timestamp());
        }
    }

    for layout in LAYOUTS {
        if let Ok(dt) = NaiveDateTime::parse_from_str(input, layout) {
            return Ok(Local
                .from_local_datetime(&dt)
                .single()
                .context(format!("Invalid datetime: [{}]", input))?
                .timestamp());
        }

        // Also try parsing as date only
        if let Ok(date) = chrono::NaiveDate::parse_from_str(input, layout) {
            let dt = date.and_hms_opt(0, 0, 0).unwrap();
            return Ok(Local
                .from_local_datetime(&dt)
                .single()
                .context(format!("Invalid date: [{}]", input))?
                .timestamp());
        }
    }

    anyhow::bail!(
        "Invalid date-time format. Unable to parse the input: [{}]",
        input
    );
}

/// Converts a datetime string to a Unix timestamp.
///
/// First tries the default format "YYYY-MM-DD HH:MM:SS", then attempts format
/// guessing across the supported layouts.
///
/// # Errors
/// Returns an error when no supported format matches the input.
fn datetime_to_unix(datetime_str: &str) -> Result<i64> {
    if let Ok(dt) = NaiveDateTime::parse_from_str(datetime_str, "%Y-%m-%d %H:%M:%S") {
        return Local
            .from_local_datetime(&dt)
            .single()
            .context(format!("Invalid datetime: [{}]", datetime_str))
            .map(|local| local.timestamp());
    }

    guess_datetime_format(datetime_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guess_datetime_format_parses_full_datetime() {
        assert!(guess_datetime_format("2021-06-15 12:30:00").is_ok());
    }

    #[test]
    fn guess_datetime_format_equivalent_layouts_yield_same_timestamp() {
        let dash = guess_datetime_format("2021-06-15 12:30:00").unwrap();
        let slash = guess_datetime_format("2021/06/15 12:30:00").unwrap();
        let day_first = guess_datetime_format("15-06-2021 12:30:00").unwrap();

        assert_eq!(dash, slash);
        assert_eq!(dash, day_first);
    }

    #[test]
    fn guess_datetime_format_date_only_resolves_to_midnight() {
        let date_only = guess_datetime_format("2021-06-15").unwrap();
        let with_time = guess_datetime_format("2021-06-15 00:00:00").unwrap();

        assert_eq!(date_only, with_time);
    }

    #[test]
    fn guess_datetime_format_rejects_unparseable_input() {
        assert!(guess_datetime_format("not a date").is_err());
        assert!(guess_datetime_format("2021-13-45 99:99:99").is_err());
    }

    #[test]
    fn guess_datetime_format_honors_an_explicit_offset() {
        // Absolute instants, independent of the process timezone.
        assert_eq!(
            guess_datetime_format("2021-06-15T12:30:00+0900").unwrap(),
            1623727800
        );
        assert_eq!(
            guess_datetime_format("2021-06-15T12:30:00+0000").unwrap(),
            1623760200
        );
    }

    #[test]
    fn guess_datetime_format_t_form_without_offset_stays_local() {
        let t_form = guess_datetime_format("2021-06-15T12:30:00").unwrap();
        let space_form = guess_datetime_format("2021-06-15 12:30:00").unwrap();

        assert_eq!(t_form, space_form);
    }

    #[test]
    fn format_unix_to_datetime_pins_epoch_utc_line() {
        let (utc_line, _) = format_unix_to_datetime(0).unwrap();

        assert_eq!(utc_line, "UTC Time: 1970-01-01T00:00:00Z");
    }

    #[test]
    fn format_unix_to_datetime_rejects_out_of_range_value() {
        assert!(format_unix_to_datetime(i64::MAX).is_err());
    }

    #[test]
    fn interpret_numeric_reads_up_to_ten_digits_as_seconds() {
        assert_eq!(
            interpret_numeric("1700000000"),
            Some(NumericInterpretation::Seconds(1700000000))
        );
    }

    #[test]
    fn interpret_numeric_reads_longer_tokens_as_milliseconds() {
        assert_eq!(
            interpret_numeric("1700000000000"),
            Some(NumericInterpretation::Milliseconds(1700000000000))
        );
    }

    #[test]
    fn interpret_numeric_returns_none_for_non_numeric_input() {
        assert_eq!(interpret_numeric("2021-06-15"), None);
    }

    #[test]
    fn interpret_numeric_excludes_the_sign_from_the_digit_count() {
        assert_eq!(
            interpret_numeric("-1700000000"),
            Some(NumericInterpretation::Seconds(-1700000000))
        );
        assert_eq!(
            interpret_numeric("+1700000000"),
            Some(NumericInterpretation::Seconds(1700000000))
        );
        assert_eq!(
            interpret_numeric("-1700000000000"),
            Some(NumericInterpretation::Milliseconds(-1700000000000))
        );
    }

    #[test]
    fn milliseconds_input_yields_the_same_datetime_as_its_seconds_value() {
        let from_ms = format_unix_to_datetime(1700000000000 / 1000).unwrap();
        let from_secs = format_unix_to_datetime(1700000000).unwrap();

        assert_eq!(from_ms, from_secs);
    }

    #[test]
    fn process_input_handles_empty_and_numeric_inputs() {
        assert!(process_input("").is_ok());
        assert!(process_input("0").is_ok());
    }

    #[test]
    fn process_input_treats_long_number_as_milliseconds() {
        // A value longer than 10 digits takes the milliseconds branch.
        assert!(process_input("1000000000000").is_ok());
    }

    #[test]
    fn process_input_rejects_unparseable_string() {
        assert!(process_input("definitely not a timestamp").is_err());
    }

    #[test]
    fn datetime_to_unix_matches_default_and_guessed_formats() {
        let default_format = datetime_to_unix("2021-06-15 12:30:00").unwrap();
        let guessed = datetime_to_unix("2021/06/15 12:30:00").unwrap();

        assert_eq!(default_format, guessed);
    }
}

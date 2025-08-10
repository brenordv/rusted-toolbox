use anyhow::{Context, Result};
use chrono::{Local, NaiveDateTime, TimeZone, Utc};
use std::str::FromStr;

/// Datetime format patterns for parsing various timestamp formats.
///
/// Supports ISO 8601, space-separated, and slash-separated formats with optional timezone info.
/// Includes both full datetime and date-only variants for flexible parsing.
const LAYOUTS: &[&str] = &[
    "%Y-%m-%dT%H:%M:%S",   // YYYY-MM-DDTHH:MM:SS
    "%Y-%m-%dT%H:%M:%S%z", // YYYY-MM-DDTHH:MM:SS<timezone>
    "%Y-%m-%d %H:%M:%S",   // YYYY-MM-DD HH:MM:SS
    "%Y-%m-%d %H:%M",      // YYYY-MM-DD HH:MM
    "%Y-%m-%d",            // YYYY-MM-DD
    "%d-%m-%Y %H:%M:%S",   // DD-MM-YYYY HH:MM:SS
    "%d-%m-%Y %H:%M",      // DD-MM-YYYY HH:MM
    "%d-%m-%Y",            // DD-MM-YYYY
    "%Y/%m/%d %H:%M:%S",   // YYYY/MM/DD HH:MM:SS
    "%Y/%m/%d %H:%M",      // YYYY/MM/DD HH:MM
    "%Y/%m/%d",            // YYYY/MM/DD
    "%d/%m/%Y %H:%M:%S",   // DD/MM/YYYY HH:MM:SS
    "%d/%m/%Y %H:%M",      // DD/MM/YYYY HH:MM
    "%d/%m/%Y",            // DD/MM/YYYY
];

/// Processes input for timestamp conversion.
///
/// Handles three scenarios:
/// - Empty input: Shows current Unix timestamp and its datetime representation
/// - Numeric input: Converts Unix timestamp to datetime (UTC and local)
/// - String input: Converts datetime string to Unix timestamp
pub fn process_input(input: &str) -> Result<()> {
    if input.is_empty() {
        let now = print_current_unix_timestamp();
        convert_unix_to_datetime(now)?;
        return Ok(());
    }

    // Try to parse the input as a Unix timestamp (integer)
    if let Ok(unix_timestamp) = i64::from_str(input) {
        if input.len() > 10 {
            // Update to > 11 after - November 20th, 2286.
            println!("⚠️ Not a standard Unix timestamp. Treating it as time in milliseconds.");
            convert_unix_to_datetime(unix_timestamp / 1000)?;
        } else {
            convert_unix_to_datetime(unix_timestamp)?;
        }        
        
        return Ok(());
    }

    // If not a Unix timestamp, treat it as a date-time string
    convert_datetime_to_unix(input)?;

    Ok(())
}

/// Gets and prints the current Unix timestamp.
///
/// Uses local time to calculate seconds elapsed since Unix epoch (1970-01-01 00:00:00 UTC).
///
/// # Returns
/// Current Unix timestamp as i64
fn print_current_unix_timestamp() -> i64 {
    let current_time = Local::now().timestamp();
    println!("Unix timestamp: {}", current_time);
    current_time
}

/// Converts Unix timestamp to UTC and local datetime formats.
///
/// Displays both UTC time (ISO 8601 with Z suffix) and local time (with timezone offset).
fn convert_unix_to_datetime(unix_timestamp: i64) -> Result<()> {
    let utc_time = Utc
        .timestamp_opt(unix_timestamp, 0)
        .single()
        .context(format!("Invalid Unix timestamp: {}", unix_timestamp))?;

    let local_time = utc_time.with_timezone(&Local);

    println!("UTC Time: {}", utc_time.format("%Y-%m-%dT%H:%M:%SZ"));
    println!("Local Time: {}", local_time.format("%Y-%m-%dT%H:%M:%S%z"));

    Ok(())
}

/// Attempts to parse datetime string using multiple format patterns.
///
/// Tries full datetime formats first, then date-only formats (assuming midnight).
/// Uses local timezone for conversion to Unix timestamp.
///
/// # Errors
/// Returns error message if no format matches the input string
fn guess_datetime_format(input: &str) -> Result<i64> {
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

/// Converts datetime string to Unix timestamp.
///
/// First tries the default format "YYYY-MM-DD HH:MM:SS", then attempts format guessing.
/// Prints the resulting Unix timestamp or error message if parsing fails.
fn convert_datetime_to_unix(datetime_str: &str) -> Result<()> {
    // First, try the default format
    if let Ok(dt) = NaiveDateTime::parse_from_str(datetime_str, "%Y-%m-%d %H:%M:%S") {
        let timestamp = Local
            .from_local_datetime(&dt)
            .single()
            .context(format!("Invalid datetime: [{}]", datetime_str))?
            .timestamp();

        println!("Unix Timestamp: {}", timestamp);

        return Ok(());
    }

    // If the default format fails, attempt to guess the format
    if let Ok(timestamp) = guess_datetime_format(datetime_str) {
        println!("Unix Timestamp: {}", timestamp);
    } else {
        println!("Invalid date-time format. Unable to parse the input.");
    }

    Ok(())
}
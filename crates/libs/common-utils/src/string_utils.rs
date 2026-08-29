use anyhow::Result;
use chrono::Duration;

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

pub fn sanitize_string_for_table_name(list_name: &str) -> Result<String> {
    let name = list_name.trim();
    if name.is_empty() {
        anyhow::bail!("List name cannot be empty");
    }
    if !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        anyhow::bail!("Invalid list name '{}': must be alphanumeric or _", name);
    }
    Ok(name.to_string())
}

/// Converts a `Duration` object into a formatted time string representation in the format `HH:MM:SS.mmm`.
///
/// This function processes a `Duration` and formats its total time components into a human-readable string.
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

    format!(
        "{:02}:{:02}:{:02}.{:03}",
        hours, minutes, seconds, milliseconds
    )
}

/// Formats a given byte value into a human-readable string representation with appropriate units.
///
/// This function converts a byte value into a string that includes the corresponding size suffix
/// (e.g., bytes, KB, MB, GB, or TB) and formats the numeric value with two decimal points precision.
/// It also includes a thousand separators (commas) in the integer part of the number for readability.
///
/// # Arguments
///
/// * `bytes` - A reference to a `u64` representing the size in bytes to be formatted.
///
/// # Returns
///
/// * A `String` representing the formatted byte size with the appropriate unit suffix.
///
/// The formatting works as follows:
/// - If the value is less than 1024, it remains in `bytes`.
/// - For values 1024 and above, it is converted iteratively into KB, MB, GB, or TB as appropriate.
/// - Decimal values are shown for units larger than `bytes`, with two digits of precision.
pub fn format_bytes_to_string(bytes: &u64) -> String {
    const UNITS: [&str; 5] = ["bytes", "KB", "MB", "GB", "TB"];
    let mut value = *bytes as f64;
    let mut unit = &UNITS[0];

    for next_unit in &UNITS[1..] {
        if value >= 1024.0 {
            value /= 1024.0;
            unit = next_unit;
        } else {
            break;
        }
    }

    let formatted_number = format!("{:.2}", value);
    let parts: Vec<&str> = formatted_number.split('.').collect();
    let whole_part = parts[0];
    let decimal_part = parts[1];

    // Add thousands separators to the integer part
    let mut formatted_with_commas = String::new();
    for (i, c) in whole_part.chars().rev().enumerate() {
        if i != 0 && i % 3 == 0 {
            formatted_with_commas.insert(0, ',');
        }
        formatted_with_commas.insert(0, c);
    }

    let decimal_suffix = if *unit == "bytes" {
        "".to_string()
    } else {
        format!(".{}", decimal_part)
    };

    format!("{}{} {}", formatted_with_commas, decimal_suffix, unit)
}

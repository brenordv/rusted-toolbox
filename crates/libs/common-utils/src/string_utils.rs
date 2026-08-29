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

    let formatted_number = format!("{:.2}", value);
    let parts: Vec<&str> = formatted_number.split('.').collect();
    let whole_part = parts[0];
    let decimal_part = parts[1];

    // Add thousand separators to the integer part
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_filename_passes_clean_name_through() {
        assert_eq!(
            sanitize_string_for_filename("hello_world-1.txt"),
            "hello_world-1.txt"
        );
    }

    #[test]
    fn sanitize_filename_replaces_reserved_characters() {
        assert_eq!(sanitize_string_for_filename("a/b:c*d?"), "a_b_c_d_");
    }

    #[test]
    fn sanitize_filename_empty_input_stays_empty() {
        assert_eq!(sanitize_string_for_filename(""), "");
    }

    #[test]
    fn format_duration_zero() {
        assert_eq!(format_duration_to_string(Duration::zero()), "00:00:00.000");
    }

    #[test]
    fn format_duration_subsecond_renders_milliseconds() {
        assert_eq!(
            format_duration_to_string(Duration::milliseconds(250)),
            "00:00:00.250"
        );
    }

    #[test]
    fn format_duration_over_an_hour() {
        assert_eq!(
            format_duration_to_string(Duration::milliseconds(3_661_005)),
            "01:01:01.005"
        );
    }

    #[test]
    fn format_bytes_zero_is_plain_bytes() {
        assert_eq!(format_bytes_to_string(0), "0 bytes");
    }

    #[test]
    fn format_bytes_below_one_kb_stays_in_bytes() {
        assert_eq!(format_bytes_to_string(512), "512 bytes");
    }

    #[test]
    fn format_bytes_at_one_kb_switches_unit_and_shows_decimals() {
        assert_eq!(format_bytes_to_string(1024), "1.00 KB");
    }

    #[test]
    fn format_bytes_adds_thousands_separator() {
        assert_eq!(format_bytes_to_string(1000), "1,000 bytes");
    }

    #[test]
    fn format_bytes_renders_fractional_kb() {
        // Pins the `{:.2}` decimal rendering that the parts[0]/parts[1] split
        // depends on: without the decimal point the fractional suffix is lost.
        assert_eq!(format_bytes_to_string(1536), "1.50 KB");
    }
}

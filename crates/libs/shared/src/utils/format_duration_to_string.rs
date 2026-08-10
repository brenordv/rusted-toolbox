use chrono::Duration;

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

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(Duration::zero(), "00:00:00.000", "zero duration")]
    #[case(Duration::milliseconds(1), "00:00:00.001", "1 millisecond")]
    #[case(Duration::milliseconds(999), "00:00:00.999", "999 milliseconds")]
    #[case(Duration::seconds(1), "00:00:01.000", "1 second")]
    #[case(Duration::seconds(59), "00:00:59.000", "59 seconds")]
    #[case(Duration::minutes(1), "00:01:00.000", "1 minute")]
    #[case(Duration::minutes(59), "00:59:00.000", "59 minutes")]
    #[case(Duration::hours(1), "01:00:00.000", "1 hour")]
    #[case(Duration::hours(23), "23:00:00.000", "23 hours")]
    #[case(Duration::hours(24), "24:00:00.000", "24 hours")]
    #[case(Duration::hours(99), "99:00:00.000", "99 hours")]
    fn test_basic_durations(
        #[case] duration: Duration,
        #[case] expected: &str,
        #[case] description: &str,
    ) {
        assert_eq!(
            format_duration_to_string(duration),
            expected,
            "Failed test case: {}",
            description
        );
    }

    #[rstest]
    #[case(
        Duration::hours(1) + Duration::minutes(30) + Duration::seconds(45) + Duration::milliseconds(123),
        "01:30:45.123",
        "complex duration with all components"
    )]
    #[case(
        Duration::hours(12) + Duration::minutes(34) + Duration::seconds(56) + Duration::milliseconds(789),
        "12:34:56.789",
        "another complex duration"
    )]
    #[case(
        Duration::minutes(90),
        "01:30:00.000",
        "90 minutes converts to 1 hour 30 minutes"
    )]
    #[case(
        Duration::seconds(3661),
        "01:01:01.000",
        "3661 seconds converts to 1 hour 1 minute 1 second"
    )]
    #[case(
        Duration::milliseconds(3661001),
        "01:01:01.001",
        "3661001 milliseconds with all components"
    )]
    fn test_complex_durations(
        #[case] duration: Duration,
        #[case] expected: &str,
        #[case] description: &str,
    ) {
        assert_eq!(
            format_duration_to_string(duration),
            expected,
            "Failed test case: {}",
            description
        );
    }

    #[rstest]
    #[case(Duration::milliseconds(-1), "00:00:00.001", "negative 1 millisecond")]
    #[case(Duration::seconds(-30), "00:00:30.000", "negative 30 seconds")]
    #[case(Duration::minutes(-45), "00:45:00.000", "negative 45 minutes")]
    #[case(Duration::hours(-2), "02:00:00.000", "negative 2 hours")]
    #[case(
        Duration::hours(-1) - Duration::minutes(30) - Duration::seconds(45) - Duration::milliseconds(123),
        "01:30:45.123",
        "negative complex duration"
    )]
    fn test_negative_durations(
        #[case] duration: Duration,
        #[case] expected: &str,
        #[case] description: &str,
    ) {
        assert_eq!(
            format_duration_to_string(duration),
            expected,
            "Failed test case: {} (function uses abs())",
            description
        );
    }

    #[rstest]
    #[case(Duration::hours(100), "100:00:00.000", "100 hours")]
    #[case(Duration::hours(999), "999:00:00.000", "999 hours")]
    #[case(Duration::days(1), "24:00:00.000", "1 day = 24 hours")]
    #[case(Duration::days(10), "240:00:00.000", "10 days = 240 hours")]
    #[case(Duration::weeks(1), "168:00:00.000", "1 week = 168 hours")]
    fn test_large_durations(
        #[case] duration: Duration,
        #[case] expected: &str,
        #[case] description: &str,
    ) {
        assert_eq!(
            format_duration_to_string(duration),
            expected,
            "Failed test case: {}",
            description
        );
    }

    #[rstest]
    #[case(
        Duration::microseconds(1),
        "00:00:00.000",
        "1 microsecond rounds to 0 milliseconds"
    )]
    #[case(
        Duration::microseconds(500),
        "00:00:00.000",
        "500 microseconds rounds to 0 milliseconds"
    )]
    #[case(
        Duration::microseconds(999),
        "00:00:00.000",
        "999 microseconds rounds to 0 milliseconds"
    )]
    #[case(
        Duration::microseconds(1000),
        "00:00:00.001",
        "1000 microseconds = 1 millisecond"
    )]
    #[case(
        Duration::microseconds(1500),
        "00:00:00.001",
        "1500 microseconds rounds to 1 millisecond"
    )]
    #[case(
        Duration::nanoseconds(1),
        "00:00:00.000",
        "1 nanosecond rounds to 0 milliseconds"
    )]
    #[case(
        Duration::nanoseconds(1_000_000),
        "00:00:00.001",
        "1 million nanoseconds = 1 millisecond"
    )]
    fn test_sub_millisecond_precision(
        #[case] duration: Duration,
        #[case] expected: &str,
        #[case] description: &str,
    ) {
        assert_eq!(
            format_duration_to_string(duration),
            expected,
            "Failed test case: {}",
            description
        );
    }

    #[test]
    fn test_boundary_values() {
        // Test exact hour boundaries
        assert_eq!(
            format_duration_to_string(Duration::milliseconds(3_600_000)),
            "01:00:00.000"
        );
        assert_eq!(
            format_duration_to_string(Duration::milliseconds(3_599_999)),
            "00:59:59.999"
        );

        // Test exact minute boundaries
        assert_eq!(
            format_duration_to_string(Duration::milliseconds(60_000)),
            "00:01:00.000"
        );
        assert_eq!(
            format_duration_to_string(Duration::milliseconds(59_999)),
            "00:00:59.999"
        );

        // Test exact second boundaries
        assert_eq!(
            format_duration_to_string(Duration::milliseconds(1_000)),
            "00:00:01.000"
        );
    }

    #[test]
    fn test_formatting_padding() {
        // Test that single digits are properly zero-padded
        assert_eq!(
            format_duration_to_string(Duration::hours(1)),
            "01:00:00.000"
        );
        assert_eq!(
            format_duration_to_string(Duration::minutes(1)),
            "00:01:00.000"
        );
        assert_eq!(
            format_duration_to_string(Duration::seconds(1)),
            "00:00:01.000"
        );
        assert_eq!(
            format_duration_to_string(Duration::milliseconds(1)),
            "00:00:00.001"
        );

        // Test that double digits don't get extra padding
        assert_eq!(
            format_duration_to_string(Duration::hours(12)),
            "12:00:00.000"
        );
        assert_eq!(
            format_duration_to_string(Duration::minutes(34)),
            "00:34:00.000"
        );
        assert_eq!(
            format_duration_to_string(Duration::seconds(56)),
            "00:00:56.000"
        );
        assert_eq!(
            format_duration_to_string(Duration::milliseconds(789)),
            "00:00:00.789"
        );
    }

    #[test]
    fn test_very_large_duration() {
        // Test with a very large duration (close to i64 limits but safe)
        let large_duration = Duration::hours(10000);
        let result = format_duration_to_string(large_duration);
        assert_eq!(result, "10000:00:00.000");

        // Verify the pattern is maintained for large numbers
        assert!(result.contains(":"));
        assert!(result.contains("."));
        assert_eq!(result.len(), 15); // "10000:00:00.000" is 15 characters
    }

    #[test]
    fn test_edge_case_calculations() {
        // Test that modulo operations work correctly
        let duration = Duration::milliseconds(7323456); // 2h 2m 3s 456ms
        assert_eq!(format_duration_to_string(duration), "02:02:03.456");

        // Test with exact multiples
        let exact_hours = Duration::milliseconds(7200000); // exactly 2 hours
        assert_eq!(format_duration_to_string(exact_hours), "02:00:00.000");

        let exact_minutes = Duration::milliseconds(120000); // exactly 2 minutes
        assert_eq!(format_duration_to_string(exact_minutes), "00:02:00.000");
    }
}

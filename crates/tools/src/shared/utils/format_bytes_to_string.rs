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

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(0, "0 bytes", "zero bytes")]
    #[case(1, "1 bytes", "single byte")]
    #[case(42, "42 bytes", "small number of bytes")]
    #[case(999, "999 bytes", "bytes just under 1KB")]
    #[case(1000, "1,000 bytes", "exactly 1000 bytes with comma")]
    #[case(1023, "1,023 bytes", "bytes just under 1024")]
    #[case(1024, "1.00 KB", "exactly 1KB")]
    #[case(1536, "1.50 KB", "1.5KB")]
    #[case(2048, "2.00 KB", "exactly 2KB")]
    #[case(1049088, "1.00 MB", "exactly 1MB")]
    #[case(1050624, "1.00 MB", "a little over 1MB")]
    #[case(1048576, "1.00 MB", "exactly 1MB")]
    #[case(1572864, "1.50 MB", "1.5MB")]
    #[case(10485760, "10.00 MB", "10MB")]
    #[case(104857600, "100.00 MB", "100MB")]
    #[case(1073741824, "1.00 GB", "exactly 1GB")]
    #[case(1610612736, "1.50 GB", "1.5GB")]
    #[case(10737418240, "10.00 GB", "10GB")]
    #[case(107374182400, "100.00 GB", "100GB")]
    #[case(1099511627776, "1.00 TB", "exactly 1TB")]
    #[case(1649267441664, "1.50 TB", "1.5TB")]
    #[case(10995116277760, "10.00 TB", "10TB")]
    #[case(109951162777600, "100.00 TB", "100TB")]
    #[case(1125899906842624, "1,024.00 TB", "1PB in TB with comma")]
    #[case(1234, "1.21 KB", "1.21 KB with two digits")]
    #[case(12345, "12.06 KB", "12.06 KB with two digits")]
    #[case(123456, "120.56 KB", "120.56 KB with two digits")]
    #[case(1234567, "1.18 MB", "1.18 MB with two digits")]
    #[case(12345678, "11.77 MB", "11.77 MB with two digits")]
    #[case(123456789, "117.74 MB", "117.74 MB with two digits")]
    #[case(1234567890, "1.15 GB", "1.15 GB with two digits")]
    #[case(1048576000, "1,000.00 MB", "MB with comma in thousands")]
    #[case(10737418240, "10.00 GB", "GB without comma (under 1000)")]
    #[case(107374182400, "100.00 GB", "GB without comma (under 1000)")]
    #[case(1073741824000, "1,000.00 GB", "GB with comma in thousands")]
    #[case(1536, "1.50 KB", "fractional KB")]
    #[case(1610612736, "1.50 GB", "fractional GB")]
    #[case(1125899906842624, "1,024.00 TB", "high precision TB")]
    #[case(1125970173362585, "1,024.06 TB", "TB with small decimal")]
    fn test_bytes_formatting(
        #[case] input: u64,
        #[case] expected: &str,
        #[case] description: &str,
    ) {
        assert_eq!(
            format_bytes_to_string(&input),
            expected,
            "Failed test case: {}",
            description
        );
    }

    #[test]
    fn test_max_u64_value() {
        let max_u64 = u64::MAX;
        let result = format_bytes_to_string(&max_u64);

        // u64::MAX is 18,446,744,073,709,551,615 bytes
        // This is approximately 16 exabytes, but our function only goes up to TB
        // So it should be expressed in TB with a very large number
        assert!(result.ends_with(" TB"));
        assert!(result.contains(","));

        // The value should be around 16,777,216 TB
        let parts: Vec<&str> = result.split_whitespace().collect();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[1], "TB");

        // Should have thousands separators in the TB value
        assert!(parts[0].contains(","));
    }

    #[test]
    fn test_large_values_approaching_overflow() {
        // Test values that are large but within reasonable bounds
        let large_values = vec![
            (u64::MAX / 2, "TB"),
            (u64::MAX / 4, "TB"),
            (u64::MAX / 8, "TB"),
        ];

        for (value, expected_unit) in large_values {
            let result = format_bytes_to_string(&value);
            assert!(result.ends_with(&format!(" {}", expected_unit)));
            assert!(result.contains(","));
        }
    }

    #[test]
    fn test_boundary_values() {
        let boundaries = vec![
            (1023, "bytes"),
            (1024, "KB"),
            (1048575, "KB"),
            (1048576, "MB"),
            (1073741823, "MB"),
            (1073741824, "GB"),
            (1099511627775, "GB"),
            (1099511627776, "TB"),
        ];

        for (value, expected_unit) in boundaries {
            let result = format_bytes_to_string(&value);
            assert!(
                result.ends_with(&format!(" {}", expected_unit))
                    || result.ends_with(&format!(" {}.00", expected_unit))
                    || result.contains(&format!(" {}", expected_unit)),
                "Value {} should use unit {}, got: {}",
                value,
                expected_unit,
                result
            );
        }
    }

    #[test]
    fn test_decimal_formatting_consistency() {
        // Test that bytes never have decimals, but other units always do
        let test_cases = vec![
            (500, false),          // bytes - no decimals
            (1024, true),          // KB - has decimals
            (1048576, true),       // MB - has decimals
            (1073741824, true),    // GB - has decimals
            (1099511627776, true), // TB - has decimals
        ];

        for (value, should_have_decimals) in test_cases {
            let result = format_bytes_to_string(&value);
            let has_decimals = result.contains('.');

            assert_eq!(
                has_decimals, should_have_decimals,
                "Value {} decimal formatting: expected {}, got {}",
                value, should_have_decimals, result
            );
        }
    }

    #[test]
    fn test_rounding_behavior() {
        // Test specific values that might expose rounding issues
        let test_cases = vec![
            (1536, "1.50 KB"), // Exactly 1.5 KB
            (1030, "1.01 KB"), // Should round to 1.01 KB
            (1025, "1.00 KB"), // Should round to 1.00 KB
        ];

        for (value, expected) in test_cases {
            let result = format_bytes_to_string(&value);
            assert_eq!(result, expected, "Rounding test failed for value {}", value);
        }
    }
}

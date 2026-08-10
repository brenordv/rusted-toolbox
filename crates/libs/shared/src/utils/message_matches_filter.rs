/// Checks if a given message matches any of the provided filters.
///
/// This function takes a message string and a list of filters. It converts both
/// the message and the filters to lowercase and determines if the message contains
/// any of the filters as a substring. The comparison is case-insensitive.
///
/// # Arguments
///
/// * `message_data` - A string slice representing the message to be checked.
/// * `filters` - A slice of `String` representing the list of filter patterns.
///
/// # Returns
///
/// A boolean value indicating whether the message matches any of the filters.
/// Returns `true` if at least one filter matches (is found as a substring in the
/// message). Returns `false` otherwise.
///
/// # Notes
///
/// This function performs a case-insensitive match by converting both the
/// message and the filters to lowercase before comparison.
pub fn message_matches_filter(message_data: &str, filters: &[String]) -> bool {
    let message_lower = message_data.to_lowercase();
    filters
        .iter()
        .any(|filter| message_lower.contains(&filter.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("Hello World", vec!["hello"], true, "exact match lowercase")]
    #[case("Hello World", vec!["HELLO"], true, "exact match uppercase")]
    #[case("Hello World", vec!["HeLLo"], true, "exact match mixed case")]
    #[case("Hello World", vec!["world"], true, "partial match at end")]
    #[case("Hello World", vec!["ell"], true, "partial match in middle")]
    #[case("Hello World", vec!["He"], true, "partial match at start")]
    #[case("Hello World", vec!["xyz"], false, "no match")]
    #[case("Hello World", vec!["WORLD!"], false, "partial mismatch with extra char")]
    #[case("Hello World", vec!["Hello World Extra"], false, "filter longer than message")]
    #[case("", vec!["hello"], false, "empty message with filter")]
    #[case("Hello World", vec!["xyz", "hello"], true, "first filter no match, second matches")]
    #[case("Hello World", vec!["hello", "world"], true, "both filters match")]
    #[case("Hello World", vec!["HeLLo", "WoRLD"], true, "multiple filters with mixed case")]
    #[case("Hello World", vec!["abc", "def", "ghi"], false, "multiple filters, none match")]
    #[case("Test message", vec!["test", "message", "extra"], true, "multiple matches")]
    #[case("", vec![], false, "empty message and empty filters")]
    #[case("Hello World", vec![], false, "non-empty message with empty filters")]
    #[case("", vec![""], true, "empty message matches empty filter")]
    #[case("Hello", vec![""], true, "non-empty message contains empty filter")]
    #[case("Café München", vec!["café"], true, "unicode characters")]
    #[case("Hello 世界", vec!["世界"], true, "chinese characters")]
    #[case("Emoji test 🚀", vec!["🚀"], true, "emoji characters")]
    #[case("Mixed café 世界 🚀", vec!["CAFÉ"], true, "mixed unicode with case insensitive")]
    #[case("Line1\nLine2\nLine3", vec!["line2"], true, "multiline with newlines")]
    #[case("Tab\tseparated\tvalues", vec!["separated"], true, "tab characters")]
    #[case("Spaces   everywhere", vec!["   "], true, "multiple spaces")]
    #[case("Special!@#$%^&*()chars", vec!["!@#"], true, "special characters")]
    #[case("JSON: {\"key\": \"value\"}", vec!["json"], true, "JSON-like content")]
    #[case("XML: <root><child>value</child></root>", vec!["<root>"], true, "XML-like content")]
    #[case("CSV: name,age,city", vec!["name,age"], true, "CSV-like content")]
    #[case("Error: Connection timeout", vec!["error"], true, "Error message")]
    #[case("INFO: Process completed successfully", vec!["info"], true, "Info message")]
    fn test_message_matches(
        #[case] message: &str,
        #[case] filters: Vec<&str>,
        #[case] expected: bool,
        #[case] description: &str,
    ) {
        let filter_strings: Vec<String> = filters.into_iter().map(String::from).collect();
        assert_eq!(
            message_matches_filter(message, &filter_strings),
            expected,
            "Failed test case: {}",
            description
        );
    }

    #[test]
    fn test_message_matches_filter_large_input() {
        let large_message = "a".repeat(10000);
        let filters = vec!["a".repeat(100)];

        assert!(
            message_matches_filter(&large_message, &filters),
            "Should match substring in large message"
        );

        let no_match_filters = vec!["b".repeat(100)];
        assert!(
            !message_matches_filter(&large_message, &no_match_filters),
            "Should not match non-existent substring in large message"
        );
    }

    #[test]
    fn test_message_matches_filter_many_filters() {
        let message = "target message";
        let many_filters: Vec<String> = (0..1000).map(|i| format!("filter_{}", i)).collect();

        assert!(
            !message_matches_filter(message, &many_filters),
            "Should not match any of the many non-matching filters"
        );

        let mut filters_with_match = many_filters;
        filters_with_match.push("target".to_string());

        assert!(
            message_matches_filter(message, &filters_with_match),
            "Should find match among many filters"
        );
    }
}

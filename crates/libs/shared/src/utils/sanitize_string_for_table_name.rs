pub fn sanitize_string_for_table_name(list_name: &str) -> anyhow::Result<String> {
    let name = list_name.trim();
    if name.is_empty() {
        anyhow::bail!("List name cannot be empty");
    }
    if !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        anyhow::bail!("Invalid list name '{}': must be alphanumeric or _", name);
    }
    Ok(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("table1", Ok("table1".to_string()))]
    #[case("my_Table_02", Ok("my_Table_02".to_string()))]
    #[case("  Valid_Name123  ", Ok("Valid_Name123".to_string()))]
    #[case("_leading_underscore", Ok("_leading_underscore".to_string()))]
    #[case("single", Ok("single".to_string()))]
    #[case("Foo\n", Ok("Foo".to_string()))]
    #[case("Имя", Ok("Имя".to_string()))] // Cyrillic letters are valid: is_alphanumeric()
    fn test_sanitize_string_for_table_name_valid(
        #[case] input: &str,
        #[case] expected: anyhow::Result<String>,
    ) {
        let result = sanitize_string_for_table_name(input).unwrap();
        let expected = expected.unwrap();
        assert_eq!(result, expected);
    }

    #[rstest]
    #[case("", "List name cannot be empty")]
    #[case("      ", "List name cannot be empty")]
    #[case(
        "name with space",
        "Invalid list name 'name with space': must be alphanumeric or _"
    )]
    #[case("foo-bar", "Invalid list name 'foo-bar': must be alphanumeric or _")]
    #[case("name!", "Invalid list name 'name!': must be alphanumeric or _")]
    #[case("my$table", "Invalid list name 'my$table': must be alphanumeric or _")]
    #[case("with.dot", "Invalid list name 'with.dot': must be alphanumeric or _")]
    #[case(
        "unicode-名",
        "Invalid list name 'unicode-名': must be alphanumeric or _"
    )] // '名' is valid, '-' is not
    #[case("abc\txyz", "Invalid list name 'abc\txyz': must be alphanumeric or _")]
    fn test_sanitize_string_for_table_name_invalid_and_edge(
        #[case] input: &str,
        #[case] expected: String,
    ) {
        let result = sanitize_string_for_table_name(input)
            .unwrap_err()
            .to_string();
        assert!(
            result.contains(expected.to_string().as_str()),
            "error '{}' did not contain expected '{}'",
            result,
            expected
        );
    }

    #[test]
    fn test_very_long_but_valid_table_name() {
        let name = "A".repeat(500);
        let result = sanitize_string_for_table_name(&name);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), name);
    }

    #[test]
    fn test_very_long_and_invalid_table_name() {
        let mut name = "A".repeat(499);
        name.push('-');
        let result = sanitize_string_for_table_name(&name);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Invalid list name"),
            "Unexpected error message: {err}"
        );
    }
}

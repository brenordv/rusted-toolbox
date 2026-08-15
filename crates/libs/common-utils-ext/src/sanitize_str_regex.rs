use once_cell::sync::Lazy;
use regex::Regex;
use std::borrow::Cow;

static NON_PRINTABLE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\p{C}").unwrap());

/// Removes non-printable characters using a precompiled regex.
pub fn clean_str_regex(input: &str) -> Cow<'_, str> {
    NON_PRINTABLE_RE.replace_all(input, "")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("", "")]
    #[case("hello world", "hello world")]
    #[case("Hello, World! 123", "Hello, World! 123")]
    #[case("spaces and tabs", "spaces and tabs")]
    fn test_clean_str_regex_printable_chars(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(clean_str_regex(input), expected);
    }

    #[rstest]
    #[case("hello\nworld", "helloworld")]
    #[case("hello\tworld", "helloworld")]
    #[case("hello\rworld", "helloworld")]
    #[case("hello\r\nworld", "helloworld")]
    #[case("text\x00with\x01null", "textwithnull")]
    #[case("bell\x07sound", "bellsound")]
    fn test_clean_str_regex_control_chars(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(clean_str_regex(input), expected);
    }

    #[rstest]
    #[case("hello\u{200B}world", "helloworld")] // Zero-width space
    #[case("text\u{FEFF}with\u{200C}zwj", "textwithzwj")] // BOM and zero-width non-joiner
    #[case("invisible\u{200D}chars", "invisiblechars")] // Zero-width joiner
    fn test_clean_str_regex_unicode_invisible(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(clean_str_regex(input), expected);
    }

    #[rstest]
    #[case("café", "café")]
    #[case("naïve", "naïve")]
    #[case("José", "José")]
    #[case("München", "München")]
    #[case("東京", "東京")]
    #[case("🦀 Rust", "🦀 Rust")]
    #[case("Здравствуй мир", "Здравствуй мир")]
    fn test_clean_str_regex_unicode_printable(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(clean_str_regex(input), expected);
    }

    #[rstest]
    #[case("hello\x7Fworld", "helloworld")] // DEL character
    #[case("text\x1Bwith\x1Bescape", "textwithescape")] // ESC character
    #[case("start\x0Cmiddle\x0Cend", "startmiddleend")] // Form feed
    fn test_clean_str_regex_ascii_control(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(clean_str_regex(input), expected);
    }

    #[test]
    fn test_clean_str_regex_mixed_content() {
        let input = "Hello\nWorld\t!\x00Clean\x01Me\u{200B}Please";
        let expected = "HelloWorld!CleanMePlease";
        assert_eq!(clean_str_regex(input), expected);
    }

    #[test]
    fn test_clean_str_regex_only_non_printable() {
        let input = "\n\t\r\x00\x01\u{200B}";
        let expected = "";
        assert_eq!(clean_str_regex(input), expected);
    }

    #[test]
    fn test_clean_str_regex_preserves_spaces() {
        let input = "hello world with spaces";
        let expected = "hello world with spaces";
        assert_eq!(clean_str_regex(input), expected);
    }

    #[test]
    fn test_clean_str_regex_special_punctuation() {
        let input = "!@#$%^&*()_+-=[]{}|;':\",./<>?";
        let expected = "!@#$%^&*()_+-=[]{}|;':\",./<>?";
        assert_eq!(clean_str_regex(input), expected);
    }

    #[test]
    fn test_clean_str_regex_numbers_and_letters() {
        let input = "ABC123xyz789";
        let expected = "ABC123xyz789";
        assert_eq!(clean_str_regex(input), expected);
    }

    #[test]
    fn test_clean_str_regex_large_string() {
        let clean_part = "a".repeat(1000);
        let input = format!("{}\n{}\t{}", clean_part, clean_part, clean_part);
        let expected = format!("{}{}{}", clean_part, clean_part, clean_part);
        assert_eq!(clean_str_regex(&input), expected);
    }
}

use super::*;
use rstest::rstest;

fn names_and_versions(specs: &[PackageSpec]) -> Vec<(&str, Option<&str>)> {
    specs
        .iter()
        .map(|spec| (spec.name.as_str(), spec.version.as_deref()))
        .collect()
}

#[rstest]
#[case::plain(Ecosystem::Cargo, "serde", "serde", None)]
#[case::versioned(Ecosystem::Cargo, "serde@1.0.215", "serde", Some("1.0.215"))]
#[case::trimmed(Ecosystem::Cargo, "  tokio@1.20.0 ", "tokio", Some("1.20.0"))]
#[case::scoped_npm(Ecosystem::Npm, "@types/node", "@types/node", None)]
#[case::scoped_npm_versioned(Ecosystem::Npm, "@types/node@20.1.0", "@types/node", Some("20.1.0"))]
fn parse_spec_splits_at_the_last_at(
    #[case] ecosystem: Ecosystem,
    #[case] text: &str,
    #[case] name: &str,
    #[case] version: Option<&str>,
) {
    let spec = parse_spec(ecosystem, text).unwrap();

    assert_eq!(
        (spec.name.as_str(), spec.version.as_deref()),
        (name, version)
    );
    assert_eq!(spec.input, text.trim());
}

#[rstest]
#[case::empty_version("serde@", "nothing after @")]
#[case::empty_text("  ", "the name is empty")]
#[case::lone_at("@", "the name holds '@', which is not allowed")]
fn parse_spec_refuses_an_empty_part(#[case] text: &str, #[case] reason: &str) {
    let error = parse_spec(Ecosystem::Cargo, text).unwrap_err();

    assert_eq!(error, reason);
}

#[rstest]
#[case::comma("a,b", "','")]
#[case::space("a b", "a space")]
#[case::question("a?b", "'?'")]
#[case::hash("a#b", "'#'")]
#[case::percent("a%2e", "'%'")]
#[case::ampersand("a&b", "'&'")]
#[case::double_quote("a\"b", "'\"'")]
#[case::single_quote("a'b", "\"'\"")]
#[case::colon("a:b", "':'")]
#[case::non_ascii_letter("caf\u{e9}", "U+00E9")]
#[case::control("a\u{1b}b", "U+001B")]
#[case::bidi("a\u{202e}b", "U+202E")]
fn parse_spec_refuses_a_name_character_and_names_it(#[case] text: &str, #[case] shown: &str) {
    let error = parse_spec(Ecosystem::Cargo, text).unwrap_err();

    assert_eq!(
        error,
        format!("the name holds {shown}, which is not allowed")
    );
}

#[rstest]
#[case::slash_outside_npm(Ecosystem::Cargo, "a/b", "the name holds '/', which is not allowed")]
#[case::slash_unscoped_npm(Ecosystem::Npm, "a/b", "the name holds '/', which is not allowed")]
#[case::second_at(
    Ecosystem::Npm,
    "@a/b@c@1.0",
    "the name holds '@', which is not allowed"
)]
#[case::scope_without_name(
    Ecosystem::Npm,
    "@scope",
    "a scoped npm name needs /name after the scope"
)]
#[case::second_slash(Ecosystem::Npm, "@a/b/c", "the name holds '/', which is not allowed")]
#[case::empty_scope(Ecosystem::Npm, "@/name", "the scope after @ is empty")]
#[case::empty_scoped_name(Ecosystem::Npm, "@scope/", "the name after the scope is empty")]
#[case::dot_scoped_name(
    Ecosystem::Npm,
    "@scope/.",
    "the name after the scope cannot be only dots"
)]
#[case::dot_dot_scoped_name(
    Ecosystem::Npm,
    "@scope/..@1.0.0",
    "the name after the scope cannot be only dots"
)]
#[case::scope_in_pypi(
    Ecosystem::Pypi,
    "@scope/name",
    "the name holds '@', which is not allowed"
)]
fn parse_spec_refuses_a_misplaced_at_or_slash(
    #[case] ecosystem: Ecosystem,
    #[case] text: &str,
    #[case] reason: &str,
) {
    let error = parse_spec(ecosystem, text).unwrap_err();

    assert_eq!(error, reason);
}

#[rstest]
#[case::dot(Ecosystem::Cargo, ".hidden")]
#[case::dash(Ecosystem::Cargo, "-flag")]
#[case::underscore(Ecosystem::Cargo, "_private")]
#[case::dot_npm_unscoped(Ecosystem::Npm, ".name")]
#[case::dot_scope(Ecosystem::Npm, "@.scope/name")]
#[case::dash_scoped_name(Ecosystem::Npm, "@scope/-name")]
fn parse_spec_refuses_a_name_that_starts_with_a_symbol(
    #[case] ecosystem: Ecosystem,
    #[case] text: &str,
) {
    let error = parse_spec(ecosystem, text).unwrap_err();

    assert_eq!(error, "the name must start with a letter or a digit");
}

#[rstest]
#[case::dot("@scope/.name")]
#[case::underscore("@scope/_name")]
fn parse_spec_accepts_a_scoped_npm_name_starting_with_dot_or_underscore(#[case] text: &str) {
    let spec = parse_spec(Ecosystem::Npm, text).unwrap();

    assert_eq!(spec.name, text);
}

#[test]
fn parse_spec_takes_names_and_versions_up_to_256_characters() {
    let name = "a".repeat(MAX_NAME_CHARS);
    let version = "1".repeat(MAX_VERSION_CHARS);

    let spec = parse_spec(Ecosystem::Cargo, &format!("{name}@{version}")).unwrap();

    assert_eq!((spec.name, spec.version), (name, Some(version)));
}

#[test]
fn parse_spec_refuses_a_name_of_257_characters() {
    let error = parse_spec(Ecosystem::Cargo, &"a".repeat(MAX_NAME_CHARS + 1)).unwrap_err();

    assert_eq!(error, "the name is longer than 256 characters");
}

#[test]
fn parse_spec_refuses_a_version_of_257_characters() {
    let text = format!("serde@{}", "1".repeat(MAX_VERSION_CHARS + 1));

    let error = parse_spec(Ecosystem::Cargo, &text).unwrap_err();

    assert_eq!(error, "the version is longer than 256 characters");
}

#[rstest]
#[case::build_metadata("1.0.0+build")]
#[case::pep440_epoch("1!2.0")]
#[case::tilde("1.0.0~rc1")]
#[case::v_prefix("v2")]
#[case::prerelease("2.0.0-beta.1")]
fn check_version_accepts_common_version_shapes(#[case] version: &str) {
    assert_eq!(check_version(version), Ok(()));
}

#[rstest]
#[case::dot(".", "the version must start with a letter or a digit")]
#[case::dot_dot("..", "the version must start with a letter or a digit")]
#[case::leading_dot(".1", "the version must start with a letter or a digit")]
#[case::slash("1/2", "the version holds '/', which is not allowed")]
#[case::space("1 2", "the version holds a space, which is not allowed")]
#[case::empty("", "the version is empty")]
fn check_version_refuses_a_bad_version(#[case] version: &str, #[case] reason: &str) {
    assert_eq!(check_version(version), Err(reason.to_owned()));
}

#[test]
fn parse_specs_skips_blank_lines_and_counts_them() {
    let error = parse_specs(Ecosystem::Cargo, ["serde", "", "   ", "bad name"]).unwrap_err();

    assert_eq!(error.line, 4);
}

#[test]
fn parse_specs_keeps_the_first_of_two_duplicates() {
    let specs = parse_specs(Ecosystem::Pypi, ["Foo@1.0", "requests", "foo@1.0"]).unwrap();

    assert_eq!(
        names_and_versions(&specs),
        [("Foo", Some("1.0")), ("requests", None)]
    );
}

#[test]
fn parse_specs_keeps_two_versions_of_one_name() {
    let specs = parse_specs(Ecosystem::Cargo, ["tokio@1.20.0", "tokio@1.38.0", "tokio"]).unwrap();

    assert_eq!(
        names_and_versions(&specs),
        [
            ("tokio", Some("1.20.0")),
            ("tokio", Some("1.38.0")),
            ("tokio", None)
        ]
    );
}

#[test]
fn parse_specs_accepts_1000_packages() {
    let lines: Vec<String> = (0..MAX_PACKAGES).map(|i| format!("crate{i}")).collect();

    let specs = parse_specs(Ecosystem::Cargo, lines.iter().map(String::as_str)).unwrap();

    assert_eq!(specs.len(), MAX_PACKAGES);
}

#[test]
fn parse_specs_refuses_the_1001st_package_on_its_line() {
    let mut lines: Vec<String> = (0..MAX_PACKAGES).map(|i| format!("crate{i}")).collect();
    lines.insert(10, String::new());
    lines.push("crate0".to_owned());
    lines.push("one-too-many".to_owned());

    let error = parse_specs(Ecosystem::Cargo, lines.iter().map(String::as_str)).unwrap_err();

    assert_eq!(error.line, MAX_PACKAGES + 3);
    assert_eq!(
        error.message,
        "more than 1000 packages; check them in smaller lists"
    );
}

#[test]
fn parse_specs_with_only_blank_lines_reports_line_zero() {
    let error = parse_specs(Ecosystem::Cargo, ["", "  "]).unwrap_err();

    assert_eq!(
        error,
        InputIssue {
            line: 0,
            message: "no packages were given".to_owned()
        }
    );
}

#[test]
fn parse_specs_quotes_the_line_on_one_line_cut_at_60_characters() {
    let line = format!("bad\u{7}name{}", "x".repeat(80));

    let error = parse_specs(Ecosystem::Cargo, [line.as_str()]).unwrap_err();

    let expected_quote = format!("\"bad name{}…\"", "x".repeat(52));
    assert_eq!(
        error.message,
        format!("{expected_quote}: the name holds U+0007, which is not allowed")
    );
}

use super::*;
use rstest::rstest;
use serde_json::{Value, json};

fn affected(entries: Value) -> Vec<OsvAffected> {
    serde_json::from_value(entries).unwrap()
}

fn one_range(ecosystem: &str, name: &str, range_type: &str, events: Value) -> Vec<OsvAffected> {
    affected(json!([{
        "package": {"ecosystem": ecosystem, "name": name},
        "ranges": [{"type": range_type, "events": events}]
    }]))
}

fn three_pairs() -> Vec<OsvAffected> {
    one_range(
        "crates.io",
        "tokio",
        "SEMVER",
        json!([
            {"introduced": "0.2.0"}, {"fixed": "1.18.4"},
            {"introduced": "1.19.0"}, {"fixed": "1.20.3"},
            {"introduced": "1.21.0"}, {"fixed": "1.23.1"}
        ]),
    )
}

#[rstest]
#[case::inside_the_second_pair("1.20.2", Verdict::Affected)]
#[case::at_the_second_fix("1.20.3", Verdict::NotAffected)]
#[case::between_the_pairs("1.20.4", Verdict::NotAffected)]
#[case::at_the_third_start("1.21.0", Verdict::Affected)]
#[case::past_the_last_fix("1.23.1", Verdict::NotAffected)]
fn several_pairs_in_one_range_follow_the_schema(#[case] version: &str, #[case] expected: Verdict) {
    let verdict = evaluate(Ecosystem::Cargo, "tokio", version, &three_pairs());

    assert_eq!(verdict, expected);
}

#[rstest]
#[case::any_version_before_the_fix("0.0.1", Verdict::Affected)]
#[case::the_fix("2.0.0", Verdict::NotAffected)]
fn introduced_zero_is_the_bottom(#[case] version: &str, #[case] expected: Verdict) {
    let record = one_range(
        "crates.io",
        "serde",
        "SEMVER",
        json!([{"introduced": "0"}, {"fixed": "2.0.0"}]),
    );

    assert_eq!(
        evaluate(Ecosystem::Cargo, "serde", version, &record),
        expected
    );
}

#[rstest]
#[case::the_last_affected("4.5.0", Verdict::Affected)]
#[case::the_next("4.5.1", Verdict::NotAffected)]
fn last_affected_includes_its_own_version(#[case] version: &str, #[case] expected: Verdict) {
    let record = one_range(
        "npm",
        "lodash.template",
        "SEMVER",
        json!([{"introduced": "0"}, {"last_affected": "4.5.0"}]),
    );

    assert_eq!(
        evaluate(Ecosystem::Npm, "lodash.template", version, &record),
        expected
    );
}

#[rstest]
#[case::below_the_limit("1.9.0", Verdict::Affected)]
#[case::at_the_limit("2.0.0", Verdict::NotAffected)]
fn a_limit_bounds_the_range(#[case] version: &str, #[case] expected: Verdict) {
    let record = one_range(
        "crates.io",
        "crate",
        "SEMVER",
        json!([{"introduced": "1.0.0"}, {"limit": "2.0.0"}]),
    );

    assert_eq!(
        evaluate(Ecosystem::Cargo, "crate", version, &record),
        expected
    );
}

#[rstest]
#[case::listed("6.0.1", Verdict::Affected)]
#[case::not_listed("6.0.2", Verdict::NotAffected)]
fn a_versions_only_entry_matches_its_list(#[case] version: &str, #[case] expected: Verdict) {
    let record = affected(json!([{
        "package": {"ecosystem": "RubyGems", "name": "rest-client"},
        "versions": ["6.0.1"]
    }]));

    assert_eq!(
        evaluate(Ecosystem::Rubygems, "rest-client", version, &record),
        expected
    );
}

#[test]
fn a_nuget_versions_list_matches_under_nuget_equality() {
    let record = affected(json!([{
        "package": {"ecosystem": "NuGet", "name": "Some.Package"},
        "versions": ["1.0.0"]
    }]));

    assert_eq!(
        evaluate(Ecosystem::Nuget, "Some.Package", "1.0", &record),
        Verdict::Affected
    );
}

#[test]
fn introduced_and_last_affected_at_one_version_affect_only_it() {
    let record = one_range(
        "crates.io",
        "crate",
        "SEMVER",
        json!([{"last_affected": "1.0.0"}, {"introduced": "1.0.0"}]),
    );

    let at = evaluate(Ecosystem::Cargo, "crate", "1.0.0", &record);
    let after = evaluate(Ecosystem::Cargo, "crate", "1.0.1", &record);

    assert_eq!((at, after), (Verdict::Affected, Verdict::NotAffected));
}

#[test]
fn an_unparseable_asked_version_against_a_versions_only_entry_is_undecided() {
    let record = affected(json!([{
        "package": {"ecosystem": "npm", "name": "left-pad"},
        "versions": ["1.0.0"]
    }]));

    assert_eq!(
        evaluate(Ecosystem::Npm, "left-pad", "v1.0.0", &record),
        Verdict::Undecided
    );
}

#[test]
fn an_unparseable_event_is_undecided() {
    let record = one_range(
        "crates.io",
        "crate",
        "SEMVER",
        json!([{"introduced": "0"}, {"fixed": "1.x"}]),
    );

    assert_eq!(
        evaluate(Ecosystem::Cargo, "crate", "1.0.0", &record),
        Verdict::Undecided
    );
}

#[test]
fn an_event_with_two_fields_is_undecided() {
    let record = one_range(
        "crates.io",
        "crate",
        "SEMVER",
        json!([{"introduced": "0", "fixed": "1.0.0"}]),
    );

    assert_eq!(
        evaluate(Ecosystem::Cargo, "crate", "0.5.0", &record),
        Verdict::Undecided
    );
}

#[test]
fn an_unknown_range_type_is_undecided() {
    let record = one_range("crates.io", "crate", "CALVER", json!([{"introduced": "0"}]));

    assert_eq!(
        evaluate(Ecosystem::Cargo, "crate", "1.0.0", &record),
        Verdict::Undecided
    );
}

#[test]
fn a_git_only_entry_is_undecided() {
    let record = one_range(
        "crates.io",
        "crate",
        "GIT",
        json!([{"introduced": "0"}, {"fixed": "abc123"}]),
    );

    assert_eq!(
        evaluate(Ecosystem::Cargo, "crate", "1.0.0", &record),
        Verdict::Undecided
    );
}

#[test]
fn a_record_that_names_another_package_is_undecided() {
    let record = one_range(
        "crates.io",
        "other-crate",
        "SEMVER",
        json!([{"introduced": "0"}]),
    );

    assert_eq!(
        evaluate(Ecosystem::Cargo, "crate", "1.0.0", &record),
        Verdict::Undecided
    );
}

#[rstest]
#[case::nuget(Ecosystem::Nuget, "NuGet", "Newtonsoft.Json", "newtonsoft.json")]
#[case::pypi(Ecosystem::Pypi, "PyPI", "django", "Django")]
fn a_re_cased_name_matches_by_the_registry_rule(
    #[case] ecosystem: Ecosystem,
    #[case] osv_name: &str,
    #[case] stored: &str,
    #[case] asked: &str,
) {
    let record = affected(json!([{
        "package": {"ecosystem": osv_name, "name": stored},
        "versions": ["1.0"]
    }]));

    assert_eq!(
        evaluate(ecosystem, asked, "1.0", &record),
        Verdict::Affected
    );
}

#[test]
fn a_nuget_two_part_version_against_a_semver_range_beside_an_ecosystem_one_is_undecided() {
    let record = affected(json!([{
        "package": {"ecosystem": "NuGet", "name": "Pkg"},
        "ranges": [
            {"type": "ECOSYSTEM", "events": [{"introduced": "2.0"}]},
            {"type": "SEMVER", "events": [{"introduced": "0"}, {"fixed": "0.5.0"}]}
        ]
    }]));

    assert_eq!(
        evaluate(Ecosystem::Nuget, "Pkg", "1.0", &record),
        Verdict::Undecided
    );
}

#[test]
fn an_affected_range_beside_an_unreadable_one_is_affected() {
    let record = affected(json!([{
        "package": {"ecosystem": "crates.io", "name": "crate"},
        "ranges": [
            {"type": "CALVER", "events": [{"introduced": "0"}]},
            {"type": "SEMVER", "events": [{"introduced": "0"}, {"fixed": "2.0.0"}]}
        ]
    }]));

    assert_eq!(
        evaluate(Ecosystem::Cargo, "crate", "1.0.0", &record),
        Verdict::Affected
    );
}

#[rstest]
#[case::no_events(json!([]))]
#[case::only_a_fix(json!([{"fixed": "2.0.0"}]))]
#[case::only_a_limit(json!([{"limit": "2.0.0"}]))]
fn a_range_with_no_introduced_event_is_undecided(#[case] events: Value) {
    let record = one_range("crates.io", "crate", "SEMVER", events);

    assert_eq!(
        evaluate(Ecosystem::Cargo, "crate", "1.0.0", &record),
        Verdict::Undecided
    );
}

#[test]
fn an_entry_with_neither_versions_nor_ranges_is_undecided() {
    let record = affected(json!([{"package": {"ecosystem": "npm", "name": "pkg"}}]));

    assert_eq!(
        evaluate(Ecosystem::Npm, "pkg", "1.0.0", &record),
        Verdict::Undecided
    );
}

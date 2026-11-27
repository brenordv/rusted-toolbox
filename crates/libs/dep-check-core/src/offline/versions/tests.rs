use super::*;
use rstest::rstest;
use std::cmp::Ordering::{Equal, Greater, Less};

fn ascending(ordering: Ordering, chain: &[&str]) {
    for pair in chain.windows(2) {
        assert_eq!(
            compare(ordering, pair[0], pair[1]),
            Some(Less),
            "{} < {}",
            pair[0],
            pair[1]
        );
        assert_eq!(compare(ordering, pair[1], pair[0]), Some(Greater));
    }
}

#[test]
fn semver_follows_the_spec_s_precedence_chain() {
    ascending(
        Ordering::SemVer,
        &[
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-alpha.beta",
            "1.0.0-beta",
            "1.0.0-beta.2",
            "1.0.0-beta.11",
            "1.0.0-rc.1",
            "1.0.0",
        ],
    );
}

#[test]
fn semver_ignores_build_metadata() {
    assert_eq!(
        compare(Ordering::SemVer, "1.0.0+build.1", "1.0.0+build.2"),
        Some(Equal)
    );
}

#[rstest]
#[case::leading_v("v1.0.0")]
#[case::four_parts("1.2.3.4")]
#[case::two_parts("1.0")]
fn semver_refuses_what_the_spec_does_not_allow(#[case] text: &str) {
    assert!(semver_key(text).is_none());
}

#[test]
fn nuget_follows_the_docs_descending_list() {
    let descending = [
        "1.0.1",
        "1.0.1-zzz",
        "1.0.1-rc.10",
        "1.0.1-rc.2",
        "1.0.1-open",
        "1.0.1-beta",
        "1.0.1-alpha2",
        "1.0.1-alpha10",
        "1.0.1-aaa",
    ];
    let mut chain = descending.to_vec();
    chain.reverse();

    ascending(Ordering::NuGet, &chain);
}

#[rstest]
#[case::missing_parts_are_zero("1", "1.0.0.0")]
#[case::two_and_three_parts("1.0", "1.0.0")]
#[case::label_case("1.0.0-Alpha", "1.0.0-alpha")]
#[case::build_metadata("1.0.7+r3456", "1.0.7")]
#[case::leading_zeros("1.01.1", "1.1.1")]
fn nuget_equal_versions_compare_equal(#[case] a: &str, #[case] b: &str) {
    assert_eq!(compare(Ordering::NuGet, a, b), Some(Equal));
}

#[rstest]
#[case::five_parts("1.0.0.0.0")]
#[case::leading_v("v1.0.0")]
#[case::empty_label("1.0.0-")]
#[case::empty_part("1..0")]
fn nuget_refuses_what_it_cannot_read(#[case] text: &str) {
    assert!(nuget_key(text).is_none());
}

#[test]
fn pep440_follows_the_specification_s_summary_order() {
    ascending(
        Ordering::Pep440,
        &[
            "1.dev0",
            "1.0.dev456",
            "1.0a1",
            "1.0a2.dev456",
            "1.0a12.dev456",
            "1.0a12",
            "1.0b1.dev456",
            "1.0b2",
            "1.0b2.post345.dev456",
            "1.0b2.post345",
            "1.0rc1.dev456",
            "1.0rc1",
            "1.0",
            "1.0+abc.5",
            "1.0+abc.7",
            "1.0+5",
            "1.0.post456.dev34",
            "1.0.post456",
            "1.0.15",
            "1.1.dev1",
        ],
    );
}

#[rstest]
#[case::release_padding("1.0", "1.0.0")]
#[case::leading_v("v1.0", "1.0")]
fn pep440_equal_versions_compare_equal(#[case] a: &str, #[case] b: &str) {
    assert_eq!(compare(Ordering::Pep440, a, b), Some(Equal));
}

#[test]
fn pep440_sorts_a_local_version_above_its_release() {
    assert_eq!(compare(Ordering::Pep440, "1.0+local", "1.0"), Some(Greater));
}

#[test]
fn pep440_refuses_a_legacy_version() {
    assert!(pep440_key("2004d").is_none());
}

#[test]
fn rubygems_follows_the_class_doc_s_order() {
    ascending(Ordering::RubyGems, &["0.9", "1.0.a.2", "1.0.b1", "1.0"]);
}

#[test]
fn rubygems_splits_letters_from_numbers() {
    assert_eq!(
        compare(Ordering::RubyGems, "1.0.a10", "1.0.a9"),
        Some(Greater)
    );
}

#[rstest]
#[case::trailing_zero("1.0", "1")]
#[case::dash_reads_as_pre("1.0-pre", "1.0.pre.pre")]
#[case::leading_zeros("1.01", "1.1")]
fn rubygems_equal_versions_compare_equal(#[case] a: &str, #[case] b: &str) {
    assert_eq!(compare(Ordering::RubyGems, a, b), Some(Equal));
}

#[rstest]
#[case::a_release_above_its_prerelease("1.0", "1.0.pre", Greater)]
#[case::a_number_above_a_letter("1.1", "1.a", Greater)]
#[case::a_longer_release("1.0.1", "1.0", Greater)]
#[case::upper_case_sorts_first("1.0.B", "1.0.a", Less)]
fn rubygems_orders_by_segments(
    #[case] a: &str,
    #[case] b: &str,
    #[case] expected: std::cmp::Ordering,
) {
    assert_eq!(compare(Ordering::RubyGems, a, b), Some(expected));
}

#[rstest]
#[case::empty("")]
#[case::leading_letter("a1")]
#[case::leading_v("v1.0")]
#[case::space("1.0 beta")]
#[case::empty_segment("1..0")]
#[case::trailing_dash("1.0-")]
fn rubygems_refuses_what_version_pattern_refuses(#[case] text: &str) {
    assert!(rubygems_key(text).is_none());
}

#[rstest]
#[case::semver_range(Ecosystem::Nuget, "SEMVER", RangeOrdering::By(Ordering::SemVer))]
#[case::cargo_ecosystem(Ecosystem::Cargo, "ECOSYSTEM", RangeOrdering::By(Ordering::SemVer))]
#[case::npm_ecosystem(Ecosystem::Npm, "ECOSYSTEM", RangeOrdering::By(Ordering::SemVer))]
#[case::nuget_ecosystem(Ecosystem::Nuget, "ECOSYSTEM", RangeOrdering::By(Ordering::NuGet))]
#[case::pypi_ecosystem(Ecosystem::Pypi, "ECOSYSTEM", RangeOrdering::By(Ordering::Pep440))]
#[case::rubygems_ecosystem(
    Ecosystem::Rubygems,
    "ECOSYSTEM",
    RangeOrdering::By(Ordering::RubyGems)
)]
#[case::git(Ecosystem::Cargo, "GIT", RangeOrdering::Skip)]
#[case::unknown(Ecosystem::Cargo, "CALVER", RangeOrdering::Unknown)]
fn ordering_for_follows_the_range_type(
    #[case] ecosystem: Ecosystem,
    #[case] range_type: &str,
    #[case] expected: RangeOrdering,
) {
    assert_eq!(ordering_for(ecosystem, range_type), expected);
}

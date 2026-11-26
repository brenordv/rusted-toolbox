use super::*;
use crate::http::SourceFailure;

fn advisory(source: SourceName, id: &str, aliases: &[&str]) -> SourceAdvisory {
    SourceAdvisory {
        aliases: aliases.iter().map(|alias| (*alias).to_owned()).collect(),
        ..SourceAdvisory::id_only(source, id)
    }
}

fn scored(source: SourceName, id: &str, score: f64) -> SourceAdvisory {
    SourceAdvisory {
        cvss: vec![Cvss {
            version: "3".to_owned(),
            vector: Some(format!("CVSS:3.1/{id}")),
            score: Some(score),
            from: source,
        }],
        ..SourceAdvisory::id_only(source, id)
    }
}

fn answered(advisories: Vec<SourceAdvisory>) -> PackageAnswer {
    PackageAnswer::Answered { advisories }
}

fn failed(failure: SourceFailure, advisories: Vec<SourceAdvisory>) -> PackageAnswer {
    PackageAnswer::Failed {
        failure,
        advisories,
    }
}

fn finding_ids(verdict: &PackageVerdict) -> Vec<&str> {
    verdict
        .findings
        .iter()
        .map(|finding| finding.id.as_str())
        .collect()
}

#[test]
fn advisories_sharing_an_alias_are_one_finding() {
    let osv = answered(vec![advisory(SourceName::Osv, "RUSTSEC-1", &["CVE-1"])]);
    let github = answered(vec![advisory(SourceName::GitHub, "GHSA-1", &["CVE-1"])]);

    let verdict = merge(&[(SourceName::Osv, &osv), (SourceName::GitHub, &github)]);

    assert_eq!(finding_ids(&verdict), ["RUSTSEC-1"]);
    assert_eq!(verdict.findings[0].aliases, ["CVE-1", "GHSA-1"]);
    assert_eq!(
        verdict.findings[0].sources,
        [SourceName::Osv, SourceName::GitHub]
    );
}

#[test]
fn a_chain_of_shared_ids_is_one_finding() {
    let osv = answered(vec![
        advisory(SourceName::Osv, "A", &["B"]),
        advisory(SourceName::Osv, "D", &[]),
    ]);
    let github = answered(vec![advisory(SourceName::GitHub, "C", &["B"])]);
    let deps_dev = answered(vec![advisory(SourceName::DepsDev, "E", &["C"])]);

    let verdict = merge(&[
        (SourceName::Osv, &osv),
        (SourceName::GitHub, &github),
        (SourceName::DepsDev, &deps_dev),
    ]);

    assert_eq!(finding_ids(&verdict), ["A", "D"]);
    assert_eq!(verdict.findings[0].aliases, ["B", "C", "E"]);
}

#[test]
fn the_headline_is_github_s_id_without_an_osv_node() {
    let deps_dev = answered(vec![advisory(SourceName::DepsDev, "DD-1", &["GHSA-1"])]);
    let github = answered(vec![advisory(SourceName::GitHub, "GHSA-1", &[])]);

    let verdict = merge(&[
        (SourceName::DepsDev, &deps_dev),
        (SourceName::GitHub, &github),
    ]);

    assert_eq!(finding_ids(&verdict), ["GHSA-1"]);
}

#[test]
fn the_headline_is_deps_dev_s_id_when_it_is_alone() {
    let deps_dev = answered(vec![advisory(SourceName::DepsDev, "GHSA-2", &[])]);

    let verdict = merge(&[(SourceName::DepsDev, &deps_dev)]);

    assert_eq!(finding_ids(&verdict), ["GHSA-2"]);
}

#[test]
fn a_github_label_beats_a_higher_score() {
    let osv = answered(vec![scored(SourceName::Osv, "X-1", 9.8)]);
    let github = answered(vec![SourceAdvisory {
        severity: Some(Severity::Medium),
        ..advisory(SourceName::GitHub, "GHSA-1", &["X-1"])
    }]);

    let verdict = merge(&[(SourceName::Osv, &osv), (SourceName::GitHub, &github)]);

    assert_eq!(verdict.findings[0].severity, Severity::Medium);
}

#[test]
fn without_a_label_the_highest_score_decides() {
    let osv = answered(vec![scored(SourceName::Osv, "X-1", 5.0)]);
    let deps_dev = answered(vec![SourceAdvisory {
        aliases: vec!["X-1".to_owned()],
        ..scored(SourceName::DepsDev, "Y-1", 7.5)
    }]);

    let verdict = merge(&[(SourceName::Osv, &osv), (SourceName::DepsDev, &deps_dev)]);

    assert_eq!(verdict.findings[0].severity, Severity::High);
    assert_eq!(verdict.findings[0].cvss.len(), 2);
}

#[test]
fn without_a_label_or_a_score_the_severity_is_unknown() {
    let osv = answered(vec![advisory(SourceName::Osv, "X-1", &[])]);

    let verdict = merge(&[(SourceName::Osv, &osv)]);

    assert_eq!(verdict.findings[0].severity, Severity::Unknown);
}

#[test]
fn the_same_vector_from_two_sources_is_kept_once_and_lists_merge_in_order() {
    let mut first = scored(SourceName::Osv, "X-1", 5.0);
    first.fixed_in = vec!["1.2.0".to_owned(), "2.0.1".to_owned()];
    first.urls = vec!["https://a.example".to_owned()];
    let mut second = scored(SourceName::DepsDev, "X-1", 5.0);
    second.cvss[0].score = None;
    second.fixed_in = vec!["2.0.1".to_owned(), "3.0.0".to_owned()];
    second.urls = vec![
        "https://b.example".to_owned(),
        "https://a.example".to_owned(),
    ];
    let osv = answered(vec![first]);
    let deps_dev = answered(vec![second]);

    let verdict = merge(&[(SourceName::Osv, &osv), (SourceName::DepsDev, &deps_dev)]);

    let finding = &verdict.findings[0];
    assert_eq!(finding.cvss.len(), 1);
    assert_eq!(finding.fixed_in, ["1.2.0", "2.0.1", "3.0.0"]);
    assert_eq!(finding.urls, ["https://a.example", "https://b.example"]);
}

#[test]
fn the_summary_prefers_github_then_osv() {
    let osv = answered(vec![SourceAdvisory {
        summary: Some("from osv".to_owned()),
        ..advisory(SourceName::Osv, "X-1", &[])
    }]);
    let github = answered(vec![SourceAdvisory {
        summary: Some("from github".to_owned()),
        ..advisory(SourceName::GitHub, "GHSA-1", &["X-1"])
    }]);

    let verdict = merge(&[(SourceName::Osv, &osv), (SourceName::GitHub, &github)]);

    assert_eq!(verdict.findings[0].summary.as_deref(), Some("from github"));
}

#[test]
fn any_finding_makes_the_package_vulnerable_even_when_deps_dev_does_not_know_it() {
    let osv = answered(vec![advisory(SourceName::Osv, "MAL-2026-1", &[])]);

    let verdict = merge(&[
        (SourceName::DepsDev, &PackageAnswer::NotFound),
        (SourceName::Osv, &osv),
    ]);

    assert_eq!(verdict.status, PackageStatus::Vulnerable);
    assert_eq!(verdict.findings[0].kind, AdvisoryKind::Malware);
}

#[test]
fn an_advisory_inside_a_failed_answer_still_makes_the_package_vulnerable() {
    let osv = failed(
        SourceFailure::TimedOut,
        vec![advisory(SourceName::Osv, "X-1", &[])],
    );

    let verdict = merge(&[(SourceName::Osv, &osv)]);

    assert_eq!(verdict.status, PackageStatus::Vulnerable);
    assert_eq!(verdict.reason, None);
}

#[test]
fn deps_dev_not_found_with_osv_empty_is_not_found() {
    let osv = answered(Vec::new());

    let verdict = merge(&[
        (SourceName::Osv, &osv),
        (SourceName::DepsDev, &PackageAnswer::NotFound),
    ]);

    assert_eq!(verdict.status, PackageStatus::NotFound);
}

#[test]
fn an_empty_answer_is_clean() {
    let osv = answered(Vec::new());

    let verdict = merge(&[(SourceName::Osv, &osv)]);

    assert_eq!(
        verdict,
        PackageVerdict {
            status: PackageStatus::Clean,
            reason: None,
            findings: Vec::new()
        }
    );
}

#[test]
fn every_source_failed_is_not_checked_with_each_reason() {
    let osv = failed(SourceFailure::TimedOut, Vec::new());
    let github = failed(SourceFailure::Status { code: 502 }, Vec::new());

    let verdict = merge(&[(SourceName::Osv, &osv), (SourceName::GitHub, &github)]);

    assert_eq!(verdict.status, PackageStatus::NotChecked);
    assert_eq!(
        verdict.reason.as_deref(),
        Some("OSV.dev did not answer in time; GitHub Advisory Database answered 502")
    );
}

#[test]
fn findings_sort_by_severity_then_malware_then_id() {
    let osv = answered(vec![
        scored(SourceName::Osv, "B-low", 2.0),
        scored(SourceName::Osv, "Z-high", 8.0),
        SourceAdvisory {
            kind: AdvisoryKind::Malware,
            ..scored(SourceName::Osv, "Y-high-malware", 8.0)
        },
        scored(SourceName::Osv, "A-high", 8.0),
    ]);

    let verdict = merge(&[(SourceName::Osv, &osv)]);

    assert_eq!(
        finding_ids(&verdict),
        ["Y-high-malware", "A-high", "Z-high", "B-low"]
    );
}

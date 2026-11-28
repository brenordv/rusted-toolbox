use super::*;
use crate::advisory::{Severity, SourceName};
use crate::ecosystem::Ecosystem;
use crate::report::{
    AnswerWord, Attestations, Outcome, REPORT_VERSION, ScorecardSummary, SourceRepo, SourceStatus,
    Summary,
};
use rstest::rstest;
use std::collections::BTreeMap;

/// A fixed time: the rendered dates are the behavior under test.
fn at(text: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(text)
        .unwrap()
        .with_timezone(&Utc)
}

fn answered(source: SourceName) -> SourceReport {
    SourceReport {
        source,
        status: SourceStatus::Answered {
            blocked_until: None,
        },
        requests: 1,
        failures: Vec::new(),
        details_missing: 0,
        data_as_of: None,
    }
}

fn tokio_finding() -> Finding {
    Finding {
        id: "RUSTSEC-2023-0005".to_owned(),
        aliases: vec![
            "CVE-2023-22466".to_owned(),
            "GHSA-7rrj-xr53-82p7".to_owned(),
        ],
        kind: AdvisoryKind::Vulnerability,
        severity: Severity::Medium,
        summary: Some("tokio::io::ReadHalf<T>::unsplit is unsound".to_owned()),
        cvss: vec![Cvss {
            version: "3".to_owned(),
            vector: Some("CVSS:3.1/AV:N".to_owned()),
            score: Some(5.4),
            from: SourceName::DepsDev,
        }],
        epss: None,
        fixed_in: vec![
            "1.18.4".to_owned(),
            "1.20.3".to_owned(),
            "1.23.1".to_owned(),
        ],
        urls: vec!["https://rustsec.org/advisories/RUSTSEC-2023-0005.html".to_owned()],
        sources: vec![SourceName::Osv, SourceName::DepsDev],
    }
}

fn chain(licenses: &str, repo: &str, score: f64, published: &str, latest: bool) -> SupplyChain {
    SupplyChain {
        latest,
        deprecated: None,
        licenses: vec![licenses.to_owned()],
        published_at: Some(at(published)),
        source_repo: Some(SourceRepo {
            id: repo.to_owned(),
            provenance: "unverified_metadata".to_owned(),
        }),
        scorecard: Some(ScorecardSummary {
            score,
            date: Some(at("2026-11-17T00:00:00Z")),
        }),
        attestations: Attestations::default(),
    }
}

fn package(name: &str, version: Option<&str>, status: PackageStatus) -> PackageReport {
    PackageReport {
        input: name.to_owned(),
        name: name.to_owned(),
        version: version.map(str::to_owned),
        version_from: version.map(|_| VersionFrom::Input),
        status,
        reason: None,
        findings: Vec::new(),
        supply_chain: None,
        answers: BTreeMap::from([(SourceName::Osv, AnswerWord::Answered)]),
    }
}

fn three_packages() -> Report {
    let tokio = PackageReport {
        findings: vec![tokio_finding()],
        supply_chain: Some(chain(
            "MIT",
            "github.com/tokio-rs/tokio",
            8.1,
            "2022-07-12T00:00:00Z",
            false,
        )),
        ..package("tokio", Some("1.20.0"), PackageStatus::Vulnerable)
    };
    let serde = PackageReport {
        version_from: Some(VersionFrom::DepsDevLatest),
        supply_chain: Some(chain(
            "MIT OR Apache-2.0",
            "github.com/serde-rs/serde",
            7.6,
            "2024-11-11T00:00:00Z",
            true,
        )),
        ..package("serde", Some("1.0.215"), PackageStatus::Clean)
    };
    let left_padd = package("left-padd", None, PackageStatus::NotFound);
    report_of(vec![tokio, serde, left_padd])
}

fn report_of(packages: Vec<PackageReport>) -> Report {
    Report {
        report_version: REPORT_VERSION,
        tool: "dep-check 1.0.0".to_owned(),
        checked_at: at("2026-11-26T13:05:00Z"),
        ecosystem: Ecosystem::Cargo,
        offline: crate::offline::OfflineUse::Off,
        outcome: Outcome::Vulnerable,
        summary: Summary::of(&packages),
        sources: vec![answered(SourceName::Osv), answered(SourceName::DepsDev)],
        packages,
    }
}

const THREE_PACKAGES_TEXT: &str = "\
dep-check 1.0.0  Cargo  3 packages: 1 vulnerable, 1 clean, 1 not found
OSV.dev answered; deps.dev answered

tokio 1.20.0  vulnerable, 1 advisory
  RUSTSEC-2023-0005  medium  fixed in 1.18.4, 1.20.3, 1.23.1
    tokio::io::ReadHalf<T>::unsplit is unsound
    also CVE-2023-22466, GHSA-7rrj-xr53-82p7
    CVSS 3 5.4 (deps.dev). From OSV.dev, deps.dev
    https://rustsec.org/advisories/RUSTSEC-2023-0005.html
  MIT. Source github.com/tokio-rs/tokio (unverified metadata). Scorecard 8.1 (2026-11-17). Published 2022-07-12. Not the latest version.

serde 1.0.215 (latest, from deps.dev)  clean
  MIT OR Apache-2.0. Source github.com/serde-rs/serde (unverified metadata). Scorecard 7.6 (2026-11-17). Published 2024-11-11.

left-padd  not found: deps.dev does not know this package
";

#[test]
fn a_three_package_report_reads_as_the_documented_layout() {
    let text = text_report(&three_packages());

    assert_eq!(text, THREE_PACKAGES_TEXT);
}

#[rstest]
#[case::zero(0, "0 packages")]
#[case::one(1, "1 package")]
#[case::two(2, "2 packages")]
#[case::thousand(1000, "1,000 packages")]
#[case::many(12345, "12,345 packages")]
fn count_phrase_groups_thousands_and_picks_the_noun(#[case] count: u64, #[case] text: &str) {
    assert_eq!(count_phrase(count, "package", "packages"), text);
}

#[test]
fn a_malware_finding_says_to_remove_the_package() {
    let finding = Finding {
        id: "MAL-2024-1234".to_owned(),
        kind: AdvisoryKind::Malware,
        fixed_in: Vec::new(),
        ..tokio_finding()
    };

    let lines = finding_lines(&finding);

    assert_eq!(
        lines.head,
        "MAL-2024-1234  malware  no fixed version: remove the package"
    );
}

#[test]
fn a_vulnerability_with_no_fix_says_so() {
    let finding = Finding {
        fixed_in: Vec::new(),
        ..tokio_finding()
    };

    assert_eq!(
        finding_lines(&finding).head,
        "RUSTSEC-2023-0005  medium  no fixed version"
    );
}

#[test]
fn a_not_checked_package_shows_its_reason() {
    let package = PackageReport {
        reason: Some("OSV.dev did not answer in time".to_owned()),
        ..package("serde", Some("1.0.215"), PackageStatus::NotChecked)
    };

    assert_eq!(
        package_line(&package),
        "serde 1.0.215  not checked: OSV.dev did not answer in time"
    );
}

#[test]
fn a_version_deps_dev_does_not_know_says_version() {
    let package = package("serde", Some("0.0.1"), PackageStatus::NotFound);

    assert_eq!(
        package_line(&package),
        "serde 0.0.1  not found: deps.dev does not know this version"
    );
}

const HOSTILE: &str = "a\u{1b}[2J\nb\u{202e}c";

fn with_hostile(field: &str) -> Report {
    let mut report = three_packages();
    let tokio = &mut report.packages[0];
    let finding = &mut tokio.findings[0];
    let chain = tokio.supply_chain.as_mut().unwrap();
    let hostile = HOSTILE.to_owned();
    match field {
        "summary" => finding.summary = Some(hostile),
        "id" => finding.id = hostile,
        "alias" => finding.aliases[0] = hostile,
        "fixed" => finding.fixed_in[0] = hostile,
        "url" => finding.urls[0] = format!("https://{hostile}"),
        "vector" => {
            finding.cvss[0].score = None;
            finding.cvss[0].vector = Some(hostile);
        }
        "license" => chain.licenses[0] = hostile,
        "repo" => chain.source_repo.as_mut().unwrap().id = hostile,
        "provenance" => chain.source_repo.as_mut().unwrap().provenance = hostile,
        "deprecation" => chain.deprecated = Some(hostile),
        "version" => tokio.version = Some(hostile),
        other => panic!("no field {other}"),
    }
    report
}

#[rstest]
#[case::summary("summary")]
#[case::id("id")]
#[case::alias("alias")]
#[case::fixed_version("fixed")]
#[case::url("url")]
#[case::cvss_vector("vector")]
#[case::license("license")]
#[case::repo_id("repo")]
#[case::provenance("provenance")]
#[case::deprecation_reason("deprecation")]
#[case::resolved_version("version")]
fn an_outside_field_with_control_characters_stays_on_its_line(#[case] field: &str) {
    let clean_lines = text_report(&three_packages()).lines().count();

    let text = text_report(&with_hostile(field));

    assert!(!text.contains('\u{1b}'), "{text}");
    assert!(!text.contains('\u{202e}'), "{text}");
    assert!(text.contains("a [2J b c"), "{text}");
    assert_eq!(text.lines().count(), clean_lines);
}

#[test]
fn a_long_summary_is_cut_at_140_characters() {
    let finding = Finding {
        summary: Some("s".repeat(2000)),
        ..tokio_finding()
    };

    let summary = finding_lines(&finding).summary.unwrap();

    assert_eq!(summary, format!("{}…", "s".repeat(140)));
}

#[test]
fn a_link_that_is_not_https_is_left_out() {
    let finding = Finding {
        urls: vec![
            "http://plain.example".to_owned(),
            "file:///etc/passwd".to_owned(),
            "https://kept.example".to_owned(),
        ],
        ..tokio_finding()
    };

    assert_eq!(finding_lines(&finding).urls, ["https://kept.example"]);
}

#[test]
fn the_json_keeps_the_documented_field_names() {
    let json = serde_json::to_value(three_packages()).unwrap();

    assert_eq!(json["report_version"], 1);
    assert_eq!(json["outcome"], "vulnerable");
    assert_eq!(json["sources"][1]["source"], "deps_dev");
    assert_eq!(json["packages"][0]["status"], "vulnerable");
    assert_eq!(
        json["packages"][0]["findings"][0]["id"],
        "RUSTSEC-2023-0005"
    );
    assert_eq!(
        json["packages"][1]["supply_chain"]["source_repo"]["provenance"],
        "unverified_metadata"
    );
    assert!(json["packages"][2]["supply_chain"].is_null());
}

#[test]
fn a_supply_chain_with_a_deprecation_and_attestations_names_both() {
    let chain = SupplyChain {
        deprecated: Some("use other".to_owned()),
        attestations: Attestations {
            total: 2,
            verified: 1,
        },
        licenses: Vec::new(),
        source_repo: None,
        scorecard: None,
        published_at: None,
        latest: true,
    };

    assert_eq!(
        supply_chain_line(&chain),
        "No license listed. Deprecated: use other. 2 attestations, 1 verified."
    );
}

#[test]
fn a_report_with_no_packages_counts_none() {
    let report = report_of(Vec::new());

    assert_eq!(header_line(&report), "dep-check 1.0.0  Cargo  0 packages");
}

#[test]
fn a_deprecation_without_a_reason_and_a_scorecard_without_a_date_read_plainly() {
    let chain = SupplyChain {
        deprecated: Some(String::new()),
        scorecard: Some(ScorecardSummary {
            score: 4.25,
            date: None,
        }),
        source_repo: None,
        published_at: None,
        ..chain("MIT", "x", 0.0, "2024-01-01T00:00:00Z", true)
    };

    assert_eq!(supply_chain_line(&chain), "MIT. Deprecated. Scorecard 4.2.");
}

#[rstest]
#[case::vector_only(None, Some("CVSS:3.1/AV:N"), "CVSS 3 CVSS:3.1/AV:N (OSV.dev)")]
#[case::unrated(None, None, "CVSS 3 unrated (OSV.dev)")]
fn a_rating_without_a_score_shows_its_vector_or_says_unrated(
    #[case] score: Option<f64>,
    #[case] vector: Option<&str>,
    #[case] phrase: &str,
) {
    let finding = Finding {
        cvss: vec![Cvss {
            version: "3".to_owned(),
            vector: vector.map(str::to_owned),
            score,
            from: SourceName::Osv,
        }],
        sources: vec![SourceName::Osv],
        ..tokio_finding()
    };

    assert_eq!(
        finding_lines(&finding).scores,
        format!("{phrase}. From OSV.dev")
    );
}

#[test]
fn a_not_checked_package_with_no_reason_says_so() {
    let package = package("serde", Some("1.0.0"), PackageStatus::NotChecked);

    assert_eq!(package_line(&package), "serde 1.0.0  not checked");
}

#[test]
fn a_source_phrase_cleans_the_failure_detail() {
    let source = SourceReport {
        status: SourceStatus::Failed {
            failure: crate::http::SourceFailure::Unreachable {
                detail: "dns\nerror".to_owned(),
            },
        },
        ..answered(SourceName::Osv)
    };

    assert_eq!(
        source_phrase(&source),
        "OSV.dev could not be reached: dns error"
    );
}

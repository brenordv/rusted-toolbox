//! The report one check produces: per-package verdicts, per-source outcomes, and the overall
//! outcome. Serialized with serde, field names in `snake_case`.

use crate::advisory::{AdvisoryKind, Cvss, Epss, Severity, SourceName};
use crate::ecosystem::Ecosystem;
use crate::http::SourceFailure;
use crate::offline::OfflineUse;
use crate::render::count_phrase;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::BTreeMap;

/// The version of the report's JSON shape.
pub const REPORT_VERSION: u32 = 1;

/// Everything one check found.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub report_version: u32,
    /// The tool and its version, as "dep-check 1.0.0".
    pub tool: String,
    pub checked_at: DateTime<Utc>,
    pub ecosystem: Ecosystem,
    /// How the check used offline data.
    pub offline: OfflineUse,
    pub outcome: Outcome,
    pub summary: Summary,
    pub sources: Vec<SourceReport>,
    pub packages: Vec<PackageReport>,
}

/// The check's verdict over every package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// Every package was checked and nothing was found.
    Clean,
    /// At least one package has a finding.
    Vulnerable,
    /// Nothing was found, but some package was not found or not checked.
    Incomplete,
    /// No package was checked.
    Failed,
}

impl Outcome {
    /// The outcome the package statuses add up to: `Failed` when every package is
    /// `NotChecked`, else `Vulnerable` when any is, else `Incomplete` when any is `NotFound` or
    /// `NotChecked`, else `Clean`.
    pub fn from_statuses(statuses: &[PackageStatus]) -> Outcome {
        if statuses
            .iter()
            .all(|status| *status == PackageStatus::NotChecked)
        {
            return Outcome::Failed;
        }
        let mut outcome = Outcome::Clean;
        for status in statuses {
            match status {
                PackageStatus::Vulnerable => return Outcome::Vulnerable,
                PackageStatus::NotFound | PackageStatus::NotChecked => {
                    outcome = Outcome::Incomplete;
                }
                PackageStatus::Clean => {}
            }
        }
        outcome
    }

    /// The outcome's word.
    pub fn name(self) -> &'static str {
        match self {
            Outcome::Clean => "clean",
            Outcome::Vulnerable => "vulnerable",
            Outcome::Incomplete => "incomplete",
            Outcome::Failed => "failed",
        }
    }
}

/// One package's verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageStatus {
    /// At least one finding, from any source.
    Vulnerable,
    /// deps.dev does not know the package or version, and nothing has a finding.
    NotFound,
    /// A source answered and nothing has a finding.
    Clean,
    /// No source answered.
    NotChecked,
}

impl PackageStatus {
    /// The status's word.
    pub fn name(self) -> &'static str {
        match self {
            PackageStatus::Vulnerable => "vulnerable",
            PackageStatus::NotFound => "not_found",
            PackageStatus::Clean => "clean",
            PackageStatus::NotChecked => "not_checked",
        }
    }
}

/// How many packages ended in each status, and how many findings there are in all.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Summary {
    pub packages: usize,
    pub vulnerable: usize,
    pub clean: usize,
    pub not_found: usize,
    pub not_checked: usize,
    pub findings: usize,
}

impl Summary {
    /// The counts over `packages`.
    pub fn of(packages: &[PackageReport]) -> Summary {
        let mut summary = Summary {
            packages: packages.len(),
            ..Summary::default()
        };
        for package in packages {
            summary.findings += package.findings.len();
            match package.status {
                PackageStatus::Vulnerable => summary.vulnerable += 1,
                PackageStatus::NotFound => summary.not_found += 1,
                PackageStatus::Clean => summary.clean += 1,
                PackageStatus::NotChecked => summary.not_checked += 1,
            }
        }
        summary
    }
}

/// Where a package's version came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionFrom {
    /// The user wrote it.
    Input,
    /// deps.dev's latest version, for a package written without one.
    DepsDevLatest,
}

/// One package in the report.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PackageReport {
    /// The text the user wrote for it.
    pub input: String,
    pub name: String,
    /// The version checked; `None` when none was found.
    pub version: Option<String>,
    pub version_from: Option<VersionFrom>,
    pub status: PackageStatus,
    /// Why the package was not checked; only on a `not_checked` package.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub findings: Vec<Finding>,
    /// deps.dev's facts about the version; `None` when deps.dev gave none.
    pub supply_chain: Option<SupplyChain>,
    /// What each source said about this package.
    pub answers: BTreeMap<SourceName, AnswerWord>,
}

/// deps.dev's facts about one package version. They never change a status. The deprecation
/// reason, licenses, repository id, and provenance word come from the package's publisher and
/// are kept as given.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SupplyChain {
    /// The version is the one deps.dev installs by default.
    pub latest: bool,
    /// `Some(reason)` when the version is deprecated; an empty reason when none was given.
    pub deprecated: Option<String>,
    /// SPDX expressions, or "non-standard" for a license with no SPDX identifier.
    pub licenses: Vec<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub source_repo: Option<SourceRepo>,
    /// The source repository's OpenSSF Scorecard, when deps.dev has one.
    pub scorecard: Option<ScorecardSummary>,
    pub attestations: Attestations,
}

/// The repository a version says it was built from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceRepo {
    /// As "github.com/tokio-rs/tokio".
    pub id: String,
    /// How deps.dev knows the link, lowercased: "unverified_metadata", "slsa_attestation", ...
    pub provenance: String,
}

/// An OpenSSF Scorecard's overall score and when it was computed.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScorecardSummary {
    pub score: f64,
    pub date: Option<DateTime<Utc>>,
}

/// Build attestations and SLSA provenances counted together, and how many deps.dev verified.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Attestations {
    pub total: u32,
    pub verified: u32,
}

/// One source's answer for one package, in a word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AnswerWord {
    Answered,
    NotFound,
    Failed,
    RateLimited,
    NotAsked,
}

/// One advisory after merging what every source said about it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Finding {
    /// The headline id: OSV's when OSV knows it, else GitHub's, else deps.dev's.
    pub id: String,
    /// Every other id of the advisory, sorted.
    pub aliases: Vec<String>,
    pub kind: AdvisoryKind,
    pub severity: Severity,
    pub summary: Option<String>,
    pub cvss: Vec<Cvss>,
    pub epss: Option<Epss>,
    pub fixed_in: Vec<String>,
    pub urls: Vec<String>,
    /// The sources that reported it, sorted.
    pub sources: Vec<SourceName>,
}

/// What one source did in this check.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SourceReport {
    pub source: SourceName,
    #[serde(flatten)]
    pub status: SourceStatus,
    /// Requests sent.
    pub requests: u32,
    /// Why packages went unanswered, one entry per distinct reason.
    pub failures: Vec<FailureGroup>,
    /// Details that could not be fetched: an advisory known by its id only, or a source
    /// repository whose scorecard request failed.
    pub details_missing: u32,
    /// For offline data, when it was last confirmed current; `None` for every other source.
    pub data_as_of: Option<DateTime<Utc>>,
}

impl SourceReport {
    /// The source's outcome as the rest of a sentence whose subject is the source: "OSV.dev
    /// answered some packages; for the others it did not answer in time (2 packages)".
    pub fn describe(&self) -> String {
        match &self.status {
            SourceStatus::Answered {
                blocked_until: None,
            } => "answered".to_owned(),
            SourceStatus::Answered {
                blocked_until: Some(until),
            } => format!(
                "answered, then was rate limited until {}",
                display_time(*until)
            ),
            SourceStatus::Partial {
                blocked_until: Some(until),
            } => format!(
                "answered some packages, then was rate limited until {}",
                display_time(*until)
            ),
            SourceStatus::Partial {
                blocked_until: None,
            } => format!(
                "answered some packages; for the others it {}",
                self.joined_reasons(" and ")
            ),
            SourceStatus::Failed { failure } => failure.describe(),
            SourceStatus::RateLimited {
                until,
                origin: BlockOrigin::Saved,
            } => format!(
                "is rate limited until {} (from an earlier run)",
                display_time(*until)
            ),
            SourceStatus::RateLimited {
                until,
                origin: BlockOrigin::ThisRun,
            } => format!("is rate limited until {}", display_time(*until)),
            SourceStatus::NotAsked => "was not needed".to_owned(),
        }
    }

    /// Every failure reason with its package count, joined with "; ".
    pub fn failure_reasons(&self) -> String {
        self.joined_reasons("; ")
    }

    fn joined_reasons(&self, separator: &str) -> String {
        self.failures
            .iter()
            .map(|group| {
                format!(
                    "{} ({})",
                    group.reason,
                    count_phrase(u64::from(group.packages), "package", "packages")
                )
            })
            .collect::<Vec<_>>()
            .join(separator)
    }

    /// Whether the source answered every package it was asked about with nothing missing and
    /// no block.
    pub fn fully_answered(&self) -> bool {
        let clean_status = match &self.status {
            SourceStatus::Answered { blocked_until } => blocked_until.is_none(),
            SourceStatus::NotAsked => true,
            SourceStatus::Partial { .. }
            | SourceStatus::Failed { .. }
            | SourceStatus::RateLimited { .. } => false,
        };
        clean_status && self.details_missing == 0
    }
}

/// A source's outcome.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SourceStatus {
    /// Every package it was asked about was answered; `blocked_until` when the run ended by
    /// blocking it after its last answer.
    Answered {
        blocked_until: Option<DateTime<Utc>>,
    },
    /// Some packages were answered and some were not.
    Partial {
        blocked_until: Option<DateTime<Utc>>,
    },
    /// No package was answered, and no block explains it.
    Failed { failure: SourceFailure },
    /// No package was answered because of a block.
    RateLimited {
        until: DateTime<Utc>,
        origin: BlockOrigin,
    },
    /// No package needed it.
    NotAsked,
}

impl SourceStatus {
    /// The status's word, as the report's `status` field writes it.
    pub fn name(&self) -> &'static str {
        match self {
            SourceStatus::Answered { .. } => "answered",
            SourceStatus::Partial { .. } => "partial",
            SourceStatus::Failed { .. } => "failed",
            SourceStatus::RateLimited { .. } => "rate_limited",
            SourceStatus::NotAsked => "not_asked",
        }
    }
}

/// Where a block that kept a source from answering came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockOrigin {
    /// An earlier run saved it, so the source was not asked.
    Saved,
    /// This run hit it before the source answered anything.
    ThisRun,
}

impl BlockOrigin {
    /// The origin's word.
    pub fn name(self) -> &'static str {
        match self {
            BlockOrigin::Saved => "saved",
            BlockOrigin::ThisRun => "this_run",
        }
    }
}

/// One reason a source left packages unanswered, and how many.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FailureGroup {
    pub reason: String,
    pub packages: u32,
}

/// `time` as people read it: "2026-11-26 14:00:05 UTC".
pub fn display_time(time: DateTime<Utc>) -> String {
    time.format("%Y-%m-%d %H:%M:%S UTC").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(status: PackageStatus) -> PackageReport {
        PackageReport {
            input: "serde".to_owned(),
            name: "serde".to_owned(),
            version: Some("1.0.0".to_owned()),
            version_from: Some(VersionFrom::Input),
            status,
            reason: None,
            findings: Vec::new(),
            supply_chain: None,
            answers: BTreeMap::from([(SourceName::Osv, AnswerWord::Answered)]),
        }
    }

    #[test]
    fn from_statuses_all_not_checked_is_failed() {
        let outcome =
            Outcome::from_statuses(&[PackageStatus::NotChecked, PackageStatus::NotChecked]);

        assert_eq!(outcome, Outcome::Failed);
    }

    #[test]
    fn from_statuses_all_not_found_is_incomplete() {
        let outcome = Outcome::from_statuses(&[PackageStatus::NotFound, PackageStatus::NotFound]);

        assert_eq!(outcome, Outcome::Incomplete);
    }

    #[test]
    fn from_statuses_vulnerable_beats_not_checked() {
        let outcome =
            Outcome::from_statuses(&[PackageStatus::NotChecked, PackageStatus::Vulnerable]);

        assert_eq!(outcome, Outcome::Vulnerable);
    }

    #[test]
    fn from_statuses_all_clean_is_clean() {
        let outcome = Outcome::from_statuses(&[PackageStatus::Clean, PackageStatus::Clean]);

        assert_eq!(outcome, Outcome::Clean);
    }

    #[test]
    fn from_statuses_clean_with_not_checked_is_incomplete() {
        let outcome = Outcome::from_statuses(&[PackageStatus::Clean, PackageStatus::NotChecked]);

        assert_eq!(outcome, Outcome::Incomplete);
    }

    #[test]
    fn a_report_serializes_with_the_documented_field_names() {
        let mut not_checked = package(PackageStatus::NotChecked);
        not_checked.reason = Some("OSV.dev did not answer in time".to_owned());
        not_checked
            .answers
            .insert(SourceName::Offline, AnswerWord::Failed);
        let as_of = Utc::now();
        let report = Report {
            report_version: REPORT_VERSION,
            tool: "dep-check 1.0.0".to_owned(),
            checked_at: Utc::now(),
            ecosystem: Ecosystem::Cargo,
            offline: OfflineUse::Fallback,
            outcome: Outcome::Incomplete,
            summary: Summary::default(),
            sources: vec![
                SourceReport {
                    source: SourceName::GitHub,
                    status: SourceStatus::Failed {
                        failure: SourceFailure::TimedOut,
                    },
                    requests: 2,
                    failures: vec![FailureGroup {
                        reason: "did not answer in time".to_owned(),
                        packages: 1,
                    }],
                    details_missing: 0,
                    data_as_of: None,
                },
                SourceReport {
                    source: SourceName::DepsDev,
                    status: SourceStatus::Answered {
                        blocked_until: None,
                    },
                    requests: 1,
                    failures: Vec::new(),
                    details_missing: 0,
                    data_as_of: None,
                },
                SourceReport {
                    source: SourceName::Offline,
                    status: SourceStatus::Failed {
                        failure: SourceFailure::OfflineStale { as_of },
                    },
                    requests: 0,
                    failures: Vec::new(),
                    details_missing: 0,
                    data_as_of: Some(as_of),
                },
            ],
            packages: vec![package(PackageStatus::Clean), not_checked],
        };

        let json = serde_json::to_value(&report).unwrap();

        assert_eq!(json["sources"][0]["source"], "github");
        assert_eq!(json["sources"][0]["status"], "failed");
        assert_eq!(json["sources"][0]["failure"]["kind"], "timed_out");
        assert_eq!(json["sources"][0]["failures"][0]["packages"], 1);
        assert!(json["sources"][0]["data_as_of"].is_null());
        assert_eq!(json["sources"][1]["source"], "deps_dev");
        assert!(json["sources"][1]["blocked_until"].is_null());
        assert_eq!(json["sources"][2]["source"], "offline");
        assert_eq!(json["sources"][2]["failure"]["kind"], "offline_stale");
        assert_eq!(
            json["sources"][2]["data_as_of"],
            serde_json::to_value(as_of).unwrap()
        );
        assert_eq!(json["ecosystem"], "cargo");
        assert_eq!(json["offline"], "fallback");
        assert_eq!(json["outcome"], "incomplete");
        assert!(json["packages"][0].get("reason").is_none());
        assert_eq!(
            json["packages"][1]["reason"],
            "OSV.dev did not answer in time"
        );
        assert_eq!(json["packages"][0]["answers"]["osv"], "answered");
        assert_eq!(json["packages"][1]["answers"]["offline"], "failed");
        assert_eq!(json["packages"][0]["version_from"], "input");
    }

    #[rstest::rstest]
    #[case::missing(SourceFailure::OfflineMissing, "offline_missing")]
    #[case::stale(SourceFailure::OfflineStale { as_of: Utc::now() }, "offline_stale")]
    #[case::unreadable(
        SourceFailure::OfflineUnreadable { reason: "file is not a database".to_owned() },
        "offline_unreadable"
    )]
    #[case::undecided(SourceFailure::OfflineUndecided, "offline_undecided")]
    #[case::busy(SourceFailure::OfflineBusy, "offline_busy")]
    #[case::stopped(SourceFailure::OfflineStopped, "offline_stopped")]
    fn each_offline_failure_serializes_with_its_kind(
        #[case] failure: SourceFailure,
        #[case] kind: &str,
    ) {
        let json = serde_json::to_value(&failure).unwrap();

        assert_eq!((json["kind"].as_str(), failure.kind()), (Some(kind), kind));
    }

    #[rstest::rstest]
    #[case::missing(
        SourceFailure::OfflineMissing,
        "Offline data is not downloaded for this ecosystem"
    )]
    #[case::stale(
        SourceFailure::OfflineStale { as_of: block_end() },
        "Offline data is from 26 Nov 2026, too old for a clean result"
    )]
    #[case::unreadable(
        SourceFailure::OfflineUnreadable { reason: "file is not\na database".to_owned() },
        "Offline data could not be read: file is not a database"
    )]
    #[case::undecided(
        SourceFailure::OfflineUndecided,
        "Offline data has an advisory with a version this tool cannot compare"
    )]
    #[case::busy(SourceFailure::OfflineBusy, "Offline data is being removed or rebuilt")]
    #[case::stopped(
        SourceFailure::OfflineStopped,
        "Offline data stopped before it answered"
    )]
    fn each_offline_failure_reads_after_the_source_s_label(
        #[case] failure: SourceFailure,
        #[case] text: &str,
    ) {
        let sentence = format!("{} {}", SourceName::Offline.label(), failure.describe());

        assert_eq!(sentence, text);
    }

    /// The block end every describe case shows; its exact text is the behavior under test.
    const BLOCK_END: &str = "2026-11-26T14:00:05Z";

    fn block_end() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(BLOCK_END)
            .unwrap()
            .with_timezone(&Utc)
    }

    fn source_with(status: SourceStatus, failures: Vec<FailureGroup>) -> SourceReport {
        SourceReport {
            source: SourceName::Osv,
            status,
            requests: 3,
            failures,
            details_missing: 0,
            data_as_of: None,
        }
    }

    fn group(reason: &str, packages: u32) -> FailureGroup {
        FailureGroup {
            reason: reason.to_owned(),
            packages,
        }
    }

    #[rstest::rstest]
    #[case::answered(SourceStatus::Answered { blocked_until: None }, vec![], "answered")]
    #[case::answered_then_blocked(
        SourceStatus::Answered { blocked_until: Some(block_end()) },
        vec![],
        "answered, then was rate limited until 2026-11-26 14:00:05 UTC"
    )]
    #[case::partial_then_blocked(
        SourceStatus::Partial { blocked_until: Some(block_end()) },
        vec![group("is rate limited until 2026-11-26 14:00:05 UTC", 2)],
        "answered some packages, then was rate limited until 2026-11-26 14:00:05 UTC"
    )]
    #[case::partial(
        SourceStatus::Partial { blocked_until: None },
        vec![group("did not answer in time", 1), group("answered 503", 2)],
        "answered some packages; for the others it did not answer in time (1 package) and answered 503 (2 packages)"
    )]
    #[case::failed(
        SourceStatus::Failed { failure: SourceFailure::Interrupted },
        vec![group("lost the connection before the answer was complete", 2)],
        "lost the connection before the answer was complete"
    )]
    #[case::saved_block(
        SourceStatus::RateLimited { until: block_end(), origin: BlockOrigin::Saved },
        vec![],
        "is rate limited until 2026-11-26 14:00:05 UTC (from an earlier run)"
    )]
    #[case::this_run_block(
        SourceStatus::RateLimited { until: block_end(), origin: BlockOrigin::ThisRun },
        vec![group("is rate limited until 2026-11-26 14:00:05 UTC", 1)],
        "is rate limited until 2026-11-26 14:00:05 UTC"
    )]
    #[case::not_asked(SourceStatus::NotAsked, vec![], "was not needed")]
    fn describe_reads_as_the_rest_of_a_sentence_about_the_source(
        #[case] status: SourceStatus,
        #[case] failures: Vec<FailureGroup>,
        #[case] text: &str,
    ) {
        let source = source_with(status, failures);

        assert_eq!(source.describe(), text);
    }

    #[test]
    fn summary_counts_each_status_and_every_finding() {
        let packages = [
            package(PackageStatus::Clean),
            package(PackageStatus::NotFound),
            package(PackageStatus::NotChecked),
        ];

        let summary = Summary::of(&packages);

        assert_eq!(
            summary,
            Summary {
                packages: 3,
                vulnerable: 0,
                clean: 1,
                not_found: 1,
                not_checked: 1,
                findings: 0
            }
        );
    }
}

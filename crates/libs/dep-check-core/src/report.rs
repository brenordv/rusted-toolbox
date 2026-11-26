//! The report one check produces: per-package verdicts, per-source outcomes, and the overall
//! outcome. Serialized with serde, field names in `snake_case`.

use crate::advisory::{AdvisoryKind, Cvss, Epss, Severity, SourceName};
use crate::ecosystem::Ecosystem;
use crate::http::SourceFailure;
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
    /// What each source said about this package.
    pub answers: BTreeMap<SourceName, AnswerWord>,
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
    /// Advisories that came back as ids without their details.
    pub details_missing: u32,
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
                    count_phrase(u64::from(group.packages), "package")
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

/// `count` with `noun`, plural when `count` is not 1: "1 package", "3 packages".
pub fn count_phrase(count: u64, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
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
        let report = Report {
            report_version: REPORT_VERSION,
            tool: "dep-check 1.0.0".to_owned(),
            checked_at: Utc::now(),
            ecosystem: Ecosystem::Cargo,
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
                },
                SourceReport {
                    source: SourceName::DepsDev,
                    status: SourceStatus::Answered {
                        blocked_until: None,
                    },
                    requests: 1,
                    failures: Vec::new(),
                    details_missing: 0,
                },
            ],
            packages: vec![package(PackageStatus::Clean), not_checked],
        };

        let json = serde_json::to_value(&report).unwrap();

        assert_eq!(json["sources"][0]["source"], "github");
        assert_eq!(json["sources"][0]["status"], "failed");
        assert_eq!(json["sources"][0]["failure"]["kind"], "timed_out");
        assert_eq!(json["sources"][0]["failures"][0]["packages"], 1);
        assert_eq!(json["sources"][1]["source"], "deps_dev");
        assert!(json["sources"][1]["blocked_until"].is_null());
        assert_eq!(json["ecosystem"], "cargo");
        assert_eq!(json["outcome"], "incomplete");
        assert!(json["packages"][0].get("reason").is_none());
        assert_eq!(
            json["packages"][1]["reason"],
            "OSV.dev did not answer in time"
        );
        assert_eq!(json["packages"][0]["answers"]["osv"], "answered");
        assert_eq!(json["packages"][0]["version_from"], "input");
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

    #[test]
    fn count_phrase_is_singular_only_for_one() {
        assert_eq!(
            (
                count_phrase(1, "package"),
                count_phrase(0, "package"),
                count_phrase(3, "package")
            ),
            (
                "1 package".to_owned(),
                "0 packages".to_owned(),
                "3 packages".to_owned()
            )
        );
    }
}

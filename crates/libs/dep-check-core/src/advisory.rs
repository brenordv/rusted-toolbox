//! What one source says about one package: its advisories, with the names of the sources.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A service the engine asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceName {
    Osv,
    DepsDev,
    // snake_case alone would give "git_hub".
    #[serde(rename = "github")]
    GitHub,
    /// OSV.dev's data downloaded to this machine.
    Offline,
}

impl SourceName {
    /// The source's name as people write it.
    pub fn label(self) -> &'static str {
        match self {
            SourceName::Osv => "OSV.dev",
            SourceName::DepsDev => "deps.dev",
            SourceName::GitHub => "GitHub Advisory Database",
            SourceName::Offline => "Offline data",
        }
    }

    /// The source's word in the report and in log lines.
    pub fn name(self) -> &'static str {
        match self {
            SourceName::Osv => "osv",
            SourceName::DepsDev => "deps_dev",
            SourceName::GitHub => "github",
            SourceName::Offline => "offline",
        }
    }
}

/// What an advisory is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AdvisoryKind {
    Vulnerability,
    /// The package itself is malicious (OSV's `MAL-` records, GitHub's `malware` type).
    Malware,
}

/// How bad an advisory is, lowest first, so the highest is the `max()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Unknown,
    Low,
    Medium,
    High,
    Critical,
}

impl Severity {
    /// The level a CVSS base score falls in on FIRST's qualitative scale (CVSS v3.1 Table 14,
    /// CVSS v4.0 Table 22): 0.1-3.9 Low, 4.0-6.9 Medium, 7.0-8.9 High, 9.0-10.0 Critical.
    /// Any other score, 0.0 included, is `Unknown`.
    pub fn from_score(score: f64) -> Severity {
        match score {
            s if (0.1..4.0).contains(&s) => Severity::Low,
            s if (4.0..7.0).contains(&s) => Severity::Medium,
            s if (7.0..9.0).contains(&s) => Severity::High,
            s if (9.0..=10.0).contains(&s) => Severity::Critical,
            _ => Severity::Unknown,
        }
    }

    /// The level's word.
    pub fn name(self) -> &'static str {
        match self {
            Severity::Unknown => "unknown",
            Severity::Low => "low",
            Severity::Medium => "medium",
            Severity::High => "high",
            Severity::Critical => "critical",
        }
    }
}

/// One CVSS rating a source gave.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Cvss {
    /// The CVSS version: "2", "3", or "4".
    pub version: String,
    pub vector: Option<String>,
    /// The base score, when the source sent one; the tool never computes one from a vector.
    pub score: Option<f64>,
    pub from: SourceName,
}

/// FIRST's Exploit Prediction Scoring System values for an advisory's CVE.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Epss {
    pub percentage: f64,
    pub percentile: f64,
}

/// One advisory as one source described it for one package.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceAdvisory {
    pub source: SourceName,
    pub id: String,
    /// Other databases' ids for the same advisory.
    pub aliases: Vec<String>,
    pub kind: AdvisoryKind,
    pub summary: Option<String>,
    /// A level the source itself gave (GitHub does).
    pub severity: Option<Severity>,
    pub cvss: Vec<Cvss>,
    pub epss: Option<Epss>,
    /// Versions that fix it.
    pub fixed_in: Vec<String>,
    pub urls: Vec<String>,
    pub published: Option<DateTime<Utc>>,
}

impl SourceAdvisory {
    /// An advisory known only by its id, as when its record could not be fetched.
    pub fn id_only(source: SourceName, id: &str) -> Self {
        Self {
            source,
            id: id.to_owned(),
            aliases: Vec::new(),
            kind: kind_of_id(id),
            summary: None,
            severity: None,
            cvss: Vec::new(),
            epss: None,
            fixed_in: Vec::new(),
            urls: Vec::new(),
            published: None,
        }
    }
}

/// `Malware` for an id in OSV's Malicious Packages Repository (the `MAL-` prefix), else
/// `Vulnerability`.
pub fn kind_of_id(id: &str) -> AdvisoryKind {
    if id.starts_with("MAL-") {
        AdvisoryKind::Malware
    } else {
        AdvisoryKind::Vulnerability
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::zero(0.0, Severity::Unknown)]
    #[case::lowest_low(0.1, Severity::Low)]
    #[case::highest_low(3.9, Severity::Low)]
    #[case::lowest_medium(4.0, Severity::Medium)]
    #[case::highest_medium(6.9, Severity::Medium)]
    #[case::lowest_high(7.0, Severity::High)]
    #[case::highest_high(8.9, Severity::High)]
    #[case::lowest_critical(9.0, Severity::Critical)]
    #[case::highest_critical(10.0, Severity::Critical)]
    #[case::above_scale(10.1, Severity::Unknown)]
    #[case::negative(-1.0, Severity::Unknown)]
    fn from_score_follows_the_first_scale(#[case] score: f64, #[case] expected: Severity) {
        assert_eq!(Severity::from_score(score), expected);
    }

    #[test]
    fn source_name_github_serializes_as_github() {
        let json = serde_json::to_string(&SourceName::GitHub).unwrap();

        assert_eq!(json, "\"github\"");
    }

    #[rstest]
    #[case::malicious("MAL-2024-1234", AdvisoryKind::Malware)]
    #[case::rustsec("RUSTSEC-2023-0005", AdvisoryKind::Vulnerability)]
    fn id_only_takes_its_kind_from_the_prefix(#[case] id: &str, #[case] kind: AdvisoryKind) {
        let advisory = SourceAdvisory::id_only(SourceName::Osv, id);

        assert_eq!(advisory.kind, kind);
    }
}

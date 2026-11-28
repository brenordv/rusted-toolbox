//! The report as text: the lines a terminal prints and a window shows.
//!
//! Every string that came from a service or a package's metadata goes through
//! `display_line` here, so no control character, line break, or bidirectional control reaches
//! the screen: summaries, ids, aliases, fixed versions, URLs, CVSS vectors, licenses,
//! repository ids, provenance words, deprecation reasons, versions, and failure details.

use crate::advisory::{AdvisoryKind, Cvss};
use crate::report::{
    Finding, PackageReport, PackageStatus, Report, SourceReport, SupplyChain, VersionFrom,
};
use chrono::{DateTime, Utc};
use common_utils::string_utils::{display_line, error_preview, format_thousands};

/// The whole report as text: the header, the sources line, then each package's block, with a
/// blank line between blocks and a final newline.
pub fn text_report(report: &Report) -> String {
    let mut lines = vec![header_line(report), sources_line(report)];
    for package in &report.packages {
        lines.push(String::new());
        lines.extend(package_block(package));
    }
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

/// "dep-check 1.0.0  Cargo  3 packages: 1 vulnerable, 1 clean, 1 not found".
pub fn header_line(report: &Report) -> String {
    let summary = &report.summary;
    let counts: Vec<String> = [
        (summary.vulnerable, PackageStatus::Vulnerable),
        (summary.clean, PackageStatus::Clean),
        (summary.not_found, PackageStatus::NotFound),
        (summary.not_checked, PackageStatus::NotChecked),
    ]
    .into_iter()
    .filter(|(count, _)| *count > 0)
    .map(|(count, status)| format!("{} {}", format_thousands(count as u64), status_word(status)))
    .collect();
    let packages = count_phrase(summary.packages as u64, "package", "packages");
    let counted = if counts.is_empty() {
        packages
    } else {
        format!("{packages}: {}", counts.join(", "))
    };
    format!(
        "{}  {}  {counted}",
        shown(&report.tool),
        report.ecosystem.label()
    )
}

/// Each source and what it did: "OSV.dev answered; deps.dev did not answer in time".
pub fn sources_line(report: &Report) -> String {
    report
        .sources
        .iter()
        .map(source_phrase)
        .collect::<Vec<_>>()
        .join("; ")
}

/// One source and what it did: "deps.dev did not answer in time".
pub fn source_phrase(source: &SourceReport) -> String {
    format!("{} {}", source.source.label(), shown(&source.describe()))
}

/// A package's first line: its name, its version, and its verdict.
pub fn package_line(package: &PackageReport) -> String {
    let mut name = shown(&package.name);
    if let Some(version) = &package.version {
        name.push(' ');
        name.push_str(&shown(version));
    }
    if package.version_from == Some(VersionFrom::DepsDevLatest) {
        name.push_str(" (latest, from deps.dev)");
    }
    let verdict = match package.status {
        PackageStatus::Vulnerable => format!(
            "vulnerable, {}",
            count_phrase(package.findings.len() as u64, "advisory", "advisories")
        ),
        PackageStatus::Clean => "clean".to_owned(),
        PackageStatus::NotFound => format!("not found: {}", not_found_sentence(package)),
        PackageStatus::NotChecked => match &package.reason {
            Some(reason) => format!("not checked: {}", shown(reason)),
            None => "not checked".to_owned(),
        },
    };
    format!("{name}  {verdict}")
}

/// What a `not_found` package's status means: "deps.dev does not know this version" when a
/// version was asked, else "... this package".
pub fn not_found_sentence(package: &PackageReport) -> &'static str {
    match package.version {
        Some(_) => "deps.dev does not know this version",
        None => "deps.dev does not know this package",
    }
}

/// A finding's lines, each without its indent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindingLines {
    /// The id, the severity (or "malware"), and the fixed versions.
    pub head: String,
    /// The summary on one line, cut at 140 characters.
    pub summary: Option<String>,
    /// "also CVE-..., GHSA-...".
    pub aliases: Option<String>,
    /// The CVSS ratings and the sources: "CVSS 3 5.4 (deps.dev). From OSV.dev, deps.dev".
    pub scores: String,
    /// The finding's `https://` links.
    pub urls: Vec<String>,
}

/// A finding as the lines a terminal or a window shows.
pub fn finding_lines(finding: &Finding) -> FindingLines {
    let id = shown(&finding.id);
    let head = match finding.kind {
        AdvisoryKind::Malware => format!("{id}  malware  no fixed version: remove the package"),
        AdvisoryKind::Vulnerability if finding.fixed_in.is_empty() => {
            format!("{id}  {}  no fixed version", finding.severity.name())
        }
        AdvisoryKind::Vulnerability => format!(
            "{id}  {}  fixed in {}",
            finding.severity.name(),
            joined(&finding.fixed_in)
        ),
    };
    let mut scores: Vec<String> = finding.cvss.iter().map(cvss_phrase).collect();
    let sources: Vec<&str> = finding
        .sources
        .iter()
        .map(|source| source.label())
        .collect();
    scores.push(format!("From {}", sources.join(", ")));
    FindingLines {
        head,
        summary: finding
            .summary
            .as_deref()
            .map(|summary| error_preview(summary).shown),
        aliases: (!finding.aliases.is_empty())
            .then(|| format!("also {}", joined(&finding.aliases))),
        scores: scores.join(". "),
        urls: finding
            .urls
            .iter()
            .filter(|url| url.starts_with("https://"))
            .map(|url| shown(url))
            .collect(),
    }
}

/// The supply-chain facts on one line: "MIT. Source github.com/tokio-rs/tokio (unverified
/// metadata). Scorecard 8.1 (2026-11-17). Published 2022-07-12. Not the latest version."
pub fn supply_chain_line(chain: &SupplyChain) -> String {
    let mut parts = Vec::new();
    if chain.licenses.is_empty() {
        parts.push("No license listed".to_owned());
    } else {
        parts.push(joined(&chain.licenses));
    }
    if let Some(reason) = &chain.deprecated {
        if reason.is_empty() {
            parts.push("Deprecated".to_owned());
        } else {
            parts.push(format!("Deprecated: {}", shown(reason)));
        }
    }
    if let Some(repo) = &chain.source_repo {
        parts.push(format!(
            "Source {} ({})",
            shown(&repo.id),
            shown(&repo.provenance.replace('_', " "))
        ));
    }
    if let Some(card) = &chain.scorecard {
        match card.date {
            Some(date) => parts.push(format!("Scorecard {:.1} ({})", card.score, day(date))),
            None => parts.push(format!("Scorecard {:.1}", card.score)),
        }
    }
    if let Some(published) = chain.published_at {
        parts.push(format!("Published {}", day(published)));
    }
    if chain.attestations.total > 0 {
        parts.push(format!(
            "{}, {} verified",
            count_phrase(
                u64::from(chain.attestations.total),
                "attestation",
                "attestations"
            ),
            format_thousands(u64::from(chain.attestations.verified))
        ));
    }
    if !chain.latest {
        parts.push("Not the latest version".to_owned());
    }
    format!("{}.", parts.join(". "))
}

/// A status as people read it: "vulnerable", "clean", "not found", "not checked".
pub fn status_word(status: PackageStatus) -> &'static str {
    match status {
        PackageStatus::Vulnerable => "vulnerable",
        PackageStatus::Clean => "clean",
        PackageStatus::NotFound => "not found",
        PackageStatus::NotChecked => "not checked",
    }
}

/// `count` with thousands separators and the noun that fits it: "1 package", "1,000 packages".
pub fn count_phrase(count: u64, singular: &str, plural: &str) -> String {
    let noun = if count == 1 { singular } else { plural };
    format!("{} {noun}", format_thousands(count))
}

fn package_block(package: &PackageReport) -> Vec<String> {
    let mut lines = vec![package_line(package)];
    for finding in &package.findings {
        let finding = finding_lines(finding);
        lines.push(format!("  {}", finding.head));
        lines.extend(finding.summary.map(|summary| format!("    {summary}")));
        lines.extend(finding.aliases.map(|aliases| format!("    {aliases}")));
        lines.push(format!("    {}", finding.scores));
        lines.extend(finding.urls.into_iter().map(|url| format!("    {url}")));
    }
    if let Some(chain) = &package.supply_chain {
        lines.push(format!("  {}", supply_chain_line(chain)));
    }
    lines
}

fn cvss_phrase(cvss: &Cvss) -> String {
    let rating = match (cvss.score, &cvss.vector) {
        (Some(score), _) => format!("{score:.1}"),
        (None, Some(vector)) => shown(vector),
        (None, None) => "unrated".to_owned(),
    };
    format!(
        "CVSS {} {rating} ({})",
        shown(&cvss.version),
        cvss.from.label()
    )
}

/// `text` on one line, with control and bidirectional characters as spaces.
fn shown(text: &str) -> String {
    display_line(text.chars())
}

fn joined(values: &[String]) -> String {
    values
        .iter()
        .map(|value| shown(value))
        .collect::<Vec<_>>()
        .join(", ")
}

fn day(time: DateTime<Utc>) -> String {
    time.format("%Y-%m-%d").to_string()
}

#[cfg(test)]
mod tests;

//! Merges what every source said about one package into its findings and its status.

use crate::advisory::{AdvisoryKind, Cvss, Severity, SourceAdvisory, SourceName};
use crate::engine::PackageAnswer;
use crate::report::{AnswerWord, Finding, PackageStatus};
use std::cmp::Reverse;
use std::collections::BTreeSet;

/// One package's merged verdict.
#[derive(Debug, Clone, PartialEq)]
pub struct PackageVerdict {
    pub status: PackageStatus,
    /// Why the package was not checked; set only for `NotChecked`.
    pub reason: Option<String>,
    pub findings: Vec<Finding>,
}

/// Merges each source's answer for one package.
///
/// Every advisory, from a finished answer or from one a source returned before it failed, is
/// a node; nodes whose ids or aliases share a value are one finding. The status is the first
/// that fits: `Vulnerable` with any finding, `NotFound` when deps.dev does not know the
/// package, `Clean` when a source answered, else `NotChecked` with each source's failure.
pub fn merge(answers: &[(SourceName, &PackageAnswer)]) -> PackageVerdict {
    let nodes: Vec<&SourceAdvisory> = answers
        .iter()
        .flat_map(|(_, answer)| answer.advisories())
        .collect();
    let mut findings: Vec<Finding> = groups(&nodes)
        .iter()
        .map(|group| finding_of(group))
        .collect();
    findings.sort_by(|a, b| {
        Reverse(a.severity)
            .cmp(&Reverse(b.severity))
            .then_with(|| malware_first(a.kind).cmp(&malware_first(b.kind)))
            .then_with(|| a.id.cmp(&b.id))
    });

    let (mut not_found, mut answered) = (false, false);
    for (_, answer) in answers {
        match answer.word() {
            AnswerWord::NotFound => not_found = true,
            AnswerWord::Answered => answered = true,
            AnswerWord::Failed | AnswerWord::RateLimited | AnswerWord::NotAsked => {}
        }
    }
    let status = if !findings.is_empty() {
        PackageStatus::Vulnerable
    } else if not_found {
        PackageStatus::NotFound
    } else if answered {
        PackageStatus::Clean
    } else {
        PackageStatus::NotChecked
    };
    let reason = (status == PackageStatus::NotChecked).then(|| not_checked_reason(answers));
    PackageVerdict {
        status,
        reason,
        findings,
    }
}

fn not_checked_reason(answers: &[(SourceName, &PackageAnswer)]) -> String {
    let reasons: Vec<String> = answers
        .iter()
        .filter_map(|(source, answer)| {
            answer
                .failure()
                .map(|failure| format!("{} {}", source.label(), failure.describe()))
        })
        .collect();
    if reasons.is_empty() {
        "no source was asked".to_owned()
    } else {
        reasons.join("; ")
    }
}

fn malware_first(kind: AdvisoryKind) -> u8 {
    match kind {
        AdvisoryKind::Malware => 0,
        AdvisoryKind::Vulnerability => 1,
    }
}

/// The nodes grouped by shared ids (union-find), each group in node order, groups in the order
/// of their first node.
fn groups<'a>(nodes: &[&'a SourceAdvisory]) -> Vec<Vec<&'a SourceAdvisory>> {
    let mut parents: Vec<usize> = (0..nodes.len()).collect();
    for i in 0..nodes.len() {
        for j in (i + 1)..nodes.len() {
            if share_an_id(nodes[i], nodes[j]) {
                let (root_i, root_j) = (root(&mut parents, i), root(&mut parents, j));
                parents[root_j.max(root_i)] = root_i.min(root_j);
            }
        }
    }
    let mut groups: Vec<(usize, Vec<&SourceAdvisory>)> = Vec::new();
    for (index, node) in nodes.iter().enumerate() {
        let group_root = root(&mut parents, index);
        match groups.iter_mut().find(|(r, _)| *r == group_root) {
            Some((_, members)) => members.push(node),
            None => groups.push((group_root, vec![node])),
        }
    }
    groups.into_iter().map(|(_, members)| members).collect()
}

fn root(parents: &mut [usize], mut index: usize) -> usize {
    while parents[index] != index {
        parents[index] = parents[parents[index]];
        index = parents[index];
    }
    index
}

fn ids(advisory: &SourceAdvisory) -> impl Iterator<Item = &str> {
    std::iter::once(advisory.id.as_str()).chain(advisory.aliases.iter().map(String::as_str))
}

fn share_an_id(a: &SourceAdvisory, b: &SourceAdvisory) -> bool {
    ids(a).any(|id| ids(b).any(|other| other == id))
}

/// Which advisory's id heads a finding, lowest first: OSV.dev's, and among OSV.dev's records the
/// ecosystem database's (RUSTSEC, PYSEC, ...) before a GitHub one it also holds; then GitHub's,
/// then deps.dev's.
fn headline_rank(advisory: &SourceAdvisory) -> u8 {
    match advisory.source {
        SourceName::Osv if advisory.id.starts_with("GHSA-") => 1,
        SourceName::Osv => 0,
        SourceName::GitHub => 2,
        SourceName::DepsDev => 3,
    }
}

/// Which source's summary a finding shows, lowest first.
fn summary_rank(source: SourceName) -> u8 {
    match source {
        SourceName::GitHub => 0,
        SourceName::Osv => 1,
        SourceName::DepsDev => 2,
    }
}

fn finding_of(group: &[&SourceAdvisory]) -> Finding {
    let headline = group
        .iter()
        .min_by_key(|node| headline_rank(node))
        .map(|node| node.id.clone())
        .unwrap_or_default();
    let aliases: BTreeSet<&str> = group
        .iter()
        .flat_map(|node| ids(node))
        .filter(|id| *id != headline)
        .collect();
    let mut summaries: Vec<&&SourceAdvisory> =
        group.iter().filter(|node| node.summary.is_some()).collect();
    summaries.sort_by_key(|node| summary_rank(node.source));
    let kind = if group.iter().any(|node| node.kind == AdvisoryKind::Malware) {
        AdvisoryKind::Malware
    } else {
        AdvisoryKind::Vulnerability
    };
    Finding {
        id: headline,
        aliases: aliases.into_iter().map(str::to_owned).collect(),
        kind,
        severity: severity_of(group),
        summary: summaries.first().and_then(|node| node.summary.clone()),
        cvss: cvss_union(group),
        epss: group.iter().find_map(|node| node.epss.clone()),
        fixed_in: union(group.iter().flat_map(|node| node.fixed_in.iter())),
        urls: union(group.iter().flat_map(|node| node.urls.iter())),
        sources: group
            .iter()
            .map(|node| node.source)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
    }
}

/// A level a source gave (GitHub's) when there is one, else the level of the highest CVSS
/// score, else `Unknown`. A score is never computed from a vector.
fn severity_of(group: &[&SourceAdvisory]) -> Severity {
    if let Some(label) = group.iter().filter_map(|node| node.severity).max() {
        return label;
    }
    group
        .iter()
        .flat_map(|node| node.cvss.iter())
        .filter_map(|cvss| cvss.score)
        .map(Severity::from_score)
        .max()
        .unwrap_or(Severity::Unknown)
}

/// Every rating once: two with the same vector are one, and the one with a score is kept.
fn cvss_union(group: &[&SourceAdvisory]) -> Vec<Cvss> {
    let mut ratings: Vec<Cvss> = Vec::new();
    for cvss in group.iter().flat_map(|node| node.cvss.iter()) {
        let duplicate = ratings
            .iter_mut()
            .find(|kept| match (&kept.vector, &cvss.vector) {
                (Some(kept_vector), Some(vector)) => kept_vector == vector,
                (None, None) | (Some(_), None) | (None, Some(_)) => *kept == cvss,
            });
        match duplicate {
            Some(kept) if kept.score.is_none() && cvss.score.is_some() => *kept = cvss.clone(),
            Some(_) => {}
            None => ratings.push(cvss.clone()),
        }
    }
    ratings
}

/// The values in first-seen order, each once.
fn union<'a>(values: impl Iterator<Item = &'a String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .filter(|value| seen.insert(value.as_str()))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests;

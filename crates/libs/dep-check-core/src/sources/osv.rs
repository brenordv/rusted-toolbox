//! OSV.dev: which advisories affect each package version (`querybatch`), then each advisory's
//! record (`vulns`).

use crate::advisory::{Cvss, SourceAdvisory, SourceName, kind_of_id};
use crate::ecosystem::Ecosystem;
use crate::engine::{PackageAnswer, Resolved, RunContext, SourceRun};
use crate::http::{
    Answer, HttpLimits, SourceFailure, SourceGate, get_json, mismatched_answer, post_json,
    segment_url, unaskable_id,
};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use futures_util::{StreamExt, stream};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use tracing::instrument;
use url::Url;

/// OSV.dev's address.
pub const OSV_BASE: &str = "https://api.osv.dev/";
/// OSV: querybatch "currently allows a maximum of 1000 package versions to be included in a
/// single query".
const BATCH_SIZE: usize = 1000;
/// The most pages one query is asked for.
const MAX_PAGES: usize = 20;
/// Record fetches in flight at once.
const RECORDS_IN_FLIGHT: usize = 4;
/// The most reference links kept per record.
const MAX_URLS: usize = 5;

/// Asks OSV.dev.
#[derive(Debug, Clone)]
pub struct OsvClient {
    client: Client,
    base: Url,
    querybatch: Url,
    limits: HttpLimits,
}

impl OsvClient {
    /// A client for the OSV.dev at `base`, a host-only address ending in `/`.
    ///
    /// # Errors
    /// When the querybatch address cannot be built on `base`.
    pub fn new(client: Client, base: Url, limits: HttpLimits) -> Result<Self> {
        let querybatch = base
            .join("v1/querybatch")
            .context("Failed to build the OSV.dev querybatch address")?;
        Ok(Self {
            client,
            base,
            querybatch,
            limits,
        })
    }

    /// Asks about every package: the advisory ids of each version in batches, then each
    /// distinct id's record, four at a time. A package is answered once its batch requests all
    /// finished; a record that could not be fetched leaves its advisory as an id only.
    #[instrument(skip_all, fields(source = "osv"))]
    pub async fn check(
        &self,
        ecosystem: Ecosystem,
        packages: &[Resolved],
        context: &RunContext,
    ) -> SourceRun {
        let gate = SourceGate::default();
        let mut progress: Vec<Progress> = packages.iter().map(|_| Progress::default()).collect();
        for (chunk, chunk_progress) in packages
            .chunks(BATCH_SIZE)
            .zip(progress.chunks_mut(BATCH_SIZE))
        {
            self.query_chunk(ecosystem, chunk, chunk_progress, &gate, context)
                .await;
        }

        let mut seen = HashSet::new();
        let ids: Vec<String> = progress
            .iter()
            .flat_map(|package| package.ids.iter())
            .filter(|id| seen.insert(id.as_str()))
            .cloned()
            .collect();
        let records = self.fetch_records(&ids, &gate, context).await;
        let details_missing = ids.iter().filter(|id| !records.contains_key(*id)).count();

        let answers: BTreeMap<_, _> = packages
            .iter()
            .zip(progress)
            .map(|(package, progress)| {
                let advisories = progress
                    .ids
                    .iter()
                    .map(|id| match records.get(id) {
                        Some(record) => {
                            advisory_from_record(SourceName::Osv, record, ecosystem, &package.name)
                        }
                        None => SourceAdvisory::id_only(SourceName::Osv, id),
                    })
                    .collect();
                let answer = match progress.failure {
                    None => PackageAnswer::Answered { advisories },
                    Some(failure) => PackageAnswer::Failed {
                        failure,
                        advisories,
                    },
                };
                (package.key, answer)
            })
            .collect();
        SourceRun {
            source: SourceName::Osv,
            requests: gate.requests(),
            block: gate.block(),
            details_missing: u32::try_from(details_missing).unwrap_or(u32::MAX),
            answers,
        }
    }

    /// Asks querybatch about one chunk, then again for each query that has more pages, only
    /// those, up to [`MAX_PAGES`] pages.
    async fn query_chunk(
        &self,
        ecosystem: Ecosystem,
        chunk: &[Resolved],
        progress: &mut [Progress],
        gate: &SourceGate,
        context: &RunContext,
    ) {
        let mut pending: Vec<(usize, Option<String>)> =
            (0..chunk.len()).map(|index| (index, None)).collect();
        for _ in 0..MAX_PAGES {
            if let Err(failure) = gate.open() {
                fail(progress, &pending, &failure);
                return;
            }
            let body = QueryBatch {
                queries: pending
                    .iter()
                    .map(|(index, page_token)| Query {
                        package: QueryPackage {
                            name: &chunk[*index].name,
                            ecosystem: ecosystem.osv_name(),
                        },
                        version: &chunk[*index].version,
                        page_token: page_token.as_deref(),
                    })
                    .collect(),
            };
            let outcome = post_json::<_, BatchAnswer>(
                &self.client,
                self.querybatch.clone(),
                &body,
                &self.limits,
                &context.clock,
                "querybatch",
            )
            .await;
            gate.record(&outcome);
            let answer = match outcome {
                Ok(Answer::Found(answer)) => answer,
                Ok(Answer::NotFound) => {
                    fail(progress, &pending, &SourceFailure::Status { code: 404 });
                    return;
                }
                Err(failure) => {
                    fail(progress, &pending, &failure);
                    return;
                }
            };
            if answer.results.len() != pending.len() {
                fail(progress, &pending, &mismatched_answer());
                return;
            }
            let mut next = Vec::new();
            for ((index, _), result) in pending.iter().zip(answer.results) {
                let ids = &mut progress[*index].ids;
                for vuln in result.vulns {
                    if !ids.contains(&vuln.id) {
                        ids.push(vuln.id);
                    }
                }
                if let Some(token) = result.next_page_token.filter(|token| !token.is_empty()) {
                    next.push((*index, Some(token)));
                }
            }
            if next.is_empty() {
                return;
            }
            pending = next;
        }
        fail(
            progress,
            &pending,
            &SourceFailure::BadAnswer {
                detail: format!("more than {MAX_PAGES} pages"),
            },
        );
    }

    /// Each id's record, [`RECORDS_IN_FLIGHT`] at a time. An id whose record could not be
    /// fetched, or was not sent for because the source stopped, is left out.
    async fn fetch_records(
        &self,
        ids: &[String],
        gate: &SourceGate,
        context: &RunContext,
    ) -> HashMap<String, OsvRecord> {
        let outcomes: Vec<(String, Result<Answer<OsvRecord>, SourceFailure>)> =
            stream::iter(ids.to_vec())
                .map(|id: String| async move {
                    let outcome = self.fetch_record(&id, gate, context).await;
                    (id, outcome)
                })
                .buffer_unordered(RECORDS_IN_FLIGHT)
                .collect()
                .await;
        outcomes
            .into_iter()
            .filter_map(|(id, outcome)| match outcome {
                Ok(Answer::Found(record)) if record.id == id => Some((id, record)),
                // Counted as a missing detail by the caller; the id alone still proves the
                // match.
                Ok(Answer::Found(_) | Answer::NotFound) | Err(_) => None,
            })
            .collect()
    }

    /// One id's record, unless the source has stopped.
    async fn fetch_record(
        &self,
        id: &str,
        gate: &SourceGate,
        context: &RunContext,
    ) -> Result<Answer<OsvRecord>, SourceFailure> {
        let url = segment_url(&self.base, &["v1", "vulns", id]).ok_or_else(unaskable_id)?;
        gate.open()?;
        let outcome =
            get_json::<OsvRecord>(&self.client, url, &self.limits, &context.clock, "vulns").await;
        gate.record(&outcome);
        outcome
    }
}

/// What the batch requests found about one package so far.
#[derive(Debug, Default)]
struct Progress {
    ids: Vec<String>,
    failure: Option<SourceFailure>,
}

fn fail(progress: &mut [Progress], pending: &[(usize, Option<String>)], failure: &SourceFailure) {
    for (index, _) in pending {
        progress[*index].failure = Some(failure.clone());
    }
}

#[derive(Debug, Serialize)]
struct QueryBatch<'a> {
    queries: Vec<Query<'a>>,
}

#[derive(Debug, Serialize)]
struct Query<'a> {
    package: QueryPackage<'a>,
    version: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    page_token: Option<&'a str>,
}

#[derive(Debug, Serialize)]
struct QueryPackage<'a> {
    name: &'a str,
    ecosystem: &'static str,
}

#[derive(Debug, Deserialize)]
struct BatchAnswer {
    #[serde(default)]
    results: Vec<BatchResult>,
}

#[derive(Debug, Deserialize)]
struct BatchResult {
    #[serde(default)]
    vulns: Vec<VulnRef>,
    next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct VulnRef {
    id: String,
}

/// An OSV record, the fields the tool reads.
#[derive(Debug, Deserialize)]
pub(crate) struct OsvRecord {
    pub id: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub summary: Option<String>,
    #[serde(default)]
    pub severity: Vec<OsvSeverity>,
    #[serde(default)]
    pub affected: Vec<OsvAffected>,
    #[serde(default)]
    pub references: Vec<OsvReference>,
    pub published: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OsvSeverity {
    #[serde(rename = "type")]
    pub kind: String,
    /// The vector string for the CVSS types.
    pub score: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OsvAffected {
    pub package: Option<OsvPackage>,
    #[serde(default)]
    pub ranges: Vec<OsvRange>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OsvPackage {
    pub ecosystem: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OsvRange {
    #[serde(default)]
    pub events: Vec<OsvEvent>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OsvEvent {
    pub fixed: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OsvReference {
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub url: Option<String>,
}

/// `record` as one source's advisory for the package `name` in `ecosystem`: its CVSS vectors
/// (no scores), the fixed versions of the `affected` entries for this package only, and up to
/// five links, advisories before web pages.
pub(crate) fn advisory_from_record(
    source: SourceName,
    record: &OsvRecord,
    ecosystem: Ecosystem,
    name: &str,
) -> SourceAdvisory {
    let cvss = record
        .severity
        .iter()
        .filter_map(|severity| {
            cvss_version(&severity.kind).map(|version| Cvss {
                version: version.to_owned(),
                vector: Some(severity.score.clone()),
                score: None,
                from: source,
            })
        })
        .collect();
    let mut fixed_in: Vec<String> = Vec::new();
    let fixed = record
        .affected
        .iter()
        .filter(|affected| is_this_package(affected, ecosystem, name))
        .flat_map(|affected| affected.ranges.iter())
        .flat_map(|range| range.events.iter())
        .filter_map(|event| event.fixed.clone());
    for version in fixed {
        if !fixed_in.contains(&version) {
            fixed_in.push(version);
        }
    }
    SourceAdvisory {
        source,
        id: record.id.clone(),
        aliases: record.aliases.clone(),
        kind: kind_of_id(&record.id),
        summary: record.summary.clone(),
        severity: None,
        cvss,
        epss: None,
        fixed_in,
        urls: reference_urls(&record.references),
        published: record.published,
    }
}

fn cvss_version(kind: &str) -> Option<&'static str> {
    match kind {
        "CVSS_V2" => Some("2"),
        "CVSS_V3" => Some("3"),
        "CVSS_V4" => Some("4"),
        _ => None,
    }
}

fn is_this_package(affected: &OsvAffected, ecosystem: Ecosystem, name: &str) -> bool {
    affected.package.as_ref().is_some_and(|package| {
        package.ecosystem.as_deref() == Some(ecosystem.osv_name())
            && package
                .name
                .as_deref()
                .is_some_and(|affected_name| ecosystem.same_package(affected_name, name))
    })
}

fn reference_urls(references: &[OsvReference]) -> Vec<String> {
    let of_kind = |wanted: &'static str| {
        references
            .iter()
            .filter(move |reference| reference.kind.as_deref() == Some(wanted))
            .filter_map(|reference| reference.url.clone())
    };
    let mut urls: Vec<String> = Vec::new();
    for url in of_kind("ADVISORY").chain(of_kind("WEB")) {
        if urls.len() == MAX_URLS {
            break;
        }
        if !urls.contains(&url) {
            urls.push(url);
        }
    }
    urls
}

#[cfg(test)]
mod tests;

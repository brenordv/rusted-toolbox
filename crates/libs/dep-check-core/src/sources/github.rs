//! The GitHub Advisory Database (REST `GET /advisories`, unauthenticated): reviewed and malware
//! advisories for each package version, under GitHub's limit of 60 requests an hour per IP.

use crate::advisory::{AdvisoryKind, Cvss, Epss, Severity, SourceAdvisory, SourceName};
use crate::ecosystem::Ecosystem;
use crate::engine::{PackageAnswer, PackageKey, Resolved, RunContext, SourceRun};
use crate::http::{
    Fetched, HttpLimits, SourceFailure, SourceGate, bad_json, encode_segment, fetch,
};
use crate::rate_limits::github_block_until;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use reqwest::Client;
use reqwest::header::{ACCEPT, HeaderMap, HeaderValue};
use serde::Deserialize;
use serde_json::Value;
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};
use tracing::{debug, instrument};
use url::Url;

/// GitHub's API address.
pub const GITHUB_BASE: &str = "https://api.github.com/";
/// The REST API version every request asks for.
pub const GITHUB_API_VERSION: &str = "2026-03-10";
/// The most packages one request names. GitHub takes 1,000, and says to name fewer when the
/// URL would be too long for the client.
const CHUNK_SIZE: usize = 100;
/// The most pages read for one request.
const MAX_PAGES: usize = 10;
/// The advisory types asked for, one request each: `reviewed` leaves malware out, and
/// `unreviewed` advisories are not mapped to packages, so `affects` cannot match them.
const TYPES: [&str; 2] = ["reviewed", "malware"];

/// The headers every GitHub request carries besides the `User-Agent`.
pub fn github_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("application/vnd.github+json"),
    );
    headers.insert(
        "x-github-api-version",
        HeaderValue::from_static(GITHUB_API_VERSION),
    );
    headers
}

/// Asks the GitHub Advisory Database, one request at a time.
#[derive(Debug, Clone)]
pub struct GitHubClient {
    client: Client,
    advisories: Url,
    limits: HttpLimits,
}

/// One package's answer asked alone: the advisory ids it listed, and whether every page came.
type Alone = (BTreeSet<String>, Result<(), SourceFailure>);

/// What a chunk found so far, and the packages that stopped before they finished.
#[derive(Debug, Default)]
struct Progress {
    found: BTreeMap<PackageKey, Vec<SourceAdvisory>>,
    failed: BTreeMap<PackageKey, SourceFailure>,
}

impl Progress {
    /// Fails every package of `chunk` that has not failed already.
    fn fail_all(&mut self, chunk: &[&Resolved], failure: &SourceFailure) {
        for package in chunk {
            self.failed
                .entry(package.key)
                .or_insert_with(|| failure.clone());
        }
    }
}

impl GitHubClient {
    /// A client for the GitHub API at `base`, a host-only address ending in `/`. `client`
    /// carries [`github_headers`].
    ///
    /// # Errors
    /// When the advisories address cannot be built on `base`.
    pub fn new(client: Client, base: Url, limits: HttpLimits) -> Result<Self> {
        let advisories = base
            .join("advisories")
            .context("Failed to build the GitHub advisories address")?;
        Ok(Self {
            client,
            advisories,
            limits,
        })
    }

    /// Asks about every package, in chunks of at most 100 distinct names. A package is
    /// answered only when both advisory types, every page of both, and every confirm about it
    /// have finished; otherwise it fails, keeping what was found for it.
    #[instrument(skip_all, fields(source = "github"))]
    pub async fn check(
        &self,
        ecosystem: Ecosystem,
        packages: &[Resolved],
        context: &RunContext,
    ) -> SourceRun {
        let gate = SourceGate::default();
        let mut progress = Progress::default();
        for chunk in chunks(ecosystem, packages) {
            self.check_chunk(ecosystem, &chunk, &gate, context, &mut progress)
                .await;
        }
        let answers = packages
            .iter()
            .map(|package| {
                let advisories = progress.found.remove(&package.key).unwrap_or_default();
                let answer = match progress.failed.remove(&package.key) {
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
            source: SourceName::GitHub,
            requests: gate.requests(),
            block: gate.block(),
            details_missing: 0,
            data_as_of: None,
            answers,
        }
    }

    async fn check_chunk(
        &self,
        ecosystem: Ecosystem,
        chunk: &[&Resolved],
        gate: &SourceGate,
        context: &RunContext,
        progress: &mut Progress,
    ) {
        for kind in TYPES {
            let (advisories, finished) = self
                .pages(self.chunk_url(ecosystem, chunk, kind), gate, context)
                .await;
            // Each package's own answer for this type, asked once and only when an advisory
            // names it beside another package of the chunk.
            let mut alone: BTreeMap<PackageKey, Alone> = BTreeMap::new();
            for advisory in advisories
                .iter()
                .filter(|advisory| advisory.withdrawn_at.is_none())
            {
                let named = named_packages(advisory, ecosystem, chunk);
                match named.as_slice() {
                    [] => {
                        // GitHub returned it for this list, so a package the tool cannot find
                        // in it means the mapping is wrong, and "nothing found" would be a
                        // guess. The rest of the answer is still kept.
                        progress.fail_all(
                            chunk,
                            &SourceFailure::BadAnswer {
                                detail: "an advisory named none of the packages asked".to_owned(),
                            },
                        );
                    }
                    [only] => keep(progress, advisory, ecosystem, only),
                    several => {
                        for package in several {
                            let (ids, finished) = match alone.entry(package.key) {
                                Entry::Occupied(asked) => asked.into_mut(),
                                Entry::Vacant(slot) => slot.insert(
                                    self.ids_alone(ecosystem, package, kind, gate, context)
                                        .await,
                                ),
                            };
                            if ids.contains(&advisory.ghsa_id) {
                                keep(progress, advisory, ecosystem, package);
                            }
                            if let Err(failure) = finished {
                                progress
                                    .failed
                                    .entry(package.key)
                                    .or_insert_with(|| failure.clone());
                            }
                        }
                    }
                }
            }
            if let Err(failure) = finished {
                progress.fail_all(chunk, &failure);
                return;
            }
        }
    }

    /// Every page of the request at `url`, following `Link` `rel="next"` up to
    /// [`MAX_PAGES`] pages. The advisories read before a failure come back with it.
    async fn pages(
        &self,
        url: Url,
        gate: &SourceGate,
        context: &RunContext,
    ) -> (Vec<GhAdvisory>, Result<(), SourceFailure>) {
        let mut advisories = Vec::new();
        let mut next = Some(url);
        for _ in 0..MAX_PAGES {
            let Some(url) = next.take() else {
                return (advisories, Ok(()));
            };
            let fetched = match self.get(url, gate, context, "advisories").await {
                Ok(fetched) => fetched,
                Err(failure) => return (advisories, Err(failure)),
            };
            match serde_json::from_slice::<Vec<GhAdvisory>>(&fetched.body) {
                Ok(page) => advisories.extend(page),
                Err(error) => return (advisories, Err(bad_json(&error))),
            }
            match self.next_page(&fetched) {
                Ok(page) => next = page,
                Err(failure) => return (advisories, Err(failure)),
            }
        }
        match next {
            None => (advisories, Ok(())),
            Some(_) => (
                advisories,
                Err(SourceFailure::BadAnswer {
                    detail: format!("more than {MAX_PAGES} pages"),
                }),
            ),
        }
    }

    /// The next page's address from the answer's `Link` header, read against the advisories
    /// address; one with another scheme, host, port, or path, or with a user name, is refused.
    fn next_page(&self, fetched: &Fetched) -> Result<Option<Url>, SourceFailure> {
        let link = match fetched.meta.link.as_deref() {
            Some(header) => next_link(header)?,
            None => None,
        };
        let Some(link) = link else {
            return Ok(None);
        };
        let url = self
            .advisories
            .join(link)
            .map_err(|_| SourceFailure::BadAnswer {
                detail: "a next-page link was not an address".to_owned(),
            })?;
        let same_place = url.scheme() == self.advisories.scheme()
            && url.host_str() == self.advisories.host_str()
            && url.port_or_known_default() == self.advisories.port_or_known_default()
            && url.path() == self.advisories.path()
            && url.username().is_empty()
            && url.password().is_none();
        if !same_place {
            debug!("a next-page link to another address");
            return Err(SourceFailure::BadAnswer {
                detail: "a next-page link pointed at another address".to_owned(),
            });
        }
        Ok(Some(url))
    }

    /// The ids of the advisories of `kind` that GitHub lists for `package`'s version asked
    /// alone, every page of them; the ids read before a failure come back with it.
    async fn ids_alone(
        &self,
        ecosystem: Ecosystem,
        package: &Resolved,
        kind: &str,
        gate: &SourceGate,
        context: &RunContext,
    ) -> Alone {
        let (advisories, finished) = self
            .pages(self.chunk_url(ecosystem, &[package], kind), gate, context)
            .await;
        let ids = advisories
            .into_iter()
            .map(|advisory| advisory.ghsa_id)
            .collect();
        (ids, finished)
    }

    /// The first page's address for `chunk` and `kind`: the fixed words, and each
    /// `name@version` percent-encoded and joined with a literal comma.
    fn chunk_url(&self, ecosystem: Ecosystem, chunk: &[&Resolved], kind: &str) -> Url {
        let affects: Vec<String> = chunk
            .iter()
            .map(|package| encode_segment(&format!("{}@{}", package.name, package.version)))
            .collect();
        let mut url = self.advisories.clone();
        url.set_query(Some(&format!(
            "ecosystem={}&type={kind}&per_page=100&affects={}",
            ecosystem.github_ecosystem(),
            affects.join(",")
        )));
        url
    }

    /// One GET, unless the source has stopped. A 403 or 429 is a block by GitHub's rules; a
    /// 2xx that leaves no requests is used and then blocks; any other status is a failure.
    async fn get(
        &self,
        url: Url,
        gate: &SourceGate,
        context: &RunContext,
        path_kind: &'static str,
    ) -> Result<Fetched, SourceFailure> {
        gate.open()?;
        let outcome = match fetch(self.client.get(url), &self.limits, path_kind).await {
            Ok(fetched) => {
                debug!(
                    ratelimit_remaining = fetched.meta.ratelimit_remaining,
                    ratelimit_reset = fetched.meta.ratelimit_reset,
                    "github rate limit"
                );
                let block = github_block_until(
                    (context.clock)(),
                    fetched.status,
                    &fetched.meta,
                    context.strikes,
                );
                match (fetched.status, block) {
                    (200..=299, Some(until)) => {
                        gate.record::<()>(&Err(SourceFailure::RateLimited { until }));
                        Ok(fetched)
                    }
                    (200..=299, None) => Ok(fetched),
                    (_, Some(until)) => Err(SourceFailure::RateLimited { until }),
                    (code, None) => Err(SourceFailure::Status { code }),
                }
            }
            Err(failure) => Err(failure),
        };
        gate.record(&outcome);
        outcome
    }
}

/// Splits `packages` into chunks of at most [`CHUNK_SIZE`] that never hold one name twice: a
/// second version of a name goes into the next chunk that lacks it.
fn chunks(ecosystem: Ecosystem, packages: &[Resolved]) -> Vec<Vec<&Resolved>> {
    let mut chunks: Vec<Vec<&Resolved>> = Vec::new();
    for package in packages {
        let slot = chunks.iter_mut().find(|chunk| {
            chunk.len() < CHUNK_SIZE
                && !chunk
                    .iter()
                    .any(|other| ecosystem.same_package(&other.name, &package.name))
        });
        match slot {
            Some(chunk) => chunk.push(package),
            None => chunks.push(vec![package]),
        }
    }
    chunks
}

/// The chunk packages an advisory lists among its vulnerabilities.
fn named_packages<'a>(
    advisory: &GhAdvisory,
    ecosystem: Ecosystem,
    chunk: &[&'a Resolved],
) -> Vec<&'a Resolved> {
    chunk
        .iter()
        .filter(|package| {
            advisory
                .entries_for(ecosystem, &package.name)
                .next()
                .is_some()
        })
        .copied()
        .collect()
}

fn keep(progress: &mut Progress, advisory: &GhAdvisory, ecosystem: Ecosystem, package: &Resolved) {
    progress
        .found
        .entry(package.key)
        .or_default()
        .push(advisory_for(advisory, ecosystem, &package.name));
}

/// The URL of the link whose `rel` holds `next` in a `Link` header, or a failure when a link's
/// `<` is never closed. The URLs hold commas, so the header is scanned link by link instead of
/// split on `,`.
pub(crate) fn next_link(header: &str) -> Result<Option<&str>, SourceFailure> {
    let mut rest = header;
    while let Some(open) = rest.find('<') {
        let after_open = &rest[open + 1..];
        let close = after_open
            .find('>')
            .ok_or_else(|| SourceFailure::BadAnswer {
                detail: "a Link header could not be read".to_owned(),
            })?;
        let url = &after_open[..close];
        let tail = &after_open[close + 1..];
        let params_end = tail.find('<').unwrap_or(tail.len());
        let is_next = tail[..params_end]
            .split(';')
            .map(|param| param.trim().trim_end_matches(',').trim())
            .any(is_rel_next);
        if is_next {
            return Ok(Some(url));
        }
        rest = &tail[params_end..];
    }
    Ok(None)
}

/// Whether one `Link` parameter is a `rel` whose space-separated values hold `next`, in any
/// case, quoted or not.
fn is_rel_next(param: &str) -> bool {
    let Some((name, value)) = param.split_once('=') else {
        return false;
    };
    name.trim().eq_ignore_ascii_case("rel")
        && value
            .trim()
            .trim_matches('"')
            .split_ascii_whitespace()
            .any(|rel| rel.eq_ignore_ascii_case("next"))
}

/// `advisory` as GitHub's advisory for the package `name`: its aliases, its label, its CVSS
/// ratings, its EPSS values, and the first patched version of each of this package's entries.
pub(crate) fn advisory_for(
    advisory: &GhAdvisory,
    ecosystem: Ecosystem,
    name: &str,
) -> SourceAdvisory {
    let mut aliases: Vec<String> = Vec::new();
    let other_ids = advisory.cve_id.iter().chain(
        advisory
            .identifiers
            .iter()
            .flatten()
            .filter_map(|identifier| identifier.value.as_ref()),
    );
    for id in other_ids {
        if !id.is_empty() && *id != advisory.ghsa_id && !aliases.contains(id) {
            aliases.push(id.clone());
        }
    }
    let mut fixed_in: Vec<String> = Vec::new();
    for entry in advisory.entries_for(ecosystem, name) {
        if let Some(version) = entry
            .first_patched_version
            .as_ref()
            .filter(|v| !v.is_empty())
            && !fixed_in.contains(version)
        {
            fixed_in.push(version.clone());
        }
    }
    let cvss = advisory
        .cvss_severities
        .as_ref()
        .map(|ratings| {
            [("3", &ratings.cvss_v3), ("4", &ratings.cvss_v4)]
                .into_iter()
                .filter_map(|(version, rating)| {
                    rating.as_ref().and_then(|rating| rating.as_cvss(version))
                })
                .collect()
        })
        .unwrap_or_default();
    SourceAdvisory {
        source: SourceName::GitHub,
        id: advisory.ghsa_id.clone(),
        aliases,
        kind: if advisory.kind == "malware" {
            AdvisoryKind::Malware
        } else {
            AdvisoryKind::Vulnerability
        },
        summary: advisory
            .summary
            .clone()
            .filter(|summary| !summary.is_empty()),
        severity: advisory.severity.as_deref().and_then(severity_label),
        cvss,
        epss: advisory.epss.as_ref().and_then(epss_of),
        fixed_in,
        urls: advisory.html_url.iter().cloned().collect(),
        published: advisory.published_at,
    }
}

/// GitHub's severity word as a level; "unknown" and any other word give none.
fn severity_label(word: &str) -> Option<Severity> {
    match word {
        "critical" => Some(Severity::Critical),
        "high" => Some(Severity::High),
        "medium" => Some(Severity::Medium),
        "low" => Some(Severity::Low),
        _ => None,
    }
}

/// The EPSS values when both are numbers (or numeric strings). GitHub's schema and changelog
/// disagree on their scale, so they are kept as sent.
fn epss_of(value: &Value) -> Option<Epss> {
    let number = |key: &str| match value.get(key)? {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse().ok(),
        Value::Null | Value::Bool(_) | Value::Array(_) | Value::Object(_) => None,
    };
    Some(Epss {
        percentage: number("percentage")?,
        percentile: number("percentile")?,
    })
}

/// An advisory, the fields the tool reads. Every list can be `null`: "Blank fields are
/// included as `null` instead of being omitted".
#[derive(Debug, Deserialize)]
pub(crate) struct GhAdvisory {
    pub ghsa_id: String,
    pub cve_id: Option<String>,
    pub html_url: Option<String>,
    pub summary: Option<String>,
    #[serde(rename = "type")]
    pub kind: String,
    pub severity: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub withdrawn_at: Option<DateTime<Utc>>,
    pub identifiers: Option<Vec<GhIdentifier>>,
    pub vulnerabilities: Option<Vec<GhVulnerability>>,
    pub cvss_severities: Option<GhCvssSeverities>,
    /// Kept loose: an object with numbers per the schema, strings in older answers.
    pub epss: Option<Value>,
}

impl GhAdvisory {
    /// The vulnerability entries for the package `name`, one per affected version range.
    fn entries_for<'a>(
        &'a self,
        ecosystem: Ecosystem,
        name: &'a str,
    ) -> impl Iterator<Item = &'a GhVulnerability> {
        self.vulnerabilities.iter().flatten().filter(move |entry| {
            entry.package.as_ref().is_some_and(|package| {
                package.ecosystem.as_deref() == Some(ecosystem.github_ecosystem())
                    && package
                        .name
                        .as_deref()
                        .is_some_and(|entry_name| ecosystem.same_package(entry_name, name))
            })
        })
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct GhIdentifier {
    pub value: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GhVulnerability {
    pub package: Option<GhPackage>,
    pub first_patched_version: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GhPackage {
    pub ecosystem: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GhCvssSeverities {
    pub cvss_v3: Option<GhCvss>,
    pub cvss_v4: Option<GhCvss>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GhCvss {
    pub vector_string: Option<String>,
    pub score: Option<f64>,
}

impl GhCvss {
    /// The rating, or none for GitHub's way of sending no rating: no vector and a score of 0.
    fn as_cvss(&self, version: &str) -> Option<Cvss> {
        let vector = self
            .vector_string
            .clone()
            .filter(|vector| !vector.is_empty());
        let score = self.score.filter(|score| vector.is_some() || *score != 0.0);
        (vector.is_some() || score.is_some()).then(|| Cvss {
            version: version.to_owned(),
            vector,
            score,
            from: SourceName::GitHub,
        })
    }
}

#[cfg(test)]
mod tests;

//! deps.dev (API v3): each package's latest version, each version's supply-chain facts, and
//! its advisories.

use crate::advisory::{Cvss, SourceAdvisory, SourceName, kind_of_id};
use crate::ecosystem::Ecosystem;
use crate::engine::{PackageAnswer, PackageKey, Resolved, RunContext, SourceRun};
use crate::http::{
    Answer, HttpLimits, SourceFailure, SourceGate, get_json, segment_url, unaskable_id,
};
use crate::package::check_version;
use crate::report::{Attestations, ScorecardSummary, SourceRepo, SupplyChain};
use chrono::{DateTime, Utc};
use futures_util::{StreamExt, stream};
use reqwest::Client;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use tracing::instrument;
use url::Url;

/// deps.dev's address.
pub const DEPS_DEV_BASE: &str = "https://api.deps.dev/";
/// Requests in flight at once.
const IN_FLIGHT: usize = 4;
/// The `relationType` of a version's source repository.
const SOURCE_REPO: &str = "SOURCE_REPO";

/// What deps.dev says a package's latest version is.
#[derive(Debug, Clone, PartialEq)]
pub enum Latest {
    /// The version deps.dev installs when none is given, checked by the version rule.
    Version(String),
    /// deps.dev does not know the package.
    NotFound,
    Failed(SourceFailure),
}

/// The latest-version lookups of one check. The rest of deps.dev's run continues from it, so
/// its requests, its block, and its failures in a row count for the whole run.
#[derive(Debug, Default)]
pub struct LatestRun {
    pub answers: BTreeMap<PackageKey, Latest>,
    gate: SourceGate,
}

/// Asks deps.dev.
#[derive(Debug, Clone)]
pub struct DepsDevClient {
    client: Client,
    base: Url,
    limits: HttpLimits,
}

impl DepsDevClient {
    /// A client for the deps.dev at `base`, a host-only address ending in `/`.
    pub fn new(client: Client, base: Url, limits: HttpLimits) -> Self {
        Self {
            client,
            base,
            limits,
        }
    }

    /// The latest version of each package, four lookups at a time. A version that fails the
    /// input rule is a failure: it comes from the package's registry and would reach a URL.
    #[instrument(skip_all, fields(source = "deps_dev"))]
    pub async fn latest_versions(
        &self,
        ecosystem: Ecosystem,
        packages: &[(PackageKey, String)],
        context: &RunContext,
    ) -> LatestRun {
        let gate = SourceGate::default();
        let answers: Vec<(PackageKey, Latest)> = stream::iter(packages.to_vec())
            .map(|(key, name): (PackageKey, String)| {
                let gate = &gate;
                async move { (key, self.latest(ecosystem, &name, gate, context).await) }
            })
            .buffer_unordered(IN_FLIGHT)
            .collect()
            .await;
        LatestRun {
            answers: answers.into_iter().collect(),
            gate,
        }
    }

    async fn latest(
        &self,
        ecosystem: Ecosystem,
        name: &str,
        gate: &SourceGate,
        context: &RunContext,
    ) -> Latest {
        let segments = [
            "v3",
            "systems",
            ecosystem.deps_dev_system(),
            "packages",
            name,
        ];
        match self
            .ask::<PackageInfo>(&segments, gate, context, "package")
            .await
        {
            Ok(Answer::Found(info)) => default_version(info),
            Ok(Answer::NotFound) => Latest::NotFound,
            Err(failure) => Latest::Failed(failure),
        }
    }

    /// Each package's version facts, then the projects and advisories they name, four requests
    /// at a time, continuing `prior`. The run also carries every lookup answer of a package
    /// that never got a version, so the lookups count as asking deps.dev.
    #[instrument(skip_all, fields(source = "deps_dev"))]
    pub async fn check(
        &self,
        ecosystem: Ecosystem,
        packages: &[Resolved],
        prior: LatestRun,
        context: &RunContext,
    ) -> (SourceRun, BTreeMap<PackageKey, SupplyChain>) {
        let gate = &prior.gate;
        let versions: BTreeMap<PackageKey, Result<Answer<VersionInfo>, SourceFailure>> =
            stream::iter(packages.to_vec())
                .map(|package: Resolved| async move {
                    let segments = [
                        "v3",
                        "systems",
                        ecosystem.deps_dev_system(),
                        "packages",
                        package.name.as_str(),
                        "versions",
                        package.version.as_str(),
                    ];
                    let outcome = self.ask(&segments, gate, context, "version").await;
                    (package.key, outcome)
                })
                .buffer_unordered(IN_FLIGHT)
                .collect()
                .await;

        let found: Vec<(PackageKey, &VersionInfo)> = versions
            .iter()
            .filter_map(|(key, outcome)| match outcome {
                Ok(Answer::Found(info)) => Some((*key, info)),
                Ok(Answer::NotFound) | Err(_) => None,
            })
            .collect();
        let project_ids: BTreeSet<String> = found
            .iter()
            .filter_map(|(_, info)| source_repo(info).map(|(id, _)| id.to_owned()))
            .collect();
        let advisory_ids: BTreeSet<String> = found
            .iter()
            .flat_map(|(_, info)| info.advisory_keys.iter().map(|key| key.id.clone()))
            .collect();
        // Advisories first: they feed a finding's severity, while a scorecard is only shown
        // beside the verdict, so a block or a stop should cost the scorecards first.
        let advisories = self.advisories(&advisory_ids, gate, context).await;
        let (scorecards, failed_projects) = self.scorecards(project_ids, gate, context).await;
        let details_missing = advisory_ids
            .iter()
            .filter(|id| !advisories.contains_key(*id))
            .count()
            + failed_projects;

        let mut chains = BTreeMap::new();
        let mut answers = BTreeMap::new();
        for (key, outcome) in versions {
            let answer = match outcome {
                Ok(Answer::Found(info)) => {
                    chains.insert(key, supply_chain(&info, &scorecards));
                    PackageAnswer::Answered {
                        advisories: info
                            .advisory_keys
                            .iter()
                            .map(|advisory_key| match advisories.get(&advisory_key.id) {
                                Some(advisory) => advisory.clone(),
                                None => {
                                    SourceAdvisory::id_only(SourceName::DepsDev, &advisory_key.id)
                                }
                            })
                            .collect(),
                    }
                }
                Ok(Answer::NotFound) => PackageAnswer::NotFound,
                Err(failure) => PackageAnswer::Failed {
                    failure,
                    advisories: Vec::new(),
                },
            };
            answers.insert(key, answer);
        }
        for (key, latest) in &prior.answers {
            let answer = match latest {
                Latest::Version(_) => continue,
                Latest::NotFound => PackageAnswer::NotFound,
                Latest::Failed(failure) => PackageAnswer::Failed {
                    failure: failure.clone(),
                    advisories: Vec::new(),
                },
            };
            answers.entry(*key).or_insert(answer);
        }
        let run = SourceRun {
            source: SourceName::DepsDev,
            requests: gate.requests(),
            block: gate.block(),
            details_missing: u32::try_from(details_missing).unwrap_or(u32::MAX),
            answers,
        };
        (run, chains)
    }

    /// Each project's scorecard, and how many projects could not be fetched. A project that
    /// cannot be fetched, deps.dev does not know, or has no scorecard is left out; it never
    /// fails a package.
    async fn scorecards(
        &self,
        ids: BTreeSet<String>,
        gate: &SourceGate,
        context: &RunContext,
    ) -> (HashMap<String, ScorecardSummary>, usize) {
        let outcomes: Vec<(String, Result<Answer<ProjectInfo>, SourceFailure>)> = stream::iter(ids)
            .map(|id: String| async move {
                let outcome = self
                    .ask(&["v3", "projects", id.as_str()], gate, context, "project")
                    .await;
                (id, outcome)
            })
            .buffer_unordered(IN_FLIGHT)
            .collect()
            .await;
        let mut scorecards = HashMap::new();
        let mut failed = 0;
        for (id, outcome) in outcomes {
            match outcome {
                Ok(Answer::Found(project)) => {
                    if let Some(card) = scorecard_of(project) {
                        scorecards.insert(id, card);
                    }
                }
                Ok(Answer::NotFound) => {}
                // Counted as a missing detail; the package's verdict does not depend on it.
                Err(_) => failed += 1,
            }
        }
        (scorecards, failed)
    }

    /// Each advisory as deps.dev describes it. One that cannot be fetched is left out, and the
    /// caller keeps it as an id only.
    async fn advisories(
        &self,
        ids: &BTreeSet<String>,
        gate: &SourceGate,
        context: &RunContext,
    ) -> HashMap<String, SourceAdvisory> {
        let outcomes: Vec<(String, Result<Answer<AdvisoryInfo>, SourceFailure>)> =
            stream::iter(ids.iter().cloned())
                .map(|id: String| async move {
                    let outcome = self
                        .ask(
                            &["v3", "advisories", id.as_str()],
                            gate,
                            context,
                            "advisory",
                        )
                        .await;
                    (id, outcome)
                })
                .buffer_unordered(IN_FLIGHT)
                .collect()
                .await;
        outcomes
            .into_iter()
            .filter_map(|(id, outcome)| match outcome {
                Ok(Answer::Found(info)) if info.is_about(&id) => {
                    let advisory = advisory_from_info(&id, info);
                    Some((id, advisory))
                }
                // Counted as a missing detail by the caller; the id alone still names it.
                Ok(Answer::Found(_) | Answer::NotFound) | Err(_) => None,
            })
            .collect()
    }

    /// GETs the path made of `segments` unless the source has stopped.
    async fn ask<T: DeserializeOwned>(
        &self,
        segments: &[&str],
        gate: &SourceGate,
        context: &RunContext,
        path_kind: &'static str,
    ) -> Result<Answer<T>, SourceFailure> {
        let url = segment_url(&self.base, segments).ok_or_else(unaskable_id)?;
        gate.open()?;
        let outcome = get_json(&self.client, url, &self.limits, &context.clock, path_kind).await;
        gate.record(&outcome);
        outcome
    }
}

fn default_version(info: PackageInfo) -> Latest {
    let Some(entry) = info.versions.into_iter().find(|entry| entry.is_default) else {
        return Latest::Failed(SourceFailure::BadAnswer {
            detail: "deps.dev names no default version".to_owned(),
        });
    };
    let version = entry.version_key.version;
    match check_version(&version) {
        Ok(()) => Latest::Version(version),
        // The rule's own message quotes the refused character; the version is the publisher's
        // text, so the failure keeps fixed words only.
        Err(_) => Latest::Failed(SourceFailure::BadAnswer {
            detail: "deps.dev named a latest version this tool does not accept".to_owned(),
        }),
    }
}

/// The first related project with an id whose relation is the source repository, with its
/// provenance.
fn source_repo(info: &VersionInfo) -> Option<(&str, Option<&str>)> {
    info.related_projects
        .iter()
        .find(|project| {
            project.relation_type.as_deref() == Some(SOURCE_REPO)
                && !project.project_key.id.is_empty()
        })
        .map(|project| {
            (
                project.project_key.id.as_str(),
                project.relation_provenance.as_deref(),
            )
        })
}

fn supply_chain(info: &VersionInfo, scorecards: &HashMap<String, ScorecardSummary>) -> SupplyChain {
    let repo = source_repo(info);
    let all_attestations = info.attestations.iter().chain(info.slsa_provenances.iter());
    let (total, verified) = all_attestations.fold((0u32, 0u32), |(total, verified), item| {
        (
            total.saturating_add(1),
            verified.saturating_add(u32::from(item.verified)),
        )
    });
    SupplyChain {
        latest: info.is_default,
        deprecated: info
            .is_deprecated
            .then(|| info.deprecated_reason.clone().unwrap_or_default()),
        licenses: info
            .licenses
            .iter()
            .filter(|license| !license.is_empty())
            .cloned()
            .collect(),
        published_at: info.published_at,
        source_repo: repo.map(|(id, provenance)| SourceRepo {
            id: id.to_owned(),
            provenance: provenance
                .filter(|provenance| !provenance.is_empty())
                .map_or_else(|| "unknown".to_owned(), str::to_ascii_lowercase),
        }),
        scorecard: repo.and_then(|(id, _)| scorecards.get(id).cloned()),
        attestations: Attestations { total, verified },
    }
}

fn scorecard_of(project: ProjectInfo) -> Option<ScorecardSummary> {
    let scorecard = project.scorecard?;
    scorecard.overall_score.map(|score| ScorecardSummary {
        score,
        date: scorecard.date,
    })
}

/// `text` unless it is empty: deps.dev sends an unset text field as "".
fn non_empty(text: Option<String>) -> Option<String> {
    text.filter(|text| !text.is_empty())
}

fn advisory_from_info(id: &str, info: AdvisoryInfo) -> SourceAdvisory {
    // deps.dev sends an advisory with no CVSS 3 rating as score 0 with an empty vector; a real
    // 0.0 rating comes with its vector.
    let vector = non_empty(info.cvss3_vector);
    let score = info
        .cvss3_score
        .filter(|score| vector.is_some() || *score != 0.0);
    let cvss = if vector.is_some() || score.is_some() {
        vec![Cvss {
            version: "3".to_owned(),
            vector,
            score,
            from: SourceName::DepsDev,
        }]
    } else {
        Vec::new()
    };
    SourceAdvisory {
        source: SourceName::DepsDev,
        id: id.to_owned(),
        aliases: info
            .aliases
            .into_iter()
            .filter(|alias| !alias.is_empty())
            .collect(),
        kind: kind_of_id(id),
        summary: non_empty(info.title),
        severity: None,
        cvss,
        epss: None,
        fixed_in: Vec::new(),
        urls: non_empty(info.url).into_iter().collect(),
        published: None,
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackageInfo {
    #[serde(default)]
    versions: Vec<VersionEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VersionEntry {
    version_key: VersionKey,
    #[serde(default)]
    is_default: bool,
}

#[derive(Debug, Deserialize)]
struct VersionKey {
    version: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VersionInfo {
    published_at: Option<DateTime<Utc>>,
    #[serde(default)]
    is_default: bool,
    #[serde(default)]
    is_deprecated: bool,
    deprecated_reason: Option<String>,
    #[serde(default)]
    licenses: Vec<String>,
    #[serde(default)]
    advisory_keys: Vec<AdvisoryKey>,
    #[serde(default)]
    related_projects: Vec<RelatedProject>,
    #[serde(default)]
    attestations: Vec<Attestation>,
    /// The same `verified` flag; deps.dev fills it for npm only.
    #[serde(default)]
    slsa_provenances: Vec<Attestation>,
}

#[derive(Debug, Deserialize)]
struct AdvisoryKey {
    id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RelatedProject {
    project_key: ProjectKey,
    relation_provenance: Option<String>,
    relation_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProjectKey {
    id: String,
}

#[derive(Debug, Deserialize)]
struct Attestation {
    #[serde(default)]
    verified: bool,
}

#[derive(Debug, Deserialize)]
struct ProjectInfo {
    scorecard: Option<Scorecard>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Scorecard {
    date: Option<DateTime<Utc>>,
    overall_score: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AdvisoryInfo {
    advisory_key: Option<AdvisoryKey>,
    url: Option<String>,
    title: Option<String>,
    #[serde(default)]
    aliases: Vec<String>,
    cvss3_score: Option<f64>,
    cvss3_vector: Option<String>,
}

impl AdvisoryInfo {
    /// Whether the answer describes `id`: it names that id, or names none.
    fn is_about(&self, id: &str) -> bool {
        self.advisory_key.as_ref().is_none_or(|key| key.id == id)
    }
}

#[cfg(test)]
mod tests;

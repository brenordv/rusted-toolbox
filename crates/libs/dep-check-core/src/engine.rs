//! The check: asks every source about every package, saves and logs what each source did, and
//! merges the answers into one [`Report`].

use crate::advisory::{SourceAdvisory, SourceName};
use crate::ecosystem::Ecosystem;
use crate::http::{HttpLimits, SourceFailure, build_client, elapsed_ms};
use crate::merge::merge;
use crate::offline::{OfflineUse, offline_dir};
use crate::package::PackageSpec;
use crate::rate_limits::{self, rate_limits_path};
use crate::report::{
    AnswerWord, Outcome, PackageReport, PackageStatus, REPORT_VERSION, Report, Summary,
    SupplyChain, VersionFrom,
};
use crate::sources::deps_dev::{DEPS_DEV_BASE, DepsDevClient, Latest, LatestRun};
use crate::sources::github::{GITHUB_BASE, GitHubClient, github_headers};
use crate::sources::offline::{OfflineRead, OfflineSource, failed_run, joined};
use crate::sources::osv::{OSV_BASE, OsvClient};
use anyhow::{Context, Result};
use chrono::{DateTime, TimeDelta, Utc};
use reqwest::header::HeaderMap;
use settle::{Settled, Store, not_asked, settle, unasked};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tracing::info;
use url::Url;

mod settle;

#[cfg(test)]
use settle::source_status;

/// The time source a check reads when it needs "now".
pub type Clock = Arc<dyn Fn() -> DateTime<Utc> + Send + Sync>;

/// The system clock.
pub fn system_clock() -> Clock {
    Arc::new(Utc::now)
}

/// What a source needs for one run besides the packages.
#[derive(Clone)]
pub struct RunContext {
    /// Read when an answer arrives, so a block counts from then.
    pub clock: Clock,
    /// The source's blocks in a row already saved; GitHub doubles its wait by them.
    pub strikes: u32,
}

/// A package's place in the request's package list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackageKey(pub usize);

/// A package with the version a source is asked about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub key: PackageKey,
    pub name: String,
    pub version: String,
}

/// One check: an ecosystem, its packages, and how offline data is used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckRequest {
    pub ecosystem: Ecosystem,
    pub packages: Vec<PackageSpec>,
    /// How offline data is used; [`OfflineUse::max_age`] is how long ago the data may have been
    /// confirmed current for a no-finding answer from it to count.
    pub offline: OfflineUse,
}

/// What one source said about one package.
#[derive(Debug, Clone, PartialEq)]
pub enum PackageAnswer {
    /// The source finished every request about the package.
    Answered { advisories: Vec<SourceAdvisory> },
    /// deps.dev does not know the package or version.
    NotFound,
    /// The source stopped before it finished; `advisories` holds what it had found by then.
    Failed {
        failure: SourceFailure,
        advisories: Vec<SourceAdvisory>,
    },
}

impl PackageAnswer {
    /// The advisories the answer holds, finished or not.
    pub fn advisories(&self) -> &[SourceAdvisory] {
        match self {
            PackageAnswer::Answered { advisories } | PackageAnswer::Failed { advisories, .. } => {
                advisories
            }
            PackageAnswer::NotFound => &[],
        }
    }

    /// Why the source did not finish, when it did not.
    pub fn failure(&self) -> Option<&SourceFailure> {
        match self {
            PackageAnswer::Failed { failure, .. } => Some(failure),
            PackageAnswer::Answered { .. } | PackageAnswer::NotFound => None,
        }
    }

    /// The answer in a word; a failure because of a block is `RateLimited`.
    pub fn word(&self) -> AnswerWord {
        match self {
            PackageAnswer::Answered { .. } => AnswerWord::Answered,
            PackageAnswer::NotFound => AnswerWord::NotFound,
            PackageAnswer::Failed {
                failure: SourceFailure::RateLimited { .. },
                ..
            } => AnswerWord::RateLimited,
            PackageAnswer::Failed {
                failure:
                    SourceFailure::Unreachable { .. }
                    | SourceFailure::TimedOut
                    | SourceFailure::Interrupted
                    | SourceFailure::Status { .. }
                    | SourceFailure::BadAnswer { .. }
                    | SourceFailure::OfflineMissing
                    | SourceFailure::OfflineStale { .. }
                    | SourceFailure::OfflineUnreadable { .. }
                    | SourceFailure::OfflineUndecided
                    | SourceFailure::OfflineBusy
                    | SourceFailure::OfflineStopped,
                ..
            } => AnswerWord::Failed,
        }
    }
}

/// Everything one source did in one check.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceRun {
    pub source: SourceName,
    pub requests: u32,
    /// The block this run hit, if any; once set the source sent nothing more.
    pub block: Option<DateTime<Utc>>,
    /// Details that could not be fetched: an advisory known by its id only, or a source
    /// repository whose scorecard request failed.
    pub details_missing: u32,
    /// For offline data, when it was last confirmed current.
    pub data_as_of: Option<DateTime<Utc>>,
    pub answers: BTreeMap<PackageKey, PackageAnswer>,
}

impl SourceRun {
    /// A run that sent nothing and answered nothing.
    pub fn empty(source: SourceName) -> Self {
        Self {
            source,
            requests: 0,
            block: None,
            details_missing: 0,
            data_as_of: None,
            answers: BTreeMap::new(),
        }
    }
}

/// Where each service is: the real ones, or the tests' local server.
#[derive(Debug, Clone)]
pub struct Bases {
    pub osv: Url,
    pub deps_dev: Url,
    pub github: Url,
}

impl Bases {
    /// The real services' addresses.
    ///
    /// # Errors
    /// Never in practice: the addresses are constants that parse.
    pub fn real() -> Result<Self> {
        Ok(Self {
            osv: Url::parse(OSV_BASE).context("Failed to read the OSV.dev address")?,
            deps_dev: Url::parse(DEPS_DEV_BASE).context("Failed to read the deps.dev address")?,
            github: Url::parse(GITHUB_BASE).context("Failed to read the GitHub address")?,
        })
    }
}

/// Runs checks against the services and the offline data.
pub struct Checker {
    osv: OsvClient,
    deps_dev: DepsDevClient,
    github: GitHubClient,
    offline: OfflineSource,
    limits_path: Option<PathBuf>,
    tool: String,
}

/// What the sources did in one check, before the per-package rows are built.
struct Ran {
    versions: BTreeMap<PackageKey, (String, VersionFrom)>,
    /// OSV.dev, deps.dev, GitHub, then offline data when the check used it.
    settled: Vec<Settled>,
    chains: BTreeMap<PackageKey, SupplyChain>,
}

impl Checker {
    /// The real services, the rate-limit file, and the offline stores in the toolbox home.
    /// `tool` names the tool and its version in the report; `user_agent` goes on every
    /// request.
    ///
    /// # Errors
    /// When the HTTP client cannot be built.
    pub fn new(tool: &str, user_agent: &str) -> Result<Self> {
        Self::with_bases(
            tool,
            user_agent,
            Bases::real()?,
            rate_limits_path(),
            offline_dir(),
        )
    }

    /// The services at `bases`, the rate-limit file at `limits_path`, and the offline stores in
    /// `offline_dir` (none of either when `None`). The crate reads no address from a flag, a
    /// file, or the environment: a query sent somewhere else could come back with a fake
    /// "clean" verdict.
    ///
    /// # Errors
    /// When the HTTP client cannot be built.
    pub fn with_bases(
        tool: &str,
        user_agent: &str,
        bases: Bases,
        limits_path: Option<PathBuf>,
        offline_dir: Option<PathBuf>,
    ) -> Result<Self> {
        let limits = HttpLimits::default();
        let client = build_client(&limits, user_agent, HeaderMap::new())?;
        let github_client = build_client(&limits, user_agent, github_headers())?;
        Ok(Self {
            osv: OsvClient::new(client.clone(), bases.osv, limits)?,
            deps_dev: DepsDevClient::new(client, bases.deps_dev, limits),
            github: GitHubClient::new(github_client, bases.github, limits)?,
            offline: OfflineSource::new(offline_dir),
            limits_path,
            tool: tool.to_owned(),
        })
    }

    /// Checks every package of `request`. Every failure ends inside the report.
    ///
    /// Online (`Off` and `Fallback`), deps.dev first names the latest version of each package
    /// written without one; then OSV.dev, deps.dev, and the GitHub Advisory Database run side
    /// by side, each saved and logged as soon as it finishes. In `Fallback` the packages
    /// OSV.dev did not answer then go to the offline data. In `Only` no service is asked: the
    /// offline data answers every package with a version.
    pub async fn check(&self, request: CheckRequest, clock: Clock) -> Report {
        let checked_at = clock();
        let began = Instant::now();
        info!(
            ecosystem = request.ecosystem.name(),
            packages = request.packages.len(),
            offline = request.offline.name(),
            "check started"
        );
        let loaded = rate_limits::load(self.limits_path.as_deref(), checked_at).await;
        let store = Store {
            path: self.limits_path.as_deref(),
            loaded: &loaded,
            clock: Arc::clone(&clock),
            lock: tokio::sync::Mutex::new(()),
        };
        let ran = match request.offline {
            OfflineUse::Only => self.offline_only(&request, &store, &clock).await,
            OfflineUse::Off | OfflineUse::Fallback => {
                self.online(&request, &store, &clock, checked_at).await
            }
        };
        let packages: Vec<PackageReport> = request
            .packages
            .iter()
            .enumerate()
            .map(|(index, spec)| {
                let key = PackageKey(index);
                let version = ran.versions.get(&key);
                let chain = ran.chains.get(&key);
                package_report(key, spec, version, &ran.settled, chain, request.offline)
            })
            .collect();
        let statuses: Vec<PackageStatus> = packages.iter().map(|package| package.status).collect();
        let report = Report {
            report_version: REPORT_VERSION,
            tool: self.tool.clone(),
            checked_at,
            ecosystem: request.ecosystem,
            offline: request.offline,
            outcome: Outcome::from_statuses(&statuses),
            summary: Summary::of(&packages),
            sources: ran
                .settled
                .into_iter()
                .map(|settled| settled.report)
                .collect(),
            packages,
        };
        info!(
            outcome = report.outcome.name(),
            packages = report.summary.packages,
            vulnerable = report.summary.vulnerable,
            clean = report.summary.clean,
            not_found = report.summary.not_found,
            not_checked = report.summary.not_checked,
            findings = report.summary.findings,
            elapsed_ms = elapsed_ms(began),
            "check finished"
        );
        report
    }

    /// The services, and in `Fallback` the offline data for what OSV.dev did not answer.
    async fn online(
        &self,
        request: &CheckRequest,
        store: &Store<'_>,
        clock: &Clock,
        checked_at: DateTime<Utc>,
    ) -> Ran {
        let loaded = store.loaded;
        let context_for = |source: SourceName| RunContext {
            clock: Arc::clone(clock),
            strikes: loaded.strikes(source),
        };
        let (osv_context, deps_dev_context, github_context) = (
            context_for(SourceName::Osv),
            context_for(SourceName::DepsDev),
            context_for(SourceName::GitHub),
        );
        let ecosystem = request.ecosystem;
        let every_key: Vec<PackageKey> = (0..request.packages.len()).map(PackageKey).collect();

        let deps_dev_began = Instant::now();
        let deps_dev_unasked = unasked(SourceName::DepsDev, &every_key, loaded, checked_at);
        let unversioned = unversioned_packages(&request.packages);
        let latest = if deps_dev_unasked.is_none() && !unversioned.is_empty() {
            self.deps_dev
                .latest_versions(ecosystem, &unversioned, &deps_dev_context)
                .await
        } else {
            LatestRun::default()
        };
        let versions = versions_of(&request.packages, &latest);
        let resolved = resolved_packages(&request.packages, &versions);
        let resolved_keys: Vec<PackageKey> = resolved.iter().map(|package| package.key).collect();

        let osv = async {
            let settled = match unasked(SourceName::Osv, &resolved_keys, loaded, checked_at) {
                Some(settled) => settled,
                None => {
                    let source_began = Instant::now();
                    let run = self.osv.check(ecosystem, &resolved, &osv_context).await;
                    settle(run, source_began, store).await
                }
            };
            let offline = match request.offline {
                OfflineUse::Fallback => Some(
                    self.fallback(&settled.run, &resolved, request, store, clock)
                        .await,
                ),
                OfflineUse::Off | OfflineUse::Only => None,
            };
            (settled, offline)
        };
        let deps_dev = async {
            match deps_dev_unasked {
                Some(settled) => (settled, BTreeMap::new()),
                None => {
                    let (run, chains) = self
                        .deps_dev
                        .check(ecosystem, &resolved, latest, &deps_dev_context)
                        .await;
                    (settle(run, deps_dev_began, store).await, chains)
                }
            }
        };
        let github = async {
            match unasked(SourceName::GitHub, &resolved_keys, loaded, checked_at) {
                Some(settled) => settled,
                None => {
                    let source_began = Instant::now();
                    let run = self
                        .github
                        .check(ecosystem, &resolved, &github_context)
                        .await;
                    settle(run, source_began, store).await
                }
            }
        };
        let ((osv, offline), (deps_dev, chains), github) = tokio::join!(osv, deps_dev, github);
        let mut settled = vec![osv, deps_dev, github];
        settled.extend(offline);
        Ran {
            versions,
            settled,
            chains,
        }
    }

    /// The offline data's part of a `Fallback` check: the packages OSV.dev did not answer,
    /// whether it failed or a saved block kept it from being asked. Not asked when OSV.dev
    /// answered every package or the ecosystem has no data.
    async fn fallback(
        &self,
        osv: &SourceRun,
        resolved: &[Resolved],
        request: &CheckRequest,
        store: &Store<'_>,
        clock: &Clock,
    ) -> Settled {
        let unanswered: Vec<Resolved> = resolved
            .iter()
            .filter(|package| {
                !matches!(
                    osv.answers.get(&package.key),
                    Some(PackageAnswer::Answered { .. })
                )
            })
            .cloned()
            .collect();
        if unanswered.is_empty() {
            return not_asked(SourceName::Offline, store.loaded);
        }
        let began = Instant::now();
        let read = self
            .read_offline(
                request.ecosystem,
                unanswered,
                request.offline.max_age(),
                clock(),
            )
            .await;
        match read {
            OfflineRead::NoData => not_asked(SourceName::Offline, store.loaded),
            OfflineRead::Run(run) => settle(run, began, store).await,
        }
    }

    /// An `Only` check: no service is asked, so no package name leaves the machine. A package
    /// without a version cannot be checked; with no data at all every other package fails as
    /// missing.
    async fn offline_only(&self, request: &CheckRequest, store: &Store<'_>, clock: &Clock) -> Ran {
        let mut settled: Vec<Settled> = [SourceName::Osv, SourceName::DepsDev, SourceName::GitHub]
            .into_iter()
            .map(|source| not_asked(source, store.loaded))
            .collect();
        let versions = versions_of(&request.packages, &LatestRun::default());
        let resolved = resolved_packages(&request.packages, &versions);
        let offline = if resolved.is_empty() {
            not_asked(SourceName::Offline, store.loaded)
        } else {
            let began = Instant::now();
            let keys: Vec<PackageKey> = resolved.iter().map(|package| package.key).collect();
            let read = self
                .read_offline(
                    request.ecosystem,
                    resolved,
                    request.offline.max_age(),
                    clock(),
                )
                .await;
            let run = match read {
                OfflineRead::NoData => failed_run(&keys, &SourceFailure::OfflineMissing),
                OfflineRead::Run(run) => run,
            };
            settle(run, began, store).await
        };
        settled.push(offline);
        Ran {
            versions,
            settled,
            chains: BTreeMap::new(),
        }
    }

    /// The offline source's answers for `packages`, read on a blocking thread.
    async fn read_offline(
        &self,
        ecosystem: Ecosystem,
        packages: Vec<Resolved>,
        max_age: TimeDelta,
        now: DateTime<Utc>,
    ) -> OfflineRead {
        let keys: Vec<PackageKey> = packages.iter().map(|package| package.key).collect();
        let source = self.offline.clone();
        let read =
            tokio::task::spawn_blocking(move || source.check(ecosystem, &packages, max_age, now))
                .await;
        joined(read, &keys)
    }
}

/// The packages written without a version, with their names.
fn unversioned_packages(packages: &[PackageSpec]) -> Vec<(PackageKey, String)> {
    packages
        .iter()
        .enumerate()
        .filter(|(_, spec)| spec.version.is_none())
        .map(|(index, spec)| (PackageKey(index), spec.name.clone()))
        .collect()
}

/// The version each package is checked at: the one written, or deps.dev's latest for a package
/// written without one. A package with neither is not in the map.
fn versions_of(
    packages: &[PackageSpec],
    latest: &LatestRun,
) -> BTreeMap<PackageKey, (String, VersionFrom)> {
    let mut versions = BTreeMap::new();
    for (index, spec) in packages.iter().enumerate() {
        let key = PackageKey(index);
        if let Some(version) = &spec.version {
            versions.insert(key, (version.clone(), VersionFrom::Input));
        } else if let Some(Latest::Version(version)) = latest.answers.get(&key) {
            versions.insert(key, (version.clone(), VersionFrom::DepsDevLatest));
        }
    }
    versions
}

/// The packages the sources are asked about, in list order.
fn resolved_packages(
    packages: &[PackageSpec],
    versions: &BTreeMap<PackageKey, (String, VersionFrom)>,
) -> Vec<Resolved> {
    versions
        .iter()
        .map(|(key, (version, _))| Resolved {
            key: *key,
            name: packages[key.0].name.clone(),
            version: version.clone(),
        })
        .collect()
}

/// One package's row, merged from every settled source's answer for it. `version` is the one
/// the sources were asked about, if any; `offline` is how the check used offline data.
fn package_report(
    key: PackageKey,
    spec: &PackageSpec,
    version: Option<&(String, VersionFrom)>,
    settled: &[Settled],
    supply_chain: Option<&SupplyChain>,
    offline: OfflineUse,
) -> PackageReport {
    let answer_words: BTreeMap<SourceName, AnswerWord> = settled
        .iter()
        .map(|part| {
            let word = part
                .run
                .answers
                .get(&key)
                .map_or(AnswerWord::NotAsked, PackageAnswer::word);
            (part.run.source, word)
        })
        .collect();
    let answers: Vec<(SourceName, &PackageAnswer)> = settled
        .iter()
        .filter_map(|part| {
            part.run
                .answers
                .get(&key)
                .map(|answer| (part.run.source, answer))
        })
        .collect();
    let Some((version, version_from)) = version else {
        let lookup = answers
            .iter()
            .find(|(source, _)| *source == SourceName::DepsDev)
            .map(|(_, answer)| *answer);
        let (status, reason) = without_version(lookup, offline);
        return PackageReport {
            input: spec.input.clone(),
            name: spec.name.clone(),
            version: None,
            version_from: None,
            status,
            reason,
            findings: Vec::new(),
            supply_chain: None,
            answers: answer_words,
        };
    };
    let verdict = merge(&answers);
    PackageReport {
        input: spec.input.clone(),
        name: spec.name.clone(),
        version: Some(version.clone()),
        version_from: Some(*version_from),
        status: verdict.status,
        reason: verdict.reason,
        findings: verdict.findings,
        supply_chain: supply_chain.cloned(),
        answers: answer_words,
    }
}

/// The reason a package written without a version is not checked offline only: no service is
/// asked to name its latest version.
pub const NEEDS_VERSION: &str = "a version is needed to check offline";

/// The status of a package written without a version that got none. Offline only, it is
/// `NotChecked` because no service was asked; otherwise it follows deps.dev's answer to the
/// lookup: `NotFound` when deps.dev does not know it, else `NotChecked` with the reason.
fn without_version(
    lookup: Option<&PackageAnswer>,
    offline: OfflineUse,
) -> (PackageStatus, Option<String>) {
    match offline {
        OfflineUse::Only => return (PackageStatus::NotChecked, Some(NEEDS_VERSION.to_owned())),
        OfflineUse::Off | OfflineUse::Fallback => {}
    }
    match lookup {
        Some(PackageAnswer::NotFound) => (PackageStatus::NotFound, None),
        Some(PackageAnswer::Failed { failure, .. }) => (
            PackageStatus::NotChecked,
            Some(format!(
                "no version was given, and deps.dev could not name the latest one: it {}",
                failure.describe()
            )),
        ),
        Some(PackageAnswer::Answered { .. }) | None => (
            PackageStatus::NotChecked,
            Some("no version was given, and deps.dev did not name the latest one".to_owned()),
        ),
    }
}

#[cfg(test)]
mod tests;

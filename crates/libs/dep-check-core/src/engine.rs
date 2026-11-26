//! The check: asks every source about every package, saves and logs what each source did, and
//! merges the answers into one [`Report`].

use crate::advisory::{SourceAdvisory, SourceName};
use crate::ecosystem::Ecosystem;
use crate::http::{HttpLimits, SourceFailure, build_client, elapsed_ms};
use crate::merge::merge;
use crate::package::PackageSpec;
use crate::rate_limits::{self, Change, RateLimits, rate_limits_path, save_changes};
use crate::report::{
    AnswerWord, BlockOrigin, FailureGroup, Outcome, PackageReport, PackageStatus, REPORT_VERSION,
    Report, SourceReport, SourceStatus, Summary, SupplyChain, VersionFrom,
};
use crate::sources::deps_dev::{DEPS_DEV_BASE, DepsDevClient, Latest, LatestRun};
use crate::sources::osv::{OSV_BASE, OsvClient};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use reqwest::header::HeaderMap;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tracing::{info, warn};
use url::Url;

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

/// One check: an ecosystem and its packages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckRequest {
    pub ecosystem: Ecosystem,
    pub packages: Vec<PackageSpec>,
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
                    | SourceFailure::BadAnswer { .. },
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
            answers: BTreeMap::new(),
        }
    }
}

/// Where each service is: the real ones, or the tests' local server.
#[derive(Debug, Clone)]
pub struct Bases {
    pub osv: Url,
    pub deps_dev: Url,
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
        })
    }
}

/// Runs checks against the services.
pub struct Checker {
    osv: OsvClient,
    deps_dev: DepsDevClient,
    limits_path: Option<PathBuf>,
    tool: String,
}

impl Checker {
    /// The real services and the rate-limit file in the toolbox home. `tool` names the tool
    /// and its version in the report; `user_agent` goes on every request.
    ///
    /// # Errors
    /// When the HTTP client cannot be built.
    pub fn new(tool: &str, user_agent: &str) -> Result<Self> {
        Self::with_bases(tool, user_agent, Bases::real()?, rate_limits_path())
    }

    /// The services at `bases` and the rate-limit file at `limits_path` (none when `None`).
    /// The crate reads no address from a flag, a file, or the environment: a query sent
    /// somewhere else could come back with a fake "clean" verdict.
    ///
    /// # Errors
    /// When the HTTP client cannot be built.
    pub fn with_bases(
        tool: &str,
        user_agent: &str,
        bases: Bases,
        limits_path: Option<PathBuf>,
    ) -> Result<Self> {
        let limits = HttpLimits::default();
        let client = build_client(&limits, user_agent, HeaderMap::new())?;
        Ok(Self {
            osv: OsvClient::new(client.clone(), bases.osv, limits)?,
            deps_dev: DepsDevClient::new(client, bases.deps_dev, limits),
            limits_path,
            tool: tool.to_owned(),
        })
    }

    /// Checks every package of `request`. Every failure ends inside the report.
    ///
    /// deps.dev first names the latest version of each package written without one; then
    /// OSV.dev and deps.dev run side by side, each saved and logged as soon as it finishes.
    pub async fn check(&self, request: CheckRequest, clock: Clock) -> Report {
        let checked_at = clock();
        let began = Instant::now();
        info!(
            ecosystem = request.ecosystem.name(),
            packages = request.packages.len(),
            "check started"
        );
        let loaded = rate_limits::load(self.limits_path.as_deref(), checked_at).await;
        let store = Store {
            path: self.limits_path.as_deref(),
            loaded: &loaded,
            clock: Arc::clone(&clock),
            lock: tokio::sync::Mutex::new(()),
        };
        let context = RunContext {
            clock: Arc::clone(&clock),
        };
        let ecosystem = request.ecosystem;
        let every_key: Vec<PackageKey> = (0..request.packages.len()).map(PackageKey).collect();

        let deps_dev_began = Instant::now();
        let deps_dev_unasked = unasked(SourceName::DepsDev, &every_key, &loaded, checked_at);
        let unversioned = unversioned_packages(&request.packages);
        let latest = if deps_dev_unasked.is_none() && !unversioned.is_empty() {
            self.deps_dev
                .latest_versions(ecosystem, &unversioned, &context)
                .await
        } else {
            LatestRun::default()
        };
        let versions = versions_of(&request.packages, &latest);
        let resolved = resolved_packages(&request.packages, &versions);
        let resolved_keys: Vec<PackageKey> = resolved.iter().map(|package| package.key).collect();

        let osv = async {
            match unasked(SourceName::Osv, &resolved_keys, &loaded, checked_at) {
                Some(settled) => settled,
                None => {
                    let source_began = Instant::now();
                    let run = self.osv.check(ecosystem, &resolved, &context).await;
                    settle(run, source_began, &store).await
                }
            }
        };
        let deps_dev = async {
            match deps_dev_unasked {
                Some(settled) => (settled, BTreeMap::new()),
                None => {
                    let (run, chains) = self
                        .deps_dev
                        .check(ecosystem, &resolved, latest, &context)
                        .await;
                    (settle(run, deps_dev_began, &store).await, chains)
                }
            }
        };
        let (osv, (deps_dev, chains)) = tokio::join!(osv, deps_dev);

        let settled = [osv, deps_dev];
        let packages: Vec<PackageReport> = request
            .packages
            .iter()
            .enumerate()
            .map(|(index, spec)| {
                let key = PackageKey(index);
                package_report(key, spec, versions.get(&key), &settled, chains.get(&key))
            })
            .collect();
        let statuses: Vec<PackageStatus> = packages.iter().map(|package| package.status).collect();
        let report = Report {
            report_version: REPORT_VERSION,
            tool: self.tool.clone(),
            checked_at,
            ecosystem: request.ecosystem,
            outcome: Outcome::from_statuses(&statuses),
            summary: Summary::of(&packages),
            sources: settled.into_iter().map(|settled| settled.report).collect(),
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
}

/// The rate-limit file and what this check loaded from it.
struct Store<'a> {
    path: Option<&'a Path>,
    loaded: &'a RateLimits,
    clock: Clock,
    /// Held while one source's change is saved, so two sources finishing together do not
    /// write the file at once.
    lock: tokio::sync::Mutex<()>,
}

/// One source's finished part of a check.
struct Settled {
    run: SourceRun,
    report: SourceReport,
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

/// The settled part of a source that is not asked: one no package needs, or one a saved block
/// keeps out (every package in `keys` fails with the block). Logs its line; `None` when the
/// source is to be asked.
fn unasked(
    source: SourceName,
    keys: &[PackageKey],
    loaded: &RateLimits,
    now: DateTime<Utc>,
) -> Option<Settled> {
    if keys.is_empty() {
        let run = SourceRun::empty(source);
        let report = source_report(&run, None);
        log_source(&report, loaded.strikes(source), 0);
        return Some(Settled { run, report });
    }
    if let Some(until) = loaded.blocked_until(source, now) {
        let mut run = SourceRun::empty(source);
        for key in keys {
            run.answers.insert(
                *key,
                PackageAnswer::Failed {
                    failure: SourceFailure::RateLimited { until },
                    advisories: Vec::new(),
                },
            );
        }
        let report = source_report(&run, Some(until));
        log_source(&report, loaded.strikes(source), 0);
        return Some(Settled { run, report });
    }
    None
}

/// Records, saves, and logs one finished source run. The save happens as soon as the source
/// finishes, so its block is on disk without waiting for the slowest source.
async fn settle(run: SourceRun, began: Instant, store: &Store<'_>) -> Settled {
    let change = store_change(&run, store.loaded, (store.clock)());
    let strikes = match change {
        Some(change) => {
            let _writing = store.lock.lock().await;
            let saved =
                save_changes(store.path, &[(run.source, change.clone())], (store.clock)()).await;
            match saved {
                Some(limits) => limits.strikes(run.source),
                None => strikes_after(store.loaded, run.source, &change),
            }
        }
        None => store.loaded.strikes(run.source),
    };
    let report = source_report(&run, None);
    log_source(&report, strikes, elapsed_ms(began));
    Settled { run, report }
}

/// The source's strikes once `change` is applied to `loaded`, for a change that was not saved.
fn strikes_after(loaded: &RateLimits, source: SourceName, change: &Change) -> u32 {
    let mut limits = loaded.clone();
    limits.apply(source, change);
    limits.strikes(source)
}

/// What the run means for the saved blocks: a block when it hit one; an answer that clears the
/// entry this run loaded when it answered any package; nothing otherwise, and nothing when an
/// answer has no loaded entry to clear.
fn store_change(run: &SourceRun, loaded: &RateLimits, now: DateTime<Utc>) -> Option<Change> {
    if let Some(until) = run.block {
        return Some(Change::Blocked { since: now, until });
    }
    let answered_any = run.answers.values().any(|answer| match answer.word() {
        AnswerWord::Answered | AnswerWord::NotFound => true,
        AnswerWord::Failed | AnswerWord::RateLimited | AnswerWord::NotAsked => false,
    });
    let entry = loaded.entry(run.source)?;
    answered_any.then_some(Change::Answered {
        loaded: Some(entry),
    })
}

/// The source's report row: its status, requests, grouped failures, and missing details.
pub(crate) fn source_report(run: &SourceRun, saved_block: Option<DateTime<Utc>>) -> SourceReport {
    let failures = match saved_block {
        Some(_) => Vec::new(),
        None => failure_groups(run),
    };
    SourceReport {
        source: run.source,
        status: source_status(run, saved_block),
        requests: run.requests,
        failures,
        details_missing: run.details_missing,
    }
}

/// A source's status from its run. `saved_block` is the block that kept it from being asked.
pub(crate) fn source_status(run: &SourceRun, saved_block: Option<DateTime<Utc>>) -> SourceStatus {
    if let Some(until) = saved_block {
        return SourceStatus::RateLimited {
            until,
            origin: BlockOrigin::Saved,
        };
    }
    let failures: Vec<&SourceFailure> = run
        .answers
        .values()
        .filter_map(PackageAnswer::failure)
        .collect();
    let answered = run.answers.len() - failures.len();
    if answered > 0 {
        return if failures.is_empty() {
            SourceStatus::Answered {
                blocked_until: run.block,
            }
        } else {
            SourceStatus::Partial {
                blocked_until: run.block,
            }
        };
    }
    if let Some(until) = run.block {
        return SourceStatus::RateLimited {
            until,
            origin: BlockOrigin::ThisRun,
        };
    }
    match most_frequent(&failures) {
        Some(failure) => SourceStatus::Failed {
            failure: failure.clone(),
        },
        // No failure and nothing answered: the run got no package.
        None => SourceStatus::NotAsked,
    }
}

/// The failure that appears most often; on a tie, the one that appears first.
fn most_frequent<'a>(failures: &[&'a SourceFailure]) -> Option<&'a SourceFailure> {
    let mut best: Option<(&SourceFailure, usize)> = None;
    for failure in failures {
        let count = failures.iter().filter(|other| *other == failure).count();
        if best.is_none_or(|(_, best_count)| count > best_count) {
            best = Some((failure, count));
        }
    }
    best.map(|(failure, _)| failure)
}

/// The run's failed packages grouped by reason, in first-seen order.
fn failure_groups(run: &SourceRun) -> Vec<FailureGroup> {
    let mut groups: Vec<FailureGroup> = Vec::new();
    for failure in run.answers.values().filter_map(PackageAnswer::failure) {
        let reason = failure.describe();
        match groups.iter_mut().find(|group| group.reason == reason) {
            Some(group) => group.packages = group.packages.saturating_add(1),
            None => groups.push(FailureGroup {
                reason,
                packages: 1,
            }),
        }
    }
    groups
}

/// One line for one source: info when it answered every package it was asked with nothing
/// missing, warn otherwise. Never a package name: the reasons are failure sentences only.
fn log_source(report: &SourceReport, strikes: u32, elapsed_ms: u64) {
    let source = report.source.name();
    let status = report.status.name();
    if report.fully_answered() {
        info!(
            source,
            status,
            requests = report.requests,
            details_missing = report.details_missing,
            elapsed_ms,
            "source finished"
        );
        return;
    }
    let (until, origin) = match &report.status {
        SourceStatus::RateLimited { until, origin } => (Some(*until), origin.name()),
        SourceStatus::Answered { blocked_until } | SourceStatus::Partial { blocked_until } => {
            (*blocked_until, "")
        }
        SourceStatus::Failed { .. } | SourceStatus::NotAsked => (None, ""),
    };
    let failed: u32 = report.failures.iter().map(|group| group.packages).sum();
    warn!(
        source,
        status,
        reason = %report.failure_reasons(),
        until = %until.map(|until| until.to_rfc3339()).unwrap_or_default(),
        origin,
        strikes,
        requests = report.requests,
        failed,
        details_missing = report.details_missing,
        elapsed_ms,
        "source finished"
    );
}

/// One package's row, merged from every settled source's answer for it. `version` is the one
/// the sources were asked about, if any.
fn package_report(
    key: PackageKey,
    spec: &PackageSpec,
    version: Option<&(String, VersionFrom)>,
    settled: &[Settled],
    supply_chain: Option<&SupplyChain>,
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
        let (status, reason) = without_version(lookup);
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

/// The status of a package written without a version that got none, from deps.dev's answer to
/// the lookup: `NotFound` when deps.dev does not know it, else `NotChecked` with the reason.
fn without_version(lookup: Option<&PackageAnswer>) -> (PackageStatus, Option<String>) {
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

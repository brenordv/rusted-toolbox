//! What happens to each source's run: its rate-limit change saved, its report row built, and
//! its one log line written.

use super::{Clock, PackageAnswer, PackageKey, SourceRun};
use crate::advisory::SourceName;
use crate::http::{SourceFailure, elapsed_ms};
use crate::rate_limits::{Change, RateLimits, save_changes};
use crate::report::{AnswerWord, BlockOrigin, FailureGroup, SourceReport, SourceStatus};
use chrono::{DateTime, Utc};
use std::path::Path;
use std::time::Instant;
use tracing::{info, warn};

/// The rate-limit file and what this check loaded from it.
pub(super) struct Store<'a> {
    pub path: Option<&'a Path>,
    pub loaded: &'a RateLimits,
    pub clock: Clock,
    /// Held while one source's change is saved, so two sources finishing together do not
    /// write the file at once.
    pub lock: tokio::sync::Mutex<()>,
}

/// One source's finished part of a check.
pub(super) struct Settled {
    pub run: SourceRun,
    pub report: SourceReport,
}

/// The settled part of a source that is not asked: one no package needs, or one a saved block
/// keeps out (every package in `keys` fails with the block). Logs its line; `None` when the
/// source is to be asked.
pub(super) fn unasked(
    source: SourceName,
    keys: &[PackageKey],
    loaded: &RateLimits,
    now: DateTime<Utc>,
) -> Option<Settled> {
    if keys.is_empty() {
        return Some(not_asked(source, loaded));
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

/// The settled part of a source no package needs. Logs its line.
pub(super) fn not_asked(source: SourceName, loaded: &RateLimits) -> Settled {
    let run = SourceRun::empty(source);
    let report = source_report(&run, None);
    log_source(&report, loaded.strikes(source), 0);
    Settled { run, report }
}

/// Records, saves, and logs one finished source run. The save happens as soon as the source
/// finishes, so its block is on disk without waiting for the slowest source.
pub(super) async fn settle(run: SourceRun, began: Instant, store: &Store<'_>) -> Settled {
    let changes: Vec<(SourceName, Change)> = store_changes(&run, store.loaded, (store.clock)())
        .into_iter()
        .map(|change| (run.source, change))
        .collect();
    let strikes = if changes.is_empty() {
        store.loaded.strikes(run.source)
    } else {
        let _writing = store.lock.lock().await;
        match save_changes(store.path, &changes, (store.clock)()).await {
            Some(limits) => limits.strikes(run.source),
            None => strikes_after(store.loaded, run.source, &changes),
        }
    };
    let report = source_report(&run, None);
    log_source(&report, strikes, elapsed_ms(began));
    Settled { run, report }
}

/// The source's strikes once `changes` are applied to `loaded`, for changes that were not saved.
fn strikes_after(loaded: &RateLimits, source: SourceName, changes: &[(SourceName, Change)]) -> u32 {
    let mut limits = loaded.clone();
    for (changed, change) in changes {
        limits.apply(*changed, change);
    }
    limits.strikes(source)
}

/// What the run means for the saved blocks, in order: an answer that clears the entry this run
/// loaded when it answered any package, then a block when it hit one. A run that answered and
/// then blocked thus starts its strikes again at one.
fn store_changes(run: &SourceRun, loaded: &RateLimits, now: DateTime<Utc>) -> Vec<Change> {
    let answered_any = run.answers.values().any(|answer| match answer.word() {
        AnswerWord::Answered | AnswerWord::NotFound => true,
        AnswerWord::Failed | AnswerWord::RateLimited | AnswerWord::NotAsked => false,
    });
    let cleared = loaded
        .entry(run.source)
        .filter(|_| answered_any)
        .map(|entry| Change::Answered {
            loaded: Some(entry),
        });
    let blocked = run.block.map(|until| Change::Blocked { since: now, until });
    cleared.into_iter().chain(blocked).collect()
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
        data_as_of: run.data_as_of,
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
/// missing, warn otherwise; offline data's line also has `data_as_of`. Never a package name:
/// the reasons are failure sentences only.
fn log_source(report: &SourceReport, strikes: u32, elapsed_ms: u64) {
    let source = report.source.name();
    let status = report.status.name();
    let data_as_of = report.data_as_of.map(|as_of| as_of.to_rfc3339());
    let data_as_of = data_as_of.as_deref();
    if report.fully_answered() {
        info!(
            source,
            status,
            requests = report.requests,
            details_missing = report.details_missing,
            data_as_of,
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
        data_as_of,
        elapsed_ms,
        "source finished"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeDelta;
    use rstest::rstest;
    use std::collections::BTreeMap;

    fn now() -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000, 0).unwrap()
    }

    /// GitHub's entry after `strikes` blocks that ended ten minutes before [`now`].
    fn lingering(strikes: u32) -> RateLimits {
        let mut limits = RateLimits::default();
        for _ in 0..strikes {
            limits.apply(
                SourceName::GitHub,
                &Change::Blocked {
                    since: now() - TimeDelta::minutes(50),
                    until: now() - TimeDelta::minutes(10),
                },
            );
        }
        limits
    }

    fn github_run(answered: bool, block: Option<DateTime<Utc>>) -> SourceRun {
        let answer = if answered {
            PackageAnswer::Answered {
                advisories: Vec::new(),
            }
        } else {
            PackageAnswer::Failed {
                failure: SourceFailure::TimedOut,
                advisories: Vec::new(),
            }
        };
        SourceRun {
            source: SourceName::GitHub,
            requests: 1,
            block,
            details_missing: 0,
            data_as_of: None,
            answers: BTreeMap::from([(PackageKey(0), answer)]),
        }
    }

    #[test]
    fn an_answer_then_a_block_not_saved_restarts_the_strikes_at_one() {
        let loaded = lingering(3);
        let changes = [
            (
                SourceName::GitHub,
                Change::Answered {
                    loaded: loaded.entry(SourceName::GitHub),
                },
            ),
            (
                SourceName::GitHub,
                Change::Blocked {
                    since: now(),
                    until: now() + TimeDelta::minutes(1),
                },
            ),
        ];

        assert_eq!(strikes_after(&loaded, SourceName::GitHub, &changes), 1);
    }

    #[rstest]
    #[case::answered_and_blocked(true, true, 3, 2)]
    #[case::answered(true, false, 3, 1)]
    #[case::blocked(false, true, 3, 1)]
    #[case::failed(false, false, 3, 0)]
    #[case::answered_with_no_entry(true, false, 0, 0)]
    #[case::blocked_with_no_entry(false, true, 0, 1)]
    fn the_run_s_changes_follow_what_it_did(
        #[case] answered: bool,
        #[case] blocked: bool,
        #[case] strikes: u32,
        #[case] changes: usize,
    ) {
        let loaded = lingering(strikes);
        let until = now() + TimeDelta::minutes(1);
        let block = blocked.then_some(until);

        let made = store_changes(&github_run(answered, block), &loaded, now());

        assert_eq!(made.len(), changes, "{made:?}");
        if blocked {
            assert_eq!(
                made.last(),
                Some(&Change::Blocked {
                    since: now(),
                    until
                })
            );
        }
        if answered && strikes > 0 {
            assert_eq!(
                made.first(),
                Some(&Change::Answered {
                    loaded: loaded.entry(SourceName::GitHub)
                })
            );
        }
    }
}

//! Which downloaded ecosystems a daily update refreshes. Only data that reads Ready is ever
//! due: an automatic update never rebuilds an unreadable store, so it never deletes data.

use crate::ecosystem::Ecosystem;
use crate::offline::store::DataStatus;
use chrono::{DateTime, TimeDelta, Utc};
use tracing::debug;

/// Data confirmed current this long ago is due.
pub const DUE_AFTER: TimeDelta = TimeDelta::hours(24);
/// A failed attempt holds the next automatic one off this long.
pub const RETRY_AFTER_FAILURE: TimeDelta = TimeDelta::hours(1);

/// Whether an ecosystem's data should be updated now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Due {
    Due,
    Skipped(Skip),
}

/// Why an ecosystem's data is not updated now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    /// Confirmed current less than a day ago.
    NotDue,
    /// An attempt failed less than an hour ago.
    FailedRecently,
    NotDownloaded,
    Unreadable,
    NewerFormat,
    Unavailable,
    Locked,
}

impl Skip {
    /// The reason's word, as the log line writes it.
    pub fn name(self) -> &'static str {
        match self {
            Skip::NotDue => "not_due",
            Skip::FailedRecently => "failed_recently",
            Skip::NotDownloaded => "not_downloaded",
            Skip::Unreadable => "unreadable",
            Skip::NewerFormat => "newer_format",
            Skip::Unavailable => "unavailable",
            Skip::Locked => "locked",
        }
    }
}

/// Whether `ecosystem`, whose data reads `status`, is due at `now`: Ready data confirmed current
/// a day ago or more, or at a time later than `now` (a clock that was ahead), with no failed
/// attempt under an hour old. A failed attempt later than `now` holds nothing off. Logs one
/// debug line with the answer.
pub fn due(ecosystem: Ecosystem, status: &DataStatus, now: DateTime<Utc>) -> Due {
    let answer = evaluate(status, now);
    match answer {
        Due::Due => debug!(
            ecosystem = ecosystem.name(),
            verdict = "due",
            "offline update due"
        ),
        Due::Skipped(skip) => debug!(
            ecosystem = ecosystem.name(),
            verdict = "skipped",
            reason = skip.name(),
            "offline update due"
        ),
    }
    answer
}

/// The answer [`due`] logs, without the log line.
pub(crate) fn evaluate(status: &DataStatus, now: DateTime<Utc>) -> Due {
    let meta = match status {
        DataStatus::Ready { meta, .. } => meta,
        DataStatus::NotDownloaded => return Due::Skipped(Skip::NotDownloaded),
        DataStatus::Unavailable { .. } => return Due::Skipped(Skip::Unavailable),
        DataStatus::Locked => return Due::Skipped(Skip::Locked),
        DataStatus::Unreadable { .. } => return Due::Skipped(Skip::Unreadable),
        DataStatus::NewerFormat => return Due::Skipped(Skip::NewerFormat),
    };
    let old = meta.current_at > now || now - meta.current_at >= DUE_AFTER;
    if !old {
        return Due::Skipped(Skip::NotDue);
    }
    let failed_recently = meta
        .last_failure
        .as_ref()
        .is_some_and(|attempt| attempt.at <= now && now - attempt.at < RETRY_AFTER_FAILURE);
    if failed_recently {
        Due::Skipped(Skip::FailedRecently)
    } else {
        Due::Due
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::offline::store::{FailedAttempt, StoreMeta};
    use rstest::rstest;

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    fn ready(current_ago: TimeDelta, failed_ago: Option<TimeDelta>) -> DataStatus {
        let at = now();
        DataStatus::Ready {
            meta: StoreMeta {
                etag: None,
                archive_bytes: 1,
                records: 1,
                downloaded_at: at - current_ago,
                current_at: at - current_ago,
                last_failure: failed_ago.map(|ago| FailedAttempt {
                    at: at - ago,
                    failure: "storage.googleapis.com answered 503".to_owned(),
                }),
            },
            file_bytes: 1,
        }
    }

    #[rstest]
    #[case::a_day_old(TimeDelta::hours(24), None, Due::Due)]
    #[case::fresh(TimeDelta::hours(23), None, Due::Skipped(Skip::NotDue))]
    #[case::from_the_future(TimeDelta::hours(-1), None, Due::Due)]
    #[case::failed_recently(
        TimeDelta::hours(30),
        Some(TimeDelta::minutes(30)),
        Due::Skipped(Skip::FailedRecently)
    )]
    #[case::failed_long_ago(TimeDelta::hours(30), Some(TimeDelta::hours(2)), Due::Due)]
    #[case::failure_from_the_future(TimeDelta::hours(30), Some(TimeDelta::hours(-1)), Due::Due)]
    fn ready_data_is_due_by_its_age_and_its_last_failure(
        #[case] current_ago: TimeDelta,
        #[case] failed_ago: Option<TimeDelta>,
        #[case] expected: Due,
    ) {
        let status = ready(current_ago, failed_ago);

        assert_eq!(evaluate(&status, now()), expected);
    }

    #[rstest]
    #[case::not_downloaded(DataStatus::NotDownloaded, Skip::NotDownloaded)]
    #[case::unavailable(DataStatus::Unavailable { reason: "busy".to_owned() }, Skip::Unavailable)]
    #[case::locked(DataStatus::Locked, Skip::Locked)]
    #[case::unreadable(DataStatus::Unreadable { reason: "corrupt".to_owned() }, Skip::Unreadable)]
    #[case::newer_format(DataStatus::NewerFormat, Skip::NewerFormat)]
    fn only_ready_data_is_ever_due(#[case] status: DataStatus, #[case] skip: Skip) {
        assert_eq!(evaluate(&status, now()), Due::Skipped(skip));
    }
}

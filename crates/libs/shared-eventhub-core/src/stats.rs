//! Live capture statistics, kept UI-free: the engine records, consumers read
//! [`StatsSnapshot`]s.

use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

/// A point-in-time copy of the capture counters and rates.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct StatsSnapshot {
    /// Events read from the service, including duplicates and unkeyable ones.
    pub received: u64,
    /// Events the sink stored for the first time.
    pub stored: u64,
    /// Events already processed this run (a re-delivery after a reconnect) or
    /// already in the sink.
    pub duplicated: u64,
    /// Stored events the capture filters did not pass (including events with
    /// no enqueue time under a time filter).
    pub filtered_out: u64,
    /// Events without a sequence number, which cannot be keyed or stored.
    pub unkeyable: u64,
    /// Message and skip events dropped because the consumer lagged; capture
    /// itself never drops.
    pub ui_dropped: u64,
    pub per_partition: BTreeMap<String, PartitionStats>,
    /// Events received per second since the previous snapshot.
    pub current_rate: f64,
    /// Events received per second since the run started.
    pub average_rate: f64,
    /// Highest `current_rate` seen this run.
    pub peak_rate: f64,
    pub last_message_at: Option<DateTime<Utc>>,
    pub started_at: DateTime<Utc>,
    pub elapsed: Duration,
}

/// Per-partition progress: the local receive time shows whether the
/// partition is still delivering, and the broker enqueue time shows how far
/// behind the head it is.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct PartitionStats {
    pub received: u64,
    /// Local clock.
    pub last_received_at: Option<DateTime<Utc>>,
    /// Broker clock.
    pub last_enqueued_at: Option<DateTime<Utc>>,
}

/// Thread-safe accumulator behind [`StatsSnapshot`].
#[derive(Debug)]
pub struct StatsTracker {
    started_at: DateTime<Utc>,
    started: Instant,
    state: Mutex<TrackerState>,
}

#[derive(Debug, Default)]
struct TrackerState {
    received: u64,
    stored: u64,
    duplicated: u64,
    filtered_out: u64,
    unkeyable: u64,
    ui_dropped: u64,
    per_partition: BTreeMap<String, PartitionStats>,
    last_message_at: Option<DateTime<Utc>>,
    window: RateWindow,
}

/// The received count and time of the previous snapshot, from which the
/// current rate is computed.
#[derive(Debug, Default)]
struct RateWindow {
    since: Option<Instant>,
    received_at_start: u64,
    peak_rate: f64,
}

impl StatsTracker {
    /// Starts a tracker whose run began at `started_at` (wall clock) and now.
    pub fn new(started_at: DateTime<Utc>) -> Self {
        Self {
            started_at,
            started: Instant::now(),
            state: Mutex::new(TrackerState::default()),
        }
    }

    /// Records one event read from `partition_id`, received locally at
    /// `received_at` and enqueued by the broker at `enqueued_at`.
    pub fn record_received(
        &self,
        partition_id: &str,
        received_at: DateTime<Utc>,
        enqueued_at: Option<DateTime<Utc>>,
    ) {
        let mut state = self.lock();
        state.received += 1;
        state.last_message_at = Some(received_at);

        let partition = state
            .per_partition
            .entry(partition_id.to_owned())
            .or_default();
        partition.received += 1;
        partition.last_received_at = Some(received_at);
        if enqueued_at.is_some() {
            partition.last_enqueued_at = enqueued_at;
        }
    }

    pub fn record_stored(&self) {
        self.lock().stored += 1;
    }

    pub fn record_duplicate(&self) {
        self.lock().duplicated += 1;
    }

    pub fn record_filtered(&self) {
        self.lock().filtered_out += 1;
    }

    pub fn record_unkeyable(&self) {
        self.lock().unkeyable += 1;
    }

    pub fn record_ui_dropped(&self) {
        self.lock().ui_dropped += 1;
    }

    /// Returns the current numbers and starts a new rate window, so the next
    /// snapshot's `current_rate` covers only the time since this one.
    pub fn snapshot(&self) -> StatsSnapshot {
        self.snapshot_at(Instant::now())
    }

    fn snapshot_at(&self, now: Instant) -> StatsSnapshot {
        let elapsed = now.saturating_duration_since(self.started);
        let mut state = self.lock();

        let window_start = state.window.since.unwrap_or(self.started);
        let window_received = state.received - state.window.received_at_start;
        let current_rate = per_second(window_received, now.saturating_duration_since(window_start));
        state.window.peak_rate = state.window.peak_rate.max(current_rate);
        state.window.since = Some(now);
        state.window.received_at_start = state.received;

        StatsSnapshot {
            received: state.received,
            stored: state.stored,
            duplicated: state.duplicated,
            filtered_out: state.filtered_out,
            unkeyable: state.unkeyable,
            ui_dropped: state.ui_dropped,
            per_partition: state.per_partition.clone(),
            current_rate,
            average_rate: per_second(state.received, elapsed),
            peak_rate: state.window.peak_rate,
            last_message_at: state.last_message_at,
            started_at: self.started_at,
            elapsed,
        }
    }

    fn lock(&self) -> MutexGuard<'_, TrackerState> {
        // Every critical section is a few counter updates that leave the state
        // consistent, so a panic elsewhere while holding the lock cannot leave
        // a half-written state; recovering the guard keeps statistics flowing.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn per_second(count: u64, over: Duration) -> f64 {
    let seconds = over.as_secs_f64();
    if seconds > 0.0 {
        count as f64 / seconds
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_accumulate_into_the_snapshot() {
        let tracker = StatsTracker::new(Utc::now());
        let received_at = Utc::now();
        let enqueued_at = received_at - chrono::Duration::seconds(5);

        tracker.record_received("0", received_at, Some(enqueued_at));
        tracker.record_received("1", received_at, None);
        tracker.record_stored();
        tracker.record_duplicate();
        tracker.record_filtered();
        tracker.record_unkeyable();
        tracker.record_ui_dropped();
        let snapshot = tracker.snapshot();

        assert_eq!(snapshot.received, 2);
        assert_eq!(snapshot.stored, 1);
        assert_eq!(snapshot.duplicated, 1);
        assert_eq!(snapshot.filtered_out, 1);
        assert_eq!(snapshot.unkeyable, 1);
        assert_eq!(snapshot.ui_dropped, 1);
        assert_eq!(snapshot.last_message_at, Some(received_at));
        assert_eq!(
            snapshot.per_partition["0"],
            PartitionStats {
                received: 1,
                last_received_at: Some(received_at),
                last_enqueued_at: Some(enqueued_at),
            }
        );
        assert_eq!(snapshot.per_partition["1"].last_enqueued_at, None);
    }

    #[test]
    fn current_rate_covers_only_the_window_since_the_previous_snapshot() {
        let tracker = StatsTracker::new(Utc::now());
        let start = tracker.started;
        for _ in 0..10 {
            tracker.record_received("0", Utc::now(), None);
        }

        let first = tracker.snapshot_at(start + Duration::from_secs(2));
        let second = tracker.snapshot_at(start + Duration::from_secs(4));

        assert_eq!(first.current_rate, 5.0);
        assert_eq!(second.current_rate, 0.0);
        assert_eq!(second.peak_rate, 5.0);
        assert_eq!(second.average_rate, 2.5);
    }

    #[test]
    fn a_snapshot_at_the_start_instant_reports_zero_rates() {
        let tracker = StatsTracker::new(Utc::now());
        tracker.record_received("0", Utc::now(), None);

        let snapshot = tracker.snapshot_at(tracker.started);

        assert_eq!(snapshot.current_rate, 0.0);
        assert_eq!(snapshot.average_rate, 0.0);
    }

    #[test]
    fn a_poisoned_lock_keeps_recording() {
        let tracker = std::sync::Arc::new(StatsTracker::new(Utc::now()));
        let poisoner = std::sync::Arc::clone(&tracker);
        let _ = std::thread::spawn(move || {
            let _guard = poisoner.state.lock().unwrap();
            panic!("poison the stats lock");
        })
        .join();

        tracker.record_stored();

        assert_eq!(tracker.snapshot().stored, 1);
    }
}

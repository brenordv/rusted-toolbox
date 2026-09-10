use std::collections::HashMap;
use std::time::Duration;

use crate::files::FileResolver;
use camino::Utf8Path;

pub fn display_relative_path(resolver: &FileResolver, path: &Utf8Path) -> String {
    path.strip_prefix(resolver.requests_root())
        .map(|relative| relative.to_string())
        .unwrap_or_else(|_| path.to_string())
}

/// Formats a duration for the elapsed-time display: integer milliseconds under
/// one second, seconds with one decimal at or above it.
pub fn format_duration(duration: Duration) -> String {
    if duration < Duration::from_secs(1) {
        format!("{} ms", duration.as_millis())
    } else {
        format!("{:.1} s", duration.as_secs_f64())
    }
}

/// In-run elapsed-time history. Tracks the running total across entries and
/// the previous elapsed value per request key, so a repeat hit of the same
/// request within one run shows a delta against the previous attempt. Nothing
/// persists across runs.
#[derive(Default)]
pub struct ElapsedTracker {
    history: HashMap<String, Duration>,
    total: Duration,
}

pub struct ElapsedObservation {
    pub elapsed: Duration,
    pub total: Duration,
    pub delta: Option<ElapsedDelta>,
}

pub struct ElapsedDelta {
    pub slower: bool,
    pub amount: Duration,
}

impl ElapsedTracker {
    /// Records one entry's elapsed time. The total always accumulates; the
    /// delta only exists when the key was seen before in this run. Entries
    /// without a key (no HTTP calls) still count toward the total. The stored
    /// history updates to the latest value after each comparison.
    pub fn observe(&mut self, key: Option<String>, elapsed: Duration) -> ElapsedObservation {
        self.total += elapsed;

        let delta = key.and_then(|key| {
            let previous = self.history.insert(key, elapsed)?;
            Some(match elapsed.checked_sub(previous) {
                Some(amount) => ElapsedDelta {
                    slower: true,
                    amount,
                },
                None => ElapsedDelta {
                    slower: false,
                    amount: previous - elapsed,
                },
            })
        });

        ElapsedObservation {
            elapsed,
            total: self.total,
            delta,
        }
    }
}

/// Renders `[Elapsed: 200 ms | Total: 1.2 s]`, with a signed delta after the
/// elapsed value on a repeat hit: `[Elapsed: 200 ms (+50 ms) | Total: 1.2 s]`.
pub fn format_elapsed_line(observation: &ElapsedObservation) -> String {
    match &observation.delta {
        Some(delta) => format!(
            "[Elapsed: {} ({}{}) | Total: {}]",
            format_duration(observation.elapsed),
            if delta.slower { "+" } else { "-" },
            format_duration(delta.amount),
            format_duration(observation.total),
        ),
        None => format!(
            "[Elapsed: {} | Total: {}]",
            format_duration(observation.elapsed),
            format_duration(observation.total),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_duration_uses_ms_under_one_second_and_seconds_above() {
        assert_eq!(format_duration(Duration::from_millis(0)), "0 ms");
        assert_eq!(format_duration(Duration::from_millis(999)), "999 ms");
        assert_eq!(format_duration(Duration::from_millis(1000)), "1.0 s");
        assert_eq!(format_duration(Duration::from_millis(1240)), "1.2 s");
        assert_eq!(format_duration(Duration::from_secs(61)), "61.0 s");
    }

    #[test]
    fn first_observation_has_no_delta_and_starts_the_total() {
        let mut tracker = ElapsedTracker::default();
        let obs = tracker.observe(
            Some("GET https://a|dev".to_string()),
            Duration::from_millis(200),
        );

        assert!(obs.delta.is_none());
        assert_eq!(obs.elapsed, Duration::from_millis(200));
        assert_eq!(obs.total, Duration::from_millis(200));
    }

    #[test]
    fn repeat_hit_reports_signed_delta_and_updates_history() {
        let mut tracker = ElapsedTracker::default();
        let key = || Some("GET https://a|dev".to_string());

        tracker.observe(key(), Duration::from_millis(200));

        let slower = tracker.observe(key(), Duration::from_millis(250));
        let delta = slower.delta.expect("delta on repeat");
        assert!(delta.slower);
        assert_eq!(delta.amount, Duration::from_millis(50));
        assert_eq!(slower.total, Duration::from_millis(450));

        // The stored value moved to 250 ms, so 240 ms is now faster.
        let faster = tracker.observe(key(), Duration::from_millis(240));
        let delta = faster.delta.expect("delta on repeat");
        assert!(!delta.slower);
        assert_eq!(delta.amount, Duration::from_millis(10));
    }

    #[test]
    fn distinct_keys_and_keyless_entries_do_not_produce_deltas() {
        let mut tracker = ElapsedTracker::default();

        tracker.observe(
            Some("GET https://a|dev".to_string()),
            Duration::from_millis(100),
        );
        let other = tracker.observe(
            Some("GET https://b|dev".to_string()),
            Duration::from_millis(100),
        );
        assert!(other.delta.is_none());

        let keyless = tracker.observe(None, Duration::from_millis(300));
        assert!(keyless.delta.is_none());
        assert_eq!(keyless.total, Duration::from_millis(500));
    }

    #[test]
    fn elapsed_line_formats_with_and_without_delta() {
        let mut tracker = ElapsedTracker::default();
        let key = || Some("GET https://a|-".to_string());

        let first = tracker.observe(key(), Duration::from_millis(200));
        assert_eq!(
            format_elapsed_line(&first),
            "[Elapsed: 200 ms | Total: 200 ms]"
        );

        let second = tracker.observe(key(), Duration::from_millis(250));
        assert_eq!(
            format_elapsed_line(&second),
            "[Elapsed: 250 ms (+50 ms) | Total: 450 ms]"
        );

        let third = tracker.observe(key(), Duration::from_millis(850));
        assert_eq!(
            format_elapsed_line(&third),
            "[Elapsed: 850 ms (+600 ms) | Total: 1.3 s]"
        );
    }
}

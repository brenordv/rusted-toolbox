use crate::models::{
    ConnectivityResult, NetQualityConfig, OutageInfo, SpeedResult, ThresholdCategory,
};
use crate::notifiers::Notifier;
use chrono::{DateTime, Utc};
use std::time::{Duration, Instant};

pub(crate) struct LoopState {
    pub(crate) next_connectivity_at: Instant,
    pub(crate) next_speed_at: Instant,
    pub(crate) current_connectivity_delay: Duration,
    pub(crate) next_url_index: usize,
    pub(crate) outage_active: bool,
    pub(crate) outage_start: Option<DateTime<Utc>>,
    pub(crate) pending_outage_end: Option<OutageInfo>,
    pub(crate) pending_speed_after_restore: bool,
    pub(crate) last_connectivity_success: bool,
    pub(crate) last_download_threshold: Option<ThresholdCategory>,
    pub(crate) last_upload_threshold: Option<ThresholdCategory>,
}

impl LoopState {
    pub(crate) fn new(config: &NetQualityConfig) -> Self {
        let now = Instant::now();
        Self {
            next_connectivity_at: now,
            next_speed_at: now,
            current_connectivity_delay: config.connectivity.delay,
            next_url_index: 0,
            outage_active: false,
            outage_start: None,
            pending_outage_end: None,
            pending_speed_after_restore: false,
            last_connectivity_success: true,
            last_download_threshold: None,
            last_upload_threshold: None,
        }
    }

    pub(crate) fn current_url_index(&self) -> usize {
        self.next_url_index
    }

    pub(crate) fn update_next_url_index(&mut self, next_index: usize) {
        self.next_url_index = next_index;
    }
}

pub(crate) fn handle_connectivity_state(
    config: &NetQualityConfig,
    state: &mut LoopState,
    result: &ConnectivityResult,
) {
    state.last_connectivity_success = result.success;
    state.next_connectivity_at = Instant::now() + state.current_connectivity_delay;

    if result.success {
        if state.outage_active {
            state.outage_active = false;
            state.pending_outage_end = Some(OutageInfo {
                started_at: state.outage_start.unwrap_or(result.timestamp),
                ended_at: result.timestamp,
            });
            state.outage_start = None;
            state.pending_speed_after_restore = true;
        }

        state.current_connectivity_delay = config.connectivity.delay;
        return;
    }

    if !state.outage_active {
        state.outage_active = true;
        state.outage_start = Some(result.timestamp);
        state.current_connectivity_delay = config.connectivity.outage_backoff;
    } else {
        let next_delay = state.current_connectivity_delay + config.connectivity.outage_backoff;
        state.current_connectivity_delay =
            std::cmp::min(next_delay, config.connectivity.outage_backoff_max);
    }

    state.next_connectivity_at = Instant::now() + state.current_connectivity_delay;
}

pub(crate) fn should_run_speed_check(state: &LoopState) -> bool {
    if state.pending_speed_after_restore {
        return true;
    }

    Instant::now() >= state.next_speed_at
}

pub(crate) async fn handle_speed_state(
    config: &NetQualityConfig,
    state: &mut LoopState,
    notifier: &mut Notifier,
    result: &SpeedResult,
) {
    if state.pending_speed_after_restore {
        if let Some(outage) = state.pending_outage_end.take() {
            notifier.send_outage_end(config, result, &outage).await;
        }
        state.pending_speed_after_restore = false;
    }

    let download_changed = state
        .last_download_threshold
        .map(|last| last != result.download_threshold)
        .unwrap_or(true);

    let upload_changed = match (state.last_upload_threshold, result.upload_threshold) {
        (Some(last), Some(current)) => last != current,
        (None, Some(_)) => true,
        (Some(_), None) => true,
        (None, None) => false,
    };

    let download_notify = result
        .download_threshold
        .is_at_or_below(config.notifications.min_download_threshold);
    let upload_notify = result
        .upload_threshold
        .map(|threshold| threshold.is_at_or_below(config.notifications.min_upload_threshold))
        .unwrap_or(false);

    if (download_changed || upload_changed) && (download_notify || upload_notify) {
        notifier.send_speed_change(config, result).await;
    }

    state.last_download_threshold = Some(result.download_threshold);
    state.last_upload_threshold = result.upload_threshold;
    state.next_speed_at = Instant::now() + config.speed.delay;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        ConnectivityConfig, NotificationConfig, SpeedConfig, StorageConfig, Thresholds,
    };
    use std::path::PathBuf;

    fn test_config(delay_secs: u64, backoff_secs: u64, backoff_max_secs: u64) -> NetQualityConfig {
        NetQualityConfig {
            connectivity: ConnectivityConfig {
                delay: Duration::from_secs(delay_secs),
                timeout: Duration::from_secs(10),
                outage_backoff: Duration::from_secs(backoff_secs),
                outage_backoff_max: Duration::from_secs(backoff_max_secs),
                urls: vec!["https://example.com/health".to_string()],
            },
            speed: SpeedConfig {
                expected_download_mbps: 100.0,
                expected_upload_mbps: None,
                delay: Duration::from_secs(14_400),
                download_thresholds: Thresholds::default_thresholds(),
                upload_thresholds: Thresholds::default_thresholds(),
                speedtest_cli_path: None,
            },
            notifications: NotificationConfig {
                telegram: None,
                min_download_threshold: ThresholdCategory::Medium,
                min_upload_threshold: ThresholdCategory::Slow,
            },
            storage: StorageConfig {
                db_path: PathBuf::from("netquality-test.db"),
                cleanup_enabled: false,
                cleanup_interval: Duration::from_secs(86_400),
            },
            otel_endpoint: None,
        }
    }

    fn connectivity_result(success: bool) -> ConnectivityResult {
        ConnectivityResult {
            timestamp: Utc::now(),
            url: "https://example.com/health".to_string(),
            result: if success { "204" } else { "timeout" }.to_string(),
            elapsed_ms: 5,
            success,
        }
    }

    #[test]
    fn failure_from_healthy_enters_outage_with_initial_backoff() {
        let config = test_config(60, 10, 3_600);
        let mut state = LoopState::new(&config);
        let failure = connectivity_result(false);

        handle_connectivity_state(&config, &mut state, &failure);

        assert!(state.outage_active);
        assert_eq!(state.outage_start, Some(failure.timestamp));
        assert!(!state.last_connectivity_success);
        assert_eq!(state.current_connectivity_delay, Duration::from_secs(10));
        assert!(state.pending_outage_end.is_none());
        assert!(!state.pending_speed_after_restore);
    }

    #[test]
    fn repeated_failures_grow_backoff_capped_at_max() {
        let config = test_config(60, 10, 25);
        let mut state = LoopState::new(&config);

        handle_connectivity_state(&config, &mut state, &connectivity_result(false));
        assert_eq!(state.current_connectivity_delay, Duration::from_secs(10));

        handle_connectivity_state(&config, &mut state, &connectivity_result(false));
        assert_eq!(state.current_connectivity_delay, Duration::from_secs(20));

        handle_connectivity_state(&config, &mut state, &connectivity_result(false));
        assert_eq!(state.current_connectivity_delay, Duration::from_secs(25));

        handle_connectivity_state(&config, &mut state, &connectivity_result(false));
        assert_eq!(state.current_connectivity_delay, Duration::from_secs(25));
    }

    #[test]
    fn recovery_queues_speed_check_and_resets_delay() {
        let config = test_config(60, 10, 3_600);
        let mut state = LoopState::new(&config);
        let failure = connectivity_result(false);
        handle_connectivity_state(&config, &mut state, &failure);

        let success = connectivity_result(true);
        handle_connectivity_state(&config, &mut state, &success);

        assert!(!state.outage_active);
        assert!(state.outage_start.is_none());
        assert!(state.last_connectivity_success);
        assert!(state.pending_speed_after_restore);
        assert_eq!(state.current_connectivity_delay, Duration::from_secs(60));

        let outage = state
            .pending_outage_end
            .expect("recovery should record the outage window");
        assert_eq!(outage.started_at, failure.timestamp);
        assert_eq!(outage.ended_at, success.timestamp);
    }

    #[test]
    fn success_while_healthy_keeps_state_clean() {
        let config = test_config(60, 10, 3_600);
        let mut state = LoopState::new(&config);

        handle_connectivity_state(&config, &mut state, &connectivity_result(true));

        assert!(!state.outage_active);
        assert!(state.pending_outage_end.is_none());
        assert!(!state.pending_speed_after_restore);
        assert_eq!(state.current_connectivity_delay, Duration::from_secs(60));
    }
}

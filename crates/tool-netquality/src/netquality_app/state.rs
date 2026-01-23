use crate::models::{
    ConnectivityResult, NetQualityConfig, OutageInfo, SpeedResult, ThresholdCategory,
};
use crate::notifiers::Notifier;
use chrono::{DateTime, Utc};
use std::time::{Duration, Instant};

pub(super) struct LoopState {
    pub(super) next_connectivity_at: Instant,
    pub(super) next_speed_at: Instant,
    pub(super) current_connectivity_delay: Duration,
    pub(super) next_url_index: usize,
    pub(super) outage_active: bool,
    pub(super) outage_start: Option<DateTime<Utc>>,
    pub(super) pending_outage_end: Option<OutageInfo>,
    pub(super) pending_speed_after_restore: bool,
    pub(super) last_connectivity_success: bool,
    pub(super) last_download_threshold: Option<ThresholdCategory>,
    pub(super) last_upload_threshold: Option<ThresholdCategory>,
}

impl LoopState {
    pub(super) fn new(config: &NetQualityConfig) -> Self {
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
}

pub(super) fn handle_connectivity_state(
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

pub(super) fn should_run_speed_check(state: &LoopState) -> bool {
    if state.pending_speed_after_restore {
        return true;
    }

    Instant::now() >= state.next_speed_at
}

pub(super) async fn handle_speed_state(
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

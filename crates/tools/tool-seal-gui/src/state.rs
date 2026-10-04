use crate::app::Tab;
use crate::worker::{JobEvent, JobOutcome};
use common_gui::widgets::{Toast, ToastKind};
use std::time::Duration;
use tracing::{debug, error};

/// How long a toast stays on screen.
pub const TOAST_TTL: Duration = Duration::from_secs(4);

/// Everything the views render. This struct is the single source of truth:
/// views bind widgets straight to these fields, and egui's own widget memory
/// is never authoritative (eframe persistence is off).
pub struct AppState {
    pub active_tab: Tab,
    pub toasts: Vec<Toast>,
    /// A job is in flight; job-starting actions are disabled while set.
    pub busy: bool,
    /// Per-file progress of the running job, as (done, total).
    pub progress: Option<(usize, usize)>,
    /// The worker thread is gone; jobs cannot run again this session.
    pub worker_gone: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            active_tab: Tab::Text,
            toasts: Vec::new(),
            busy: false,
            progress: None,
            worker_gone: false,
        }
    }
}

/// Applies one worker event to the state: the only place job events mutate
/// the UI model.
pub fn apply_event(state: &mut AppState, event: JobEvent) {
    match event {
        JobEvent::Started { job_id } => {
            debug!(job_id, "background job started");
            state.busy = true;
            state.progress = None;
        }
        JobEvent::FileProgress {
            job_id,
            path,
            done,
            total,
        } => {
            debug!(job_id, path = %path.display(), done, total, "background job progress");
            state.progress = Some((done, total));
        }
        JobEvent::Done { job_id, outcome } => {
            debug!(job_id, "background job finished");
            state.busy = false;
            state.progress = None;
            let text = match outcome {
                JobOutcome::Probe => "The worker replied; the job pipeline works.",
            };
            state
                .toasts
                .push(Toast::new(ToastKind::Ok, text, TOAST_TTL));
        }
        JobEvent::Failed { job_id, error_text } => {
            state.busy = false;
            state.progress = None;
            error!(job_id, error = %error_text, "background job failed");
            state
                .toasts
                .push(Toast::new(ToastKind::Err, error_text, TOAST_TTL));
        }
    }
}

/// Marks the worker as gone after its event channel disconnected (the thread
/// panicked or exited). Idempotent, because the drain reports the disconnect
/// on every following frame while the user should hear about it once.
pub fn on_worker_gone(state: &mut AppState) {
    if state.worker_gone {
        return;
    }
    state.worker_gone = true;
    state.busy = false;
    state.progress = None;
    error!("the background worker stopped; jobs need an app restart");
    state.toasts.push(Toast::new(
        ToastKind::Err,
        "The background worker stopped. Restart the app to run jobs.",
        TOAST_TTL,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn started_sets_busy_and_clears_stale_progress() {
        let mut state = AppState {
            progress: Some((1, 2)),
            ..AppState::default()
        };

        apply_event(&mut state, JobEvent::Started { job_id: 1 });

        assert!(state.busy);
        assert_eq!(state.progress, None);
        assert!(state.toasts.is_empty());
    }

    #[test]
    fn file_progress_updates_the_counters() {
        let mut state = AppState::default();

        apply_event(
            &mut state,
            JobEvent::FileProgress {
                job_id: 1,
                path: PathBuf::new(),
                done: 2,
                total: 5,
            },
        );

        assert_eq!(state.progress, Some((2, 5)));
    }

    #[test]
    fn done_clears_busy_and_pushes_an_ok_toast() {
        let mut state = AppState {
            busy: true,
            progress: Some((1, 1)),
            ..AppState::default()
        };

        apply_event(
            &mut state,
            JobEvent::Done {
                job_id: 1,
                outcome: JobOutcome::Probe,
            },
        );

        assert!(!state.busy);
        assert_eq!(state.progress, None);
        assert_eq!(state.toasts.len(), 1);
        assert!(matches!(state.toasts[0].kind, ToastKind::Ok));
    }

    #[test]
    fn failed_clears_busy_and_carries_the_error_text_into_the_toast() {
        let mut state = AppState {
            busy: true,
            ..AppState::default()
        };

        apply_event(
            &mut state,
            JobEvent::Failed {
                job_id: 1,
                error_text: "it broke".to_string(),
            },
        );

        assert!(!state.busy);
        assert_eq!(state.toasts.len(), 1);
        assert!(matches!(state.toasts[0].kind, ToastKind::Err));
        assert_eq!(state.toasts[0].text, "it broke");
    }

    #[test]
    fn worker_gone_reports_once_and_stays_set() {
        let mut state = AppState {
            busy: true,
            ..AppState::default()
        };

        on_worker_gone(&mut state);
        on_worker_gone(&mut state);

        assert!(state.worker_gone);
        assert!(!state.busy);
        assert_eq!(state.toasts.len(), 1);
        assert!(matches!(state.toasts[0].kind, ToastKind::Err));
    }
}

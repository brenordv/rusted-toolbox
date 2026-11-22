//! How a session ends: whether it failed, why, and when.

use shared_eventhub_core::engine::{RunEnd, RunSummary};
use shared_eventhub_persistence::export::ExportOutcome;
use tracing::info;

/// When a session failed, which decides what the window tells the user.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureStage {
    /// Before capture began: the store, the secrets, signing in, or the connection.
    Starting,
    /// A session that ran: a failed partition or the at-end pass. Captured messages are in the
    /// archive.
    Run,
}

/// Why the connection is in `Failed`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub stage: FailureStage,
    pub text: String,
}

impl Failure {
    pub fn starting(text: impl Into<String>) -> Self {
        Self {
            stage: FailureStage::Starting,
            text: text.into(),
        }
    }
}

/// Every failure of a finished session joined with "; ", or `None` when it ended cleanly. A
/// stopped run with a failed partition fails, as `eh read` does; `fatal`, when the engine sent
/// one, replaces only the engine's entry.
pub(super) fn session_failures(
    engine: &Result<RunSummary, String>,
    finish: &Result<ExportOutcome, String>,
    fatal: Option<String>,
) -> Option<String> {
    let mut failures = Vec::new();
    let engine_failure = match engine {
        Err(text) => Some(text.clone()),
        Ok(summary) => match &summary.end {
            RunEnd::Completed => None,
            RunEnd::Stopped => {
                let failed = summary.failed_partitions();
                (!failed.is_empty())
                    .then(|| format!("Partitions {} failed before the stop", failed.join(", ")))
            }
            RunEnd::Failed(reason) => Some(format!("The run failed: {reason}")),
        },
    };
    if let Some(text) = engine_failure {
        failures.push(fatal.unwrap_or(text));
    }
    match finish {
        Ok(outcome) if outcome.failed > 0 => failures.push(format!(
            "{} message file(s) could not be written; they are in the archive, and the Export \
             section can write them",
            outcome.failed
        )),
        Ok(_) => {}
        Err(text) => failures.push(format!("Failed to write the at-end message files: {text}")),
    }
    (!failures.is_empty()).then(|| failures.join("; "))
}

pub(super) fn log_summary(
    session: u64,
    summary: &RunSummary,
    finish: &Result<ExportOutcome, String>,
) {
    info!(
        session,
        end = ?summary.end,
        received = summary.stats.received,
        stored = summary.stats.stored,
        ui_dropped = summary.stats.ui_dropped,
        failed_partitions = ?summary.failed_partitions(),
        finish = ?finish.as_ref().map(|outcome| outcome.exported),
        "session ended"
    );
}

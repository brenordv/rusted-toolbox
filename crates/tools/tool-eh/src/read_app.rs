//! `eh read`: captures events into the data folder's archive.

use crate::event_loop::{self, LoopEnd};
use crate::models::{ReadJob, RunOutcome};
use crate::progress::{
    ExportProgressLine, Renderer, connecting_notice, display, read_summary_block,
};
use crate::target_setup::{TargetSetup, prepare_target};
use anyhow::anyhow;
use eh_core::engine::{CaptureEngine, RunEnd};
use eh_persistence::export::{DiskWriteMode, ExportOutcome, WritingSink};
use eh_persistence::store::MessageStore;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

/// Runs a capture: resolves the target (saving the config first when asked),
/// opens the archive, runs the engine until it ends or Ctrl-C stops it,
/// writes the at-end message files, closes the archive, and prints the
/// summary.
///
/// # Errors
/// Fails when the target cannot be resolved or saved, the archive cannot be
/// opened, the run cannot start, a partition failed, message files could not
/// be written, or the archive fails to close.
pub async fn run(job: ReadJob, shutdown: &AtomicBool) -> anyhow::Result<RunOutcome> {
    let paths = &job.paths;
    let mut renderer = Renderer::terminal(job.verbose);
    let target = prepare_target(
        &TargetSetup {
            config: &job.config,
            given: &job.secrets,
            paths,
            save_config: job.save_config,
            home: &job.home,
        },
        &mut renderer,
    )?;

    let store = MessageStore::open(paths)?;
    let disk_write = &job.config.storage.disk_write;
    let sink = WritingSink::new(
        store,
        job.config.disk_write_config(paths),
        job.config.capture.filters.clone(),
    );

    renderer.notice(&connecting_notice(
        &job.config.target,
        Some(&job.config.target.consumer_group),
    ));
    let (channels, run) = CaptureEngine::build(job.config.engine_config(target), sink.clone());
    let ended = event_loop::run(channels, run, shutdown, &mut renderer).await;

    let finished = finish_disk_writes(&sink, disk_write.mode).await;
    let closed = sink.close().await;
    if let LoopEnd::Finished(Ok(summary)) = &ended {
        let at_end = match (disk_write.mode, &finished) {
            (DiskWriteMode::AtEnd, Ok(outcome)) => Some(outcome),
            (DiskWriteMode::AtEnd, Err(_)) | (DiskWriteMode::Off | DiskWriteMode::Immediate, _) => {
                None
            }
        };
        renderer.block(&read_summary_block(summary, at_end));
    }

    run_outcome(ended, finished, closed)
}

/// Writes the at-end message files (nothing to do in the other modes), with
/// the export progress line while it runs.
async fn finish_disk_writes(
    sink: &WritingSink,
    mode: DiskWriteMode,
) -> anyhow::Result<ExportOutcome> {
    match mode {
        DiskWriteMode::Off | DiskWriteMode::Immediate => sink.finish(|_| {}).await,
        DiskWriteMode::AtEnd => {
            let line = Arc::new(Mutex::new(ExportProgressLine::stdout()));
            let drawer = Arc::clone(&line);
            let finished = sink
                .finish(move |outcome| {
                    drawer
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .draw(outcome, Instant::now());
                })
                .await;
            // The line only holds drawing state, so a panic while it was
            // locked cannot leave anything half-written that matters.
            line.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .finish(finished.is_ok());
            finished
        }
    }
}

/// Maps how the run ended, the at-end writes, and the archive close to the
/// exit outcome. Any failure wins over an interrupt: a partition that failed
/// during a run stopped with Ctrl-C (the normal end of a live tail) still
/// fails the run.
pub(crate) fn run_outcome(
    ended: LoopEnd,
    finished: anyhow::Result<ExportOutcome>,
    closed: anyhow::Result<()>,
) -> anyhow::Result<RunOutcome> {
    let mut errors = Vec::new();
    let mut interrupted = false;

    match ended {
        LoopEnd::InterruptedBeforeStart => interrupted = true,
        LoopEnd::Finished(Err(error)) => errors.push(error),
        LoopEnd::Finished(Ok(summary)) => {
            let failed = summary.failed_partitions();
            match summary.end {
                RunEnd::Completed => {}
                RunEnd::Stopped => {
                    interrupted = true;
                    if !failed.is_empty() {
                        errors.push(anyhow!(
                            "Partitions {} failed before the stop",
                            display(failed.join(", "))
                        ));
                    }
                }
                RunEnd::Failed(reason) => {
                    errors.push(anyhow!("The run failed: {}", display(reason)));
                }
            }
        }
    }
    match finished {
        Ok(outcome) if outcome.failed > 0 => errors.push(anyhow!(
            "{} message file(s) could not be written; they are in the archive, and eh export \
             can write them",
            outcome.failed
        )),
        Ok(_) => {}
        Err(error) => errors.push(error.context("Failed to write the at-end message files")),
    }
    if let Err(error) = closed {
        errors.push(error.context("Failed to close the archive"));
    }

    match errors.len() {
        0 if interrupted => Ok(RunOutcome::Interrupted),
        0 => Ok(RunOutcome::Completed),
        1 => Err(errors.remove(0)),
        _ => Err(anyhow!(
            "{}",
            errors
                .iter()
                .map(|error| format!("{error:#}"))
                .collect::<Vec<_>>()
                .join("; ")
        )),
    }
}

#[cfg(test)]
mod tests;

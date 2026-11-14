//! `eh read`: captures events into the data folder's archive.

use crate::event_loop::{self, LoopEnd};
use crate::models::{CLIENT_SECRET_ENV, ReadJob, RunOutcome};
use crate::progress::{
    ExportProgressLine, Renderer, connecting_notice, display, read_summary_block,
};
use anyhow::anyhow;
use shared_eventhub_core::connection::EventHubTarget;
use shared_eventhub_core::engine::{CaptureEngine, PartitionEnd, RunEnd};
use shared_eventhub_persistence::config::{needs_stored_secret, resolve_target_with, save_config};
use shared_eventhub_persistence::export::{
    DiskWriteConfig, DiskWriteMode, ExportOutcome, WritingSink,
};
use shared_eventhub_persistence::paths::DataPaths;
use shared_eventhub_persistence::secrets::SecretsStore;
use shared_eventhub_persistence::store::MessageStore;
use std::io::{self, Write};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, PoisonError};

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
    let paths = DataPaths::new(&job.config.storage.base_data_folder);
    let mut renderer = Renderer::terminal(job.verbose);
    let target = prepare_target(&job, &paths, &mut renderer)?;

    let store = MessageStore::open(&paths)?;
    let disk_write = &job.config.storage.disk_write;
    let sink = WritingSink::new(
        store,
        DiskWriteConfig {
            mode: disk_write.mode,
            inbound_dir: paths.inbound_dir(&disk_write.received_msg_path),
            content_only: disk_write.content_only,
        },
        job.config.capture.filters.clone(),
    );

    renderer.notice(&connecting_notice(&job.config.target));
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

/// Builds the event hub target, reading the secrets file only when a secret
/// the auth method uses was not given, and saves the config first when
/// `--save-config` was given.
fn prepare_target<O: Write, E: Write>(
    job: &ReadJob,
    paths: &DataPaths,
    renderer: &mut Renderer<O, E>,
) -> anyhow::Result<EventHubTarget> {
    let config = &job.config;
    if !job.save_config && !needs_stored_secret(config, &job.secrets) {
        return resolve_target_with(config, job.secrets.clone(), None);
    }

    let secrets = SecretsStore::open(paths)?;
    if job.save_config {
        save_config(&job.working_dir, config, &secrets, &job.secrets)?;
        renderer.notice(&saved_notice(job, paths));
    }
    let target = resolve_target_with(config, job.secrets.clone(), Some(&secrets))?;
    drop(secrets);

    if let (Some(client_id), None) = (config.client_secret_id(), &target.client_secret) {
        anyhow::bail!(
            "No client secret for client '{}': set {CLIENT_SECRET_ENV}, or set it once and save \
             it with --save-config",
            display(client_id)
        );
    }
    Ok(target)
}

fn saved_notice(job: &ReadJob, paths: &DataPaths) -> String {
    let secrets = match (
        job.secrets.connection_string.is_some(),
        job.secrets.client_secret.is_some(),
    ) {
        (true, _) => "the connection string",
        (false, true) => "the client secret",
        (false, false) => return "Saved config.json".to_owned(),
    };
    format!(
        "Saved config.json; {secrets} is stored in {} (not encrypted)",
        display(paths.secrets_db().display().to_string())
    )
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
            let line = Arc::new(Mutex::new(ExportProgressLine::new(io::stdout())));
            let drawer = Arc::clone(&line);
            let finished = sink
                .finish(move |outcome| {
                    drawer
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .draw(outcome);
                })
                .await;
            // The line only holds drawing state, so a panic while it was
            // locked cannot leave anything half-written that matters.
            line.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .finish(finished.as_ref().ok());
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
            let failed: Vec<&str> = summary
                .partitions
                .iter()
                .filter(|(_, end)| matches!(end, PartitionEnd::Failed(_)))
                .map(|(partition_id, _)| partition_id.as_str())
                .collect();
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

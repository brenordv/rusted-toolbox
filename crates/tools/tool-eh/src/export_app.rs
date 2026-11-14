//! `eh export`: writes archived events to txt, csv, or json files.

use crate::models::{ExportJob, RunOutcome};
use crate::progress::{ExportProgressLine, display, export_summary_block};
use anyhow::{Context, anyhow};
use shared_eventhub_persistence::export::{ExportOptions, ExportOutcome};
use shared_eventhub_persistence::paths::DataPaths;
use shared_eventhub_persistence::search::SearchQuery;
use shared_eventhub_persistence::store::MessageStore;
use std::io::{self, Write};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

/// Exports every archived message matching the job's filters. Ctrl-C stops
/// between messages; every file written stays complete.
///
/// # Errors
/// Fails when the data folder has no archive, the archive cannot be opened
/// or read, any message file fails to write, or the archive fails to close.
pub async fn run(job: ExportJob, shutdown: &Arc<AtomicBool>) -> anyhow::Result<RunOutcome> {
    let paths = DataPaths::new(&job.base_data_folder);
    if !paths.archive_db().exists() {
        anyhow::bail!(
            "No archive in '{}'; run `eh read` with this data folder first",
            display(paths.base().display().to_string())
        );
    }
    let store = MessageStore::open(&paths)?;

    let query = SearchQuery {
        filters: job.filters.clone(),
        limit: None,
        after: None,
    };
    let options = ExportOptions {
        format: job.export.format,
        include_metadata: job.export.include_metadata,
        condense_output: job.export.condense_output,
        use_local_time: job.export.use_local_time,
        export_folder: paths.base().join(&job.export.export_folder),
        skip_already_exported: job.export.skip_already_exported,
    };
    let worker = store.clone();
    let cancel = Arc::clone(shutdown);
    let exported = tokio::task::spawn_blocking(move || {
        let mut line = ExportProgressLine::new(io::stdout());
        let exported = worker.export(&query, &options, |outcome| line.draw(outcome), &cancel);
        line.finish(exported.as_ref().ok());
        exported
    })
    .await
    .context("The export stopped unexpectedly")
    .and_then(|exported| exported);

    let closed = store.close().await;
    if let Ok(outcome) = &exported {
        print_block(&mut io::stdout(), &export_summary_block(outcome));
    }

    export_outcome(exported, closed)
}

/// Writes `block` to `out`. Best-effort: the exported files and the export
/// log are the record, so a closed stdout must not fail an export that
/// succeeded; the failure is logged instead.
fn print_block(out: &mut impl Write, block: &str) {
    if let Err(error) = out.write_all(block.as_bytes()).and_then(|()| out.flush()) {
        tracing::warn!(error = %error, "Could not print the export summary");
    }
}

/// Maps the export result and the archive close to the exit outcome; any
/// failure wins over an interrupt. Only an export that actually stopped
/// early counts as interrupted: a Ctrl-C after the last message changes
/// nothing.
pub(crate) fn export_outcome(
    exported: anyhow::Result<ExportOutcome>,
    closed: anyhow::Result<()>,
) -> anyhow::Result<RunOutcome> {
    let closed = closed.context("Failed to close the archive");
    match (exported, closed) {
        (Err(error), Ok(())) => Err(error),
        (Err(error), Err(close)) => Err(anyhow!("{error:#}; {close:#}")),
        (Ok(_), Err(close)) => Err(close),
        (Ok(outcome), Ok(())) if outcome.failed > 0 => Err(anyhow!(
            "{} message file(s) failed to export; the first failure is shown above",
            outcome.failed
        )),
        (Ok(outcome), Ok(())) if outcome.cancelled => Ok(RunOutcome::Interrupted),
        (Ok(_), Ok(())) => Ok(RunOutcome::Completed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(failed: u64, cancelled: bool) -> ExportOutcome {
        ExportOutcome {
            exported: 3,
            skipped: 1,
            failed,
            first_error: (failed > 0).then(|| "out/x.txt: denied".to_owned()),
            cancelled,
        }
    }

    #[rstest::rstest]
    #[case::complete(false, Ok(RunOutcome::Completed))]
    #[case::cancelled(true, Ok(RunOutcome::Interrupted))]
    fn a_clean_export_maps_to_completed_or_interrupted(
        #[case] cancelled: bool,
        #[case] expected: Result<RunOutcome, ()>,
    ) {
        let mapped = export_outcome(Ok(outcome(0, cancelled)), Ok(())).map_err(|_| ());

        assert_eq!(mapped, expected);
    }

    #[test]
    fn a_failed_file_fails_the_export_even_when_cancelled() {
        let error = export_outcome(Ok(outcome(2, true)), Ok(())).unwrap_err();

        assert!(error.to_string().contains("2 message file(s)"), "{error:#}");
    }

    #[test]
    fn a_close_failure_fails_a_successful_export() {
        let error = export_outcome(Ok(outcome(0, false)), Err(anyhow!("disk gone"))).unwrap_err();

        assert!(
            format!("{error:#}").contains("Failed to close the archive: disk gone"),
            "{error:#}"
        );
    }

    #[test]
    fn an_export_error_and_a_close_error_are_both_reported() {
        let error =
            export_outcome(Err(anyhow!("read failed")), Err(anyhow!("disk gone"))).unwrap_err();

        let rendered = format!("{error:#}");
        assert!(rendered.contains("read failed"), "{rendered}");
        assert!(rendered.contains("disk gone"), "{rendered}");
    }

    #[test]
    fn a_summary_that_cannot_be_printed_does_not_panic() {
        print_block(&mut common_cli::test_writers::ClosedPipe, "Exported: 1\n");
    }

    #[test]
    fn the_summary_is_written_whole() {
        let mut out = Vec::new();

        print_block(&mut out, "Exported: 1\n");

        assert_eq!(out, b"Exported: 1\n");
    }

    #[test]
    fn an_export_error_alone_is_returned_as_is() {
        let error = export_outcome(Err(anyhow!("read failed")), Ok(())).unwrap_err();

        assert_eq!(error.to_string(), "read failed");
    }
}

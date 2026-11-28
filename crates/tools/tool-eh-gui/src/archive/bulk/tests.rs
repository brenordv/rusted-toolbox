use super::*;
use crate::archive::results::Results;
use crate::archive::test_support::{messages, page};
use crate::archive::{ArchiveInput, PageRequest};
use crate::read::rules::TextMode;
use eh_core::filters::TextFilter;
use rune::TextEdit;

fn paths() -> DataPaths {
    DataPaths::new(".eh-data")
}

fn act(archive: &mut ArchiveState, input: ArchiveInput) -> ArchiveOutcome {
    archive.apply(input, &paths())
}

/// An archive whose search for "order" showed two rows.
fn searched() -> ArchiveState {
    let mut archive = ArchiveState::default();
    archive.form.text_mode = TextMode::Contains;
    archive.form.text = TextEdit::new("order");
    let out = act(&mut archive, ArchiveInput::SearchPressed);
    let [ArchiveEffect::Search(PageRequest { generation, .. })] = out.effects.as_slice() else {
        panic!("expected a search, got {:?}", out.effects);
    };
    let generation = *generation;
    act(
        &mut archive,
        ArchiveInput::PageLoaded {
            generation,
            elapsed_ms: 1,
            result: Ok(page(messages(2), None)),
        },
    );
    archive
}

fn started(archive: &mut ArchiveState) -> BulkRequest {
    let out = act(archive, ArchiveInput::BulkExportPressed);
    match out.effects.into_iter().next() {
        Some(ArchiveEffect::BulkExport(request)) => request,
        other => panic!("expected a bulk export, got {other:?}"),
    }
}

fn done(export: u64, result: Result<ExportOutcome, String>) -> ArchiveInput {
    ArchiveInput::BulkDone {
        export,
        elapsed_ms: 5,
        result,
    }
}

fn failed_files(cancelled: bool) -> ExportOutcome {
    ExportOutcome {
        exported: 3,
        failed: 2,
        first_error: Some("x.txt: disk full".to_owned()),
        cancelled,
        ..ExportOutcome::default()
    }
}

#[test]
fn the_export_covers_the_last_search_from_its_start_not_the_edited_form() {
    let mut archive = searched();
    archive.form.text = TextEdit::new("invoice");
    archive.options.format = ExportFormat::Json;

    let request = started(&mut archive);

    assert!(request.query.after.is_none());
    assert!(request.query.limit.is_some());
    assert!(matches!(
        &request.query.filters.text,
        Some(TextFilter::Contains(needles)) if needles == &["order".to_owned()]
    ));
    assert_eq!(request.options.format, ExportFormat::Json);
    assert_eq!(
        request.options.export_folder,
        paths().base().join("exports")
    );
    assert!(archive.bulk.is_running());
}

#[test]
fn options_map_onto_the_export_options() {
    let options = BulkOptions {
        format: ExportFormat::Csv,
        metadata: false,
        condense: true,
        local_time: true,
        skip_exported: false,
        export_folder: "out".to_owned(),
        destination: Some(PathBuf::from("picked")),
    };

    let mapped = options.export_options(&paths());

    assert_eq!(
        mapped,
        ExportOptions {
            format: ExportFormat::Csv,
            include_metadata: false,
            condense_output: true,
            use_local_time: true,
            export_folder: PathBuf::from("picked"),
            skip_already_exported: false,
        }
    );
}

#[test]
fn options_start_from_the_configs_export_settings() {
    let settings = ExportSettings {
        format: ExportFormat::Json,
        skip_already_exported: false,
        ..ExportSettings::default()
    };

    let options = BulkOptions::from_settings(&settings);

    assert_eq!(options.format, ExportFormat::Json);
    assert!(!options.skip_exported);
    assert!(options.destination.is_none());
}

#[test]
fn nothing_exports_before_a_search() {
    let mut archive = ArchiveState::default();

    let out = act(&mut archive, ArchiveInput::BulkExportPressed);

    assert!(out.effects.is_empty());
    assert!(matches!(archive.bulk, BulkExport::Idle));
}

#[test]
fn a_second_export_is_refused_while_one_runs() {
    let mut archive = searched();
    started(&mut archive);

    let out = act(&mut archive, ArchiveInput::BulkExportPressed);

    assert!(out.effects.is_empty());
}

#[test]
fn cancel_sets_the_flag_once() {
    let mut archive = searched();
    let request = started(&mut archive);

    let first = act(&mut archive, ArchiveInput::CancelBulk);
    let second = act(&mut archive, ArchiveInput::CancelBulk);

    assert!(request.cancel.load(Ordering::Acquire));
    assert!(first.changed && !second.changed);
    assert!(matches!(
        archive.bulk,
        BulkExport::Running {
            cancel_requested: true,
            ..
        }
    ));
}

#[test]
fn cancel_and_dismiss_do_nothing_where_they_do_not_apply() {
    let mut archive = searched();

    let idle_cancel = act(&mut archive, ArchiveInput::CancelBulk);
    let idle_dismiss = act(&mut archive, ArchiveInput::DismissBulk);
    let request = started(&mut archive);
    let running_dismiss = act(&mut archive, ArchiveInput::DismissBulk);
    act(
        &mut archive,
        done(request.export, Ok(ExportOutcome::default())),
    );
    let done_cancel = act(&mut archive, ArchiveInput::CancelBulk);

    assert!(!idle_cancel.changed && !idle_dismiss.changed);
    assert!(!running_dismiss.changed && !done_cancel.changed);
    assert!(matches!(archive.bulk, BulkExport::Done { .. }));
}

#[test]
fn progress_replaces_the_counts_of_its_own_run_only() {
    let mut archive = searched();
    let request = started(&mut archive);
    let counts = ExportOutcome {
        exported: 7,
        ..ExportOutcome::default()
    };

    act(
        &mut archive,
        ArchiveInput::BulkProgress {
            export: request.export,
            counts: counts.clone(),
        },
    );
    let other = act(
        &mut archive,
        ArchiveInput::BulkProgress {
            export: request.export + 1,
            counts: ExportOutcome::default(),
        },
    );

    assert!(!other.changed);
    assert!(matches!(&archive.bulk, BulkExport::Running { counts: c, .. } if *c == counts));
}

#[test]
fn a_late_progress_or_end_after_done_is_ignored() {
    let mut archive = searched();
    let request = started(&mut archive);
    act(
        &mut archive,
        done(request.export, Ok(ExportOutcome::default())),
    );

    let progress = act(
        &mut archive,
        ArchiveInput::BulkProgress {
            export: request.export,
            counts: failed_files(false),
        },
    );
    let end = act(&mut archive, done(request.export, Err("late".to_owned())));

    assert!(!progress.changed && !end.changed);
    assert!(matches!(
        &archive.bulk,
        BulkExport::Done { result: Ok(_), .. }
    ));
}

#[test]
fn failed_files_report_the_first_error_even_when_cancelled() {
    let mut archive = searched();
    let request = started(&mut archive);

    let out = act(&mut archive, done(request.export, Ok(failed_files(true))));

    assert!(out.run_failed);
    assert!(out.toasts.is_empty());
    assert!(
        out.bulk_failure
            .as_deref()
            .is_some_and(|text| text.contains("disk full"))
    );
}

#[test]
fn a_run_that_stopped_shows_its_error() {
    let mut archive = searched();
    let request = started(&mut archive);

    let out = act(
        &mut archive,
        done(request.export, Err("read failed".to_owned())),
    );

    assert!(out.run_failed);
    assert!(
        out.bulk_failure
            .as_deref()
            .is_some_and(|text| text.contains("read failed"))
    );
    assert!(!archive.bulk.is_running());
}

#[test]
fn a_clean_or_cancelled_run_is_no_failure() {
    let mut archive = searched();
    let request = started(&mut archive);
    let cancelled = ExportOutcome {
        exported: 4,
        cancelled: true,
        ..ExportOutcome::default()
    };

    let out = act(&mut archive, done(request.export, Ok(cancelled)));

    assert!(!out.run_failed);
    assert!(out.toasts.is_empty());
}

#[test]
fn a_new_search_keeps_a_running_export() {
    let mut archive = searched();
    started(&mut archive);

    act(&mut archive, ArchiveInput::SearchPressed);

    assert!(archive.bulk.is_running());
    assert!(matches!(archive.results, Results::Loading { .. }));
}

#[test]
fn a_new_data_folder_clears_a_finished_export() {
    let mut archive = searched();
    let request = started(&mut archive);
    act(
        &mut archive,
        done(request.export, Ok(ExportOutcome::default())),
    );

    archive.folder_changed();

    assert!(matches!(archive.bulk, BulkExport::Idle));
}

#[test]
fn a_closed_window_asks_the_running_export_to_stop() {
    let mut archive = searched();
    let request = started(&mut archive);

    archive.window_closed();

    assert!(request.cancel.load(Ordering::Acquire));
}

#[test]
fn progress_is_due_first_and_then_once_per_interval() {
    let start = Instant::now();

    assert!(progress_due(None, start));
    assert!(!progress_due(Some(start), start + PROGRESS_EVERY / 2));
    assert!(progress_due(Some(start), start + PROGRESS_EVERY));
}

#[test]
fn the_failure_toast_counts_files_in_words_that_agree() {
    assert_eq!(
        failed_files_text(1, "x.txt: disk full"),
        "1 file could not be written: x.txt: disk full"
    );
    assert!(failed_files_text(2, "x.txt: disk full").starts_with("2 files could not"));
}

#[test]
fn the_counts_line_reads_each_count() {
    let line = counts_line(&ExportOutcome {
        exported: 1_200,
        skipped: 3,
        failed: 0,
        ..ExportOutcome::default()
    });

    assert_eq!(line, "Exported 1,200 · Skipped 3 · Failed 0");
}

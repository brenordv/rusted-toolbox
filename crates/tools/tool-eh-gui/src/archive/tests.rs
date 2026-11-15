use super::test_support::{cursor, messages, page};
use super::*;
use crate::read::inspector::{ExportPhase, FeedbackKind};
use crate::read::rules::{TextMode, TimeMode};
use rune::TextEdit;
use shared_eventhub_core::models::MessageKey;
use shared_eventhub_persistence::export::ExportFormat;

fn paths() -> DataPaths {
    DataPaths::new(".eh-data")
}

fn page_request(out: &ArchiveOutcome) -> &PageRequest {
    match out.effects.as_slice() {
        [ArchiveEffect::Search(request)] => request,
        other => panic!("expected one page request, got {other:?}"),
    }
}

impl ArchiveState {
    fn act(&mut self, input: ArchiveInput) -> ArchiveOutcome {
        self.apply(input, &paths())
    }

    fn search(&mut self) -> u64 {
        let out = self.act(ArchiveInput::SearchPressed);
        page_request(&out).generation
    }

    fn land_page(&mut self, generation: u64, page: SearchPage) -> ArchiveOutcome {
        self.act(ArchiveInput::PageLoaded {
            generation,
            elapsed_ms: 1,
            result: Ok(page),
        })
    }

    /// A search whose first page of `count` rows landed, with more to come when `more`.
    fn shown(&mut self, count: usize, more: bool) -> u64 {
        let generation = self.search();
        self.land_page(generation, page(messages(count), more.then(cursor)));
        generation
    }

    fn more(&self) -> &More {
        match &self.results {
            Results::Shown { more, .. } => more,
            other => panic!("expected Shown, got {other:?}"),
        }
    }

    fn keys(&self) -> Vec<String> {
        (0..self.count())
            .map(|position| self.row(position).unwrap().key_str.clone())
            .collect()
    }
}

#[test]
fn search_asks_for_the_first_page_of_the_form_with_the_page_limit() {
    let mut archive = ArchiveState::default();
    archive.form.text_mode = TextMode::Contains;
    archive.form.text = TextEdit::new("order");

    let out = archive.act(ArchiveInput::SearchPressed);

    let request = page_request(&out);
    assert_eq!(request.query.limit, NonZeroUsize::new(ARCHIVE_PAGE));
    assert!(request.query.after.is_none());
    assert!(matches!(
        &request.query.filters.text,
        Some(TextFilter::Contains(needles)) if needles == &["order".to_owned()]
    ));
    assert!(matches!(archive.results, Results::Loading { .. }));
}

#[test]
fn an_invalid_form_starts_no_search() {
    let mut archive = ArchiveState::default();
    archive.form.time_mode = TimeMode::NewerThan;

    let out = archive.act(ArchiveInput::SearchPressed);

    assert!(out.effects.is_empty());
    assert!(matches!(archive.results, Results::NotSearched));
    assert!(!archive.searchable());
}

#[test]
fn search_is_refused_while_the_first_page_is_read() {
    let mut archive = ArchiveState::default();
    archive.search();

    let out = archive.act(ArchiveInput::SearchPressed);

    assert!(out.effects.is_empty());
    assert!(!archive.searchable());
}

#[test]
fn search_is_refused_while_a_later_page_is_read() {
    let mut archive = ArchiveState::default();
    archive.shown(300, true);
    archive.act(ArchiveInput::VisibleRows { end: 300 });

    let out = archive.act(ArchiveInput::SearchPressed);

    assert!(matches!(archive.more(), More::Loading(_)));
    assert!(out.effects.is_empty());
}

#[test]
fn the_first_page_shows_its_rows_and_what_comes_next() {
    let mut archive = ArchiveState::default();

    archive.shown(3, false);

    assert_eq!(archive.keys(), ["p0-s0", "p0-s1", "p0-s2"]);
    assert!(matches!(archive.more(), More::End));
}

#[test]
fn a_page_of_an_earlier_search_is_dropped() {
    let mut archive = ArchiveState::default();
    let first = archive.search();
    archive.land_page(first, page(messages(1), None));
    let second = archive.search();

    let out = archive.land_page(first, page(messages(5), None));

    assert!(!out.changed);
    assert!(matches!(archive.results, Results::Loading { generation } if generation == second));
}

#[test]
fn a_first_page_landing_after_a_folder_change_is_dropped() {
    let mut archive = ArchiveState::default();
    let generation = archive.search();
    archive.folder_changed();

    let out = archive.land_page(generation, page(messages(3), None));

    assert!(!out.changed);
    assert!(matches!(archive.results, Results::NotSearched));
    assert_eq!(archive.count(), 0);
}

#[test]
fn a_page_nothing_asked_for_is_dropped() {
    let mut archive = ArchiveState::default();
    let generation = archive.shown(300, true);

    let out = archive.land_page(generation, page(messages(5), None));

    assert!(!out.changed);
    assert_eq!(archive.count(), 300);
    assert!(matches!(archive.more(), More::Ready(_)));
}

#[test]
fn near_the_end_the_next_page_is_asked_for_once() {
    let mut archive = ArchiveState::default();
    let generation = archive.shown(300, true);

    let far = archive.act(ArchiveInput::VisibleRows { end: 20 });
    let near = archive.act(ArchiveInput::VisibleRows { end: 250 });
    let again = archive.act(ArchiveInput::VisibleRows { end: 260 });

    assert!(far.effects.is_empty());
    let request = page_request(&near);
    assert_eq!(request.generation, generation);
    assert!(request.query.after.is_some());
    assert!(again.effects.is_empty());
}

#[test]
fn a_later_page_appends_and_skips_messages_already_loaded() {
    let mut archive = ArchiveState::default();
    let generation = archive.shown(2, true);
    archive.act(ArchiveInput::VisibleRows { end: 2 });
    let mut items = messages(4);
    items.remove(0);

    archive.land_page(generation, page(items, None));

    assert_eq!(archive.keys(), ["p0-s0", "p0-s1", "p0-s2", "p0-s3"]);
    assert!(matches!(archive.more(), More::End));
}

#[test]
fn a_landing_already_near_the_end_asks_for_the_next_page() {
    let mut archive = ArchiveState::default();
    let generation = archive.search();
    archive.act(ArchiveInput::VisibleRows { end: 15 });

    let out = archive.land_page(generation, page(messages(20), Some(cursor())));

    assert_eq!(page_request(&out).generation, generation);
    assert!(matches!(archive.more(), More::Loading(_)));
}

#[test]
fn a_new_search_forgets_where_the_old_list_was_scrolled() {
    let mut archive = ArchiveState::default();
    archive.shown(5, false);
    archive.act(ArchiveInput::VisibleRows { end: 5 });
    let generation = archive.search();

    let out = archive.land_page(generation, page(messages(300), Some(cursor())));

    assert!(out.effects.is_empty());
    assert!(matches!(archive.more(), More::Ready(_)));
}

#[test]
fn visible_rows_with_nothing_more_to_load_ask_for_nothing() {
    let mut archive = ArchiveState::default();
    archive.shown(5, false);

    let out = archive.act(ArchiveInput::VisibleRows { end: 5 });

    assert!(out.effects.is_empty());
    assert!(!out.changed);
}

#[test]
fn a_failed_first_page_leaves_the_archive_unavailable_with_its_error() {
    let mut archive = ArchiveState::default();
    let generation = archive.search();

    archive.act(ArchiveInput::PageLoaded {
        generation,
        elapsed_ms: 1,
        result: Err("in use by another eh or eh-gui".to_owned()),
    });

    assert!(matches!(
        &archive.results,
        Results::Unavailable { error, .. } if error.contains("in use")
    ));
    assert!(archive.searchable());
}

#[test]
fn a_failed_later_page_shows_its_error_and_retry_asks_again() {
    let mut archive = ArchiveState::default();
    let generation = archive.shown(300, true);
    let first = archive.act(ArchiveInput::VisibleRows { end: 300 });
    let asked = format!("{:?}", page_request(&first).query.after);
    archive.act(ArchiveInput::PageLoaded {
        generation,
        elapsed_ms: 1,
        result: Err("read failed".to_owned()),
    });
    let failed = matches!(archive.more(), More::Failed { error, .. } if error == "read failed");

    let retry = archive.act(ArchiveInput::RetryPage);

    assert!(failed);
    assert_eq!(format!("{:?}", page_request(&retry).query.after), asked);
    assert!(matches!(archive.more(), More::Loading(_)));
}

#[test]
fn retry_without_a_failed_page_does_nothing() {
    let mut archive = ArchiveState::default();
    archive.shown(300, true);

    let out = archive.act(ArchiveInput::RetryPage);

    assert!(out.effects.is_empty());
    assert!(matches!(archive.more(), More::Ready(_)));
}

#[test]
fn two_searches_that_land_the_same_count_never_share_a_list_version() {
    let mut archive = ArchiveState::default();
    archive.shown(3, false);
    let first = archive.epoch();

    archive.shown(3, false);

    assert_ne!(archive.epoch(), first);
}

#[test]
fn a_new_data_folder_starts_over() {
    let mut archive = ArchiveState::default();
    archive.shown(3, false);
    archive.options.destination = Some(PathBuf::from("elsewhere"));

    archive.folder_changed();

    assert!(matches!(archive.results, Results::NotSearched));
    assert!(!archive.searched());
    assert!(archive.options.destination.is_none());
    assert_eq!(archive.count(), 0);
}

#[test]
fn selecting_a_result_opens_it_and_keys_move_the_selection() {
    let mut archive = ArchiveState::default();
    archive.shown(3, false);

    archive.act(ArchiveInput::Select("p0-s1".to_owned()));
    archive.act(ArchiveInput::Move(Step::Down));

    let selected = archive.inspector.selected.as_ref().unwrap();
    assert_eq!(selected.key_str, "p0-s2");
    assert!(archive.inspector.pane.is_open());
}

#[test]
fn a_result_that_is_no_longer_shown_cannot_be_selected() {
    let mut archive = ArchiveState::default();

    let out = archive.act(ArchiveInput::Select("p0-s1".to_owned()));

    assert!(!out.changed);
    assert!(archive.inspector.selected.is_none());
}

fn picking(archive: &mut ArchiveState) -> MessageKey {
    archive.shown(3, false);
    archive.act(ArchiveInput::Select("p0-s0".to_owned()));
    archive.act(ArchiveInput::Inspector(InspectorInput::ExportPressed));
    archive.inspector.selected.as_ref().unwrap().key.clone()
}

fn owner_of(archive: &ArchiveState) -> u64 {
    match &archive.inspector.export {
        ExportPhase::Picking { owner, .. } | ExportPhase::Running { owner, .. } => *owner,
        ExportPhase::Idle => panic!("no export"),
    }
}

fn answered(archive: &mut ArchiveState, owner: u64, key: &MessageKey) -> ArchiveOutcome {
    archive.act(ArchiveInput::Inspector(InspectorInput::ExportAnswered {
        owner,
        key: key.clone(),
        answer: Ok(Some(PathBuf::from("one.txt"))),
    }))
}

#[test]
fn a_picked_file_exports_the_selected_result_back_to_the_export_section() {
    let mut archive = ArchiveState::default();
    let key = picking(&mut archive);
    let owner = owner_of(&archive);

    let out = answered(&mut archive, owner, &key);

    let [ArchiveEffect::ExportOne(request)] = out.effects.as_slice() else {
        panic!("expected an export, got {:?}", out.effects);
    };
    assert_eq!(request.section, ListSection::Export);
    assert_eq!(request.key, key);
    assert!(archive.busy());
}

#[test]
fn a_file_picked_after_a_new_search_is_not_written_and_says_so() {
    let mut archive = ArchiveState::default();
    let key = picking(&mut archive);
    let owner = owner_of(&archive);
    archive.search();

    let out = answered(&mut archive, owner, &key);

    assert!(out.effects.is_empty());
    assert!(matches!(
        out.toasts.as_slice(),
        [(ToastKind::Info, text)] if text == DROPPED_ANSWER
    ));
}

#[test]
fn a_running_export_survives_a_new_search_and_a_new_folder() {
    let mut archive = ArchiveState::default();
    archive.inspector.format = ExportFormat::Json;
    let key = picking(&mut archive);
    let owner = owner_of(&archive);
    answered(&mut archive, owner, &key);

    archive.search();
    archive.folder_changed();
    let still_running = archive.busy();
    archive.act(ArchiveInput::Inspector(InspectorInput::ExportDone {
        owner,
        key,
        result: Ok(()),
        now: Utc::now(),
    }));

    assert!(still_running);
    assert!(!archive.busy());
    assert_eq!(archive.inspector.format, ExportFormat::Json);
}

#[test]
fn exported_feedback_shows_only_while_its_results_are_shown() {
    let mut archive = ArchiveState::default();
    let key = picking(&mut archive);
    let owner = owner_of(&archive);
    answered(&mut archive, owner, &key);

    archive.act(ArchiveInput::Inspector(InspectorInput::ExportDone {
        owner,
        key: key.clone(),
        result: Ok(()),
        now: Utc::now(),
    }));

    let feedback = archive.inspector.feedback.as_ref().unwrap();
    assert!(feedback.shows(&key, FeedbackKind::Exported));
}

#[test]
fn exported_feedback_stays_hidden_once_the_data_folder_changed() {
    let mut archive = ArchiveState::default();
    let key = picking(&mut archive);
    let owner = owner_of(&archive);
    answered(&mut archive, owner, &key);
    archive.folder_changed();

    archive.act(ArchiveInput::Inspector(InspectorInput::ExportDone {
        owner,
        key,
        result: Ok(()),
        now: Utc::now(),
    }));

    assert!(archive.inspector.feedback.is_none());
    assert!(!archive.busy());
}

#[test]
fn the_destination_picker_failing_is_a_danger_toast() {
    let mut archive = ArchiveState::default();

    let out = archive.act(ArchiveInput::DestinationFailed("no portal".to_owned()));

    assert!(
        matches!(out.toasts.as_slice(), [(ToastKind::Danger, text)] if text.contains("no portal"))
    );
    assert!(archive.options.destination.is_none());
}

#[test]
fn a_picked_destination_replaces_the_data_folders_export_folder() {
    let mut archive = ArchiveState::default();
    let before = archive.options.destination(&paths());

    archive.act(ArchiveInput::DestinationPicked(PathBuf::from("picked")));

    assert_eq!(before, paths().base().join("exports"));
    assert_eq!(
        archive.options.destination(&paths()),
        PathBuf::from("picked")
    );
}

#[test]
fn clear_empties_the_form_and_keeps_the_results() {
    let mut archive = ArchiveState::default();
    archive.shown(2, false);
    archive.form.text_mode = TextMode::Regex;

    archive.act(ArchiveInput::ClearForm);

    assert_eq!(archive.form.text_mode, TextMode::None);
    assert_eq!(archive.count(), 2);
}

#[test]
fn the_log_names_a_text_condition_by_its_kind() {
    let regex = CaptureFilters {
        time: None,
        text: Some(TextFilter::regex("a+").unwrap()),
    };

    assert_eq!(text_mode(&CaptureFilters::default()), "none");
    assert_eq!(text_mode(&regex), "regex");
}

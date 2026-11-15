use super::*;
use crate::bridge::test_support::detached_session;
use crate::read::inspector::{ExportPhase, FeedbackKind, InspectorInput};
use crate::read::search::Matcher;
use crate::read::test_support::captured;
use shared_eventhub_core::filters::TextFilter;
use std::path::{Path, PathBuf};

struct Fixture {
    rt: tokio::runtime::Runtime,
    read: ReadState,
}

fn fixture() -> Fixture {
    let mut read = ReadState::default();
    read.start_session(1, Duration::from_secs(1));
    Fixture {
        rt: tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap(),
        read,
    }
}

fn paths() -> DataPaths {
    DataPaths::new(".eh-data")
}

impl Fixture {
    fn connected(&self) -> ConnectionState {
        let (session, _) = detached_session(1, self.rt.handle());
        ConnectionState::Connected {
            session,
            partitions: vec!["0".to_owned()],
        }
    }

    fn apply(&mut self, input: ReadInput) -> ReadOutcome {
        let connection = self.connected();
        self.read.apply(input, &connection, paths())
    }

    fn arrive(&mut self, bodies: &[&str]) {
        let start = self.read.rows.displayed().len() + self.read.rows.incoming_len();
        for (offset, body) in bodies.iter().enumerate() {
            self.read
                .message(captured("0", (start + offset) as i64, body));
        }
    }

    fn arrive_and_flush(&mut self, bodies: &[&str]) {
        self.arrive(bodies);
        self.apply(ReadInput::ListFlush);
    }

    fn search(&mut self, text: &str, regex: bool) -> ReadOutcome {
        self.read.search_edit = TextEdit::new(text);
        self.read.search_regex = regex;
        self.apply(ReadInput::SearchEdited)
    }

    fn visible(&self) -> Vec<u64> {
        match &self.read.search {
            SearchState::Active { visible, .. } => visible.clone(),
            SearchState::Off => panic!("the search is off"),
        }
    }
}

fn rebuild_request(out: ReadOutcome) -> RebuildRequest {
    match out.effects.into_iter().next() {
        Some(ReadEffect::RebuildSearch(request)) => request,
        other => panic!("expected a rebuild, got {other:?}"),
    }
}

fn landed(request: &RebuildRequest) -> ReadInput {
    let matches = request
        .rows
        .iter()
        .filter(|(_, message)| request.matcher.matches(message))
        .map(|(index, _)| *index)
        .collect();
    ReadInput::SearchRebuilt(Rebuilt {
        generation: request.current,
        matcher: request.matcher.clone(),
        result: Ok(matches),
        upto: request.upto,
        follow_up: request.follow_up,
        scanned: request.rows.len(),
        elapsed_ms: 0,
    })
}

#[test]
fn arrived_rows_wait_for_a_flush_that_lands_them_without_motion() {
    let mut f = fixture();
    f.arrive(&["a", "b"]);

    let out = f.apply(ReadInput::ListFlush);
    let empty = f.apply(ReadInput::ListFlush);

    assert!(out.changed && out.rows_landed);
    assert_eq!(f.read.rows.displayed().len(), 2);
    assert!(!empty.changed && !empty.rows_landed);
}

#[test]
fn a_selection_keeps_its_message_after_its_row_is_evicted() {
    let mut f = fixture();
    f.read.rows = RowBuffer::with_caps(2, usize::MAX);
    f.arrive_and_flush(&["first", "second"]);
    f.apply(ReadInput::Select("p0-s0".to_owned()));

    f.arrive_and_flush(&["third"]);

    assert!(f.read.rows.by_key("p0-s0").is_none());
    assert_eq!(
        f.read.inspector.selected.as_ref().unwrap().message.body,
        "first"
    );
    assert!(f.read.inspector.pane.is_open());
}

#[test]
fn a_search_edit_snapshots_the_displayed_rows_up_to_the_last_one() {
    let mut f = fixture();
    f.arrive_and_flush(&["bacon", "eggs"]);
    f.arrive(&["late bacon"]);

    let request = rebuild_request(f.search("bacon", false));

    assert_eq!(request.rows.len(), 2);
    assert_eq!(request.upto, Some(1));
    assert_eq!(f.read.search.rebuilding(), Some(request.current));
}

#[test]
fn a_landed_search_shows_matches_including_rows_that_arrived_meanwhile() {
    let mut f = fixture();
    f.arrive_and_flush(&["bacon", "eggs"]);
    let request = rebuild_request(f.search("bacon", false));
    f.arrive_and_flush(&["late bacon", "late eggs"]);
    let epoch = f.read.rows.epoch;

    let out = f.apply(landed(&request));

    assert!(out.changed);
    assert_eq!(f.visible(), [0, 2]);
    assert_eq!(f.read.list_len(), 2);
    assert!(f.read.rows.epoch > epoch);
}

#[test]
fn an_invalid_edit_leaves_the_pending_rebuild_to_land() {
    let mut f = fixture();
    f.arrive_and_flush(&["order-1", "eggs"]);
    let request = rebuild_request(f.search("order", true));
    let generation = f.read.generation.load(Ordering::Acquire);

    let invalid = f.search("order-(", true);
    f.apply(landed(&request));

    assert!(invalid.effects.is_empty());
    assert!(f.read.search_error.is_some());
    assert_eq!(f.read.generation.load(Ordering::Acquire), generation);
    assert_eq!(f.visible(), [0]);
    assert_eq!(f.read.search.rebuilding(), None);
}

#[test]
fn escape_during_a_rebuild_keeps_the_filter_off_when_the_result_lands() {
    let mut f = fixture();
    f.arrive_and_flush(&["bacon", "eggs"]);
    let request = rebuild_request(f.search("bacon", false));

    f.apply(ReadInput::SearchCleared);
    let out = f.apply(landed(&request));

    assert!(!out.changed);
    assert!(matches!(f.read.search, SearchState::Off));
    assert_eq!(f.read.list_len(), 2);
}

#[test]
fn a_result_from_before_a_new_session_is_dropped() {
    let mut f = fixture();
    f.arrive_and_flush(&["bacon"]);
    let request = rebuild_request(f.search("bacon", false));

    f.read.start_session(2, Duration::from_secs(1));
    f.arrive_and_flush(&["bacon"]);
    rebuild_request(f.search("bacon", false));
    let out = f.apply(landed(&request));

    assert!(!out.changed);
    assert!(f.read.search.rebuilding().is_some());
}

#[test]
fn a_flush_with_too_many_bytes_for_the_ui_thread_starts_a_rebuild() {
    let mut f = fixture();
    f.arrive_and_flush(&["bacon"]);
    let request = rebuild_request(f.search("bacon", false));
    f.apply(landed(&request));
    let big = "bacon".repeat(search::UI_MATCH_BYTES / 5 + 1);

    f.arrive(&[&big]);
    let out = f.apply(ReadInput::ListFlush);

    let follow = rebuild_request(out);
    assert_eq!(follow.rows.len(), 2);
    assert_eq!(follow.upto, Some(1));
}

#[test]
fn a_small_flush_matches_new_rows_on_the_spot() {
    let mut f = fixture();
    f.arrive_and_flush(&["bacon"]);
    let request = rebuild_request(f.search("bacon", false));
    f.apply(landed(&request));

    f.arrive_and_flush(&["eggs", "bacon again"]);

    assert_eq!(f.visible(), [0, 2]);
}

#[test]
fn the_regex_toggle_starts_a_new_rebuild() {
    let mut f = fixture();
    f.arrive_and_flush(&["order-7"]);
    f.search("order-\\d", false);

    let out = f.search("order-\\d", true);

    assert!(matches!(rebuild_request(out).matcher, Matcher::Regex(_)));
}

#[test]
fn moving_the_selection_walks_the_list_and_stops_at_its_ends() {
    let mut f = fixture();
    f.arrive_and_flush(&["a", "b", "c"]);

    f.apply(ReadInput::Move(Step::Up));
    let first = f.read.inspector.selected.as_ref().unwrap().key_str.clone();
    f.apply(ReadInput::Move(Step::Up));
    f.apply(ReadInput::Move(Step::First));
    f.apply(ReadInput::Move(Step::Up));
    let top = f.read.inspector.selected.as_ref().unwrap().key_str.clone();
    f.apply(ReadInput::ClearSelection);

    assert_eq!(first, "p0-s2");
    assert_eq!(top, "p0-s0");
    assert!(f.read.inspector.selected.is_none());
    assert!(!f.read.inspector.pane.is_open());
}

#[test]
fn arrivals_out_of_view_grow_the_pill_until_the_bottom_is_reached() {
    let mut f = fixture();
    f.arrive_and_flush(&["a", "b", "c"]);
    f.read.visible_rows(1, 3);

    f.arrive_and_flush(&["d", "e"]);
    f.read.tick(Utc::now(), true);
    let shows = f.read.tail.pill_shows();
    let flipped = f.read.visible_rows(5, 5);

    assert!(shows);
    assert_eq!(f.read.tail.shown_unseen, 0);
    assert!(flipped);
}

#[test]
fn the_pill_waits_for_two_rows_from_the_bottom() {
    let mut f = fixture();
    f.arrive_and_flush(&["a", "b"]);
    f.read.visible_rows(1, 2);
    f.arrive_and_flush(&["c"]);
    f.read.tick(Utc::now(), true);

    f.read.visible_rows(1, 3);
    assert!(f.read.tail.pill_shows());
    f.read.visible_rows(2, 3);
    assert!(!f.read.tail.pill_shows());
}

#[test]
fn apply_while_connected_sends_the_rules_to_the_engine() {
    let mut f = fixture();
    f.read.rules.text_mode = rules::TextMode::Contains;
    f.read.rules.text = TextEdit::new("bacon");

    let out = f.apply(ReadInput::ApplyRules);

    let expected = CaptureFilters {
        time: None,
        text: Some(TextFilter::Contains(vec!["bacon".to_owned()])),
    };
    assert!(matches!(
        out.effects.as_slice(),
        [ReadEffect::UpdateFilters(filters)] if *filters == expected
    ));
    assert_eq!(f.read.rules_applied, expected);
}

#[test]
fn apply_while_disconnected_keeps_the_rules_for_the_next_connect() {
    let mut f = fixture();
    f.read.rules.text_mode = rules::TextMode::Contains;
    f.read.rules.text = TextEdit::new("bacon");

    let out = f.read.apply(
        ReadInput::ApplyRules,
        &ConnectionState::Disconnected,
        paths(),
    );

    assert!(out.effects.is_empty() && out.changed);
    assert_eq!(rules::rule_count(&f.read.rules_applied), 1);
}

#[test]
fn apply_while_the_session_is_being_prepared_is_refused() {
    let mut f = fixture();
    f.read.rules.text_mode = rules::TextMode::Contains;
    f.read.rules.text = TextEdit::new("bacon");
    let preparing = ConnectionState::Connecting {
        attempt: 1,
        session: None,
        fatal: None,
    };

    let out = f.read.apply(ReadInput::ApplyRules, &preparing, paths());

    assert!(!out.changed && out.effects.is_empty());
    assert_eq!(f.read.rules_applied, CaptureFilters::default());
    assert!(!rules_editable(&preparing));
}

#[test]
fn apply_while_the_session_is_connecting_sends_the_rules_ahead_of_start() {
    let mut f = fixture();
    f.read.rules.text_mode = rules::TextMode::Contains;
    f.read.rules.text = TextEdit::new("bacon");
    let (session, _) = detached_session(1, f.rt.handle());
    let connecting = ConnectionState::Connecting {
        attempt: 1,
        session: Some(session),
        fatal: None,
    };

    let out = f.read.apply(ReadInput::ApplyRules, &connecting, paths());

    assert!(matches!(
        out.effects.as_slice(),
        [ReadEffect::UpdateFilters(_)]
    ));
}

#[test]
fn apply_while_stopping_keeps_the_rules_for_the_next_connect() {
    let mut f = fixture();
    f.read.rules.text_mode = rules::TextMode::Contains;
    f.read.rules.text = TextEdit::new("bacon");
    let (session, _) = detached_session(1, f.rt.handle());

    let out = f.read.apply(
        ReadInput::ApplyRules,
        &ConnectionState::Stopping { session },
        paths(),
    );

    assert!(out.changed && out.effects.is_empty());
    assert_eq!(rules::rule_count(&f.read.rules_applied), 1);
}

#[test]
fn clear_while_the_session_is_being_prepared_keeps_the_draft() {
    let mut f = fixture();
    f.read.rules.text_mode = rules::TextMode::Contains;
    let preparing = ConnectionState::Connecting {
        attempt: 1,
        session: None,
        fatal: None,
    };

    let out = f.read.apply(ReadInput::ClearRules, &preparing, paths());

    assert!(!out.changed);
    assert_eq!(f.read.rules.text_mode, rules::TextMode::Contains);
}

#[test]
fn starting_and_clearing_a_search_each_move_the_epoch() {
    let mut f = fixture();
    f.arrive_and_flush(&["bacon"]);
    let before = f.read.rows.epoch;

    f.search("bacon", false);
    let started = f.read.rows.epoch;
    f.apply(ReadInput::SearchCleared);

    assert!(started > before);
    assert!(f.read.rows.epoch > started);
}

#[test]
fn a_failed_rebuild_shows_every_row_and_says_why() {
    let mut f = fixture();
    f.arrive_and_flush(&["bacon", "eggs"]);
    let request = rebuild_request(f.search("bacon", false));

    let out = f.apply(ReadInput::SearchRebuilt(Rebuilt {
        result: Err("boom".to_owned()),
        ..match landed(&request) {
            ReadInput::SearchRebuilt(rebuilt) => rebuilt,
            other => panic!("expected a rebuild, got {other:?}"),
        }
    }));

    assert!(out.changed);
    assert!(matches!(f.read.search, SearchState::Off));
    assert_eq!(f.read.list_len(), 2);
    assert!(matches!(out.toasts.as_slice(), [(ToastKind::Danger, _)]));
    assert!(f.read.search_error.is_none());
}

#[test]
fn an_invalid_draft_is_not_applied() {
    let mut f = fixture();
    f.read.rules.text_mode = rules::TextMode::Regex;
    f.read.rules.text = TextEdit::new("(");

    let out = f.apply(ReadInput::ApplyRules);

    assert!(!out.changed && out.effects.is_empty());
}

#[test]
fn clear_applies_no_rules() {
    let mut f = fixture();
    f.read.rules_applied = CaptureFilters {
        time: None,
        text: Some(TextFilter::Contains(vec!["x".to_owned()])),
    };

    let out = f.apply(ReadInput::ClearRules);

    assert!(matches!(
        out.effects.as_slice(),
        [ReadEffect::UpdateFilters(filters)] if *filters == CaptureFilters::default()
    ));
    assert_eq!(f.read.rules.text_mode, rules::TextMode::None);
}

fn stats_at(last_message: Option<DateTime<Utc>>) -> StatsSnapshot {
    StatsSnapshot {
        received: 5,
        last_message_at: last_message,
        started_at: Utc::now() - TimeDelta::minutes(5),
        ..StatsSnapshot::default()
    }
}

#[test]
fn shown_statistics_change_only_on_a_tick() {
    let mut f = fixture();
    let now = Utc::now();

    f.read.delivered(now, Some(stats_at(Some(now))));
    let before = f.read.shown.clone();
    let changed = f.read.tick(now, true);

    assert!(before.is_none());
    assert!(changed);
    assert_eq!(f.read.shown.as_ref().unwrap().stats.received, 5);
}

#[test]
fn a_hub_with_no_message_for_thirty_seconds_is_quiet() {
    let mut f = fixture();
    let now = Utc::now();
    f.read
        .delivered(now, Some(stats_at(Some(now - TimeDelta::seconds(31)))));

    f.read.tick(now, true);

    assert!(f.read.shown.as_ref().unwrap().quiet);
}

#[test]
fn no_delivery_for_three_intervals_marks_the_feed_stale_until_one_arrives() {
    let mut f = fixture();
    let start = Utc::now();
    f.read.delivered(start, None);

    let fresh = f.read.tick(start + TimeDelta::seconds(2), true);
    let stale_tick = f.read.tick(start + TimeDelta::seconds(4), true);
    let stale = f.read.feed_stale;
    f.read.delivered(start + TimeDelta::seconds(5), None);
    f.read.tick(start + TimeDelta::seconds(5), true);

    assert!(!fresh);
    assert!(stale && stale_tick);
    assert!(!f.read.feed_stale);
}

#[test]
fn copied_feedback_shows_on_its_message_and_clears_after_its_time() {
    let mut f = fixture();
    f.arrive_and_flush(&["a", "b"]);
    f.apply(ReadInput::Select("p0-s0".to_owned()));
    let now = Utc::now();
    f.apply(ReadInput::Inspector(InspectorInput::Copied {
        kind: FeedbackKind::CopiedBody,
        now,
    }));
    let feedback = f.read.inspector.feedback.clone().unwrap();
    let other = f.read.rows.by_key("p0-s1").unwrap().message.key.clone();

    f.read.tick(now + inspector::FEEDBACK_FOR, true);

    assert!(feedback.shows(
        &f.read.inspector.selected.as_ref().unwrap().key,
        FeedbackKind::CopiedBody
    ));
    assert!(!feedback.shows(&other, FeedbackKind::CopiedBody));
    assert!(f.read.inspector.feedback.is_none());
}

#[test]
fn a_copy_failure_is_a_danger_toast() {
    let mut f = fixture();

    let out = f.apply(ReadInput::Inspector(InspectorInput::CopyFailed(
        "no json".to_owned(),
    )));

    assert!(matches!(out.toasts.as_slice(), [(ToastKind::Danger, text)] if text == "no json"));
}

fn picking(f: &mut Fixture) -> MessageKey {
    f.arrive_and_flush(&["a"]);
    f.apply(ReadInput::Select("p0-s0".to_owned()));
    f.apply(ReadInput::Inspector(InspectorInput::ExportPressed));
    f.read.inspector.selected.as_ref().unwrap().key.clone()
}

fn answer(key: &MessageKey, answer: Result<Option<PathBuf>, String>) -> ReadInput {
    ReadInput::Inspector(InspectorInput::ExportAnswered {
        owner: 1,
        key: key.clone(),
        answer,
    })
}

#[test]
fn export_pressed_opens_the_dialog_for_the_selection() {
    let mut f = fixture();

    let key = picking(&mut f);

    assert!(
        matches!(&f.read.inspector.export, ExportPhase::Picking { owner: 1, key: k, .. } if *k == key)
    );
}

#[test]
fn a_picked_destination_starts_one_export_with_single_file_options() {
    let mut f = fixture();
    let key = picking(&mut f);
    f.read.inspector.format = ExportFormat::Json;
    let destination = Path::new("out").join("one.json");

    let out = f.apply(answer(&key, Ok(Some(destination.clone()))));

    let [ReadEffect::ExportOne(request)] = out.effects.as_slice() else {
        panic!("expected an export, got {:?}", out.effects);
    };
    assert_eq!(request.destination, destination);
    assert_eq!(request.options.format, ExportFormat::Json);
    assert!(!request.options.skip_already_exported && !request.options.condense_output);
    assert!(f.read.export_running());
}

#[test]
fn a_cancelled_dialog_returns_to_idle() {
    let mut f = fixture();
    let key = picking(&mut f);

    f.apply(answer(&key, Ok(None)));

    assert!(f.read.inspector.export.is_idle());
}

#[test]
fn a_failed_dialog_returns_to_idle_with_a_toast() {
    let mut f = fixture();
    let key = picking(&mut f);

    let out = f.apply(answer(&key, Err("no portal".to_owned())));

    assert!(f.read.inspector.export.is_idle());
    assert_eq!(out.toasts.len(), 1);
}

#[test]
fn an_answer_for_another_message_is_dropped() {
    let mut f = fixture();
    let key = picking(&mut f);
    let other = MessageKey {
        sequence_number: 99,
        ..key.clone()
    };

    let out = f.apply(answer(&other, Ok(Some(PathBuf::from("x.txt")))));

    assert!(out.effects.is_empty());
    assert!(matches!(&f.read.inspector.export, ExportPhase::Picking { key: k, .. } if *k == key));
}

fn running(f: &mut Fixture) -> MessageKey {
    let key = picking(f);
    f.apply(answer(&key, Ok(Some(PathBuf::from("x.txt")))));
    key
}

fn done(owner: u64, key: &MessageKey, result: Result<(), String>) -> ReadInput {
    ReadInput::Inspector(InspectorInput::ExportDone {
        owner,
        key: key.clone(),
        result,
        now: Utc::now(),
    })
}

#[test]
fn a_finished_export_shows_exported_on_its_message() {
    let mut f = fixture();
    let key = running(&mut f);

    f.apply(done(1, &key, Ok(())));

    assert!(f.read.inspector.export.is_idle());
    assert!(
        f.read
            .inspector
            .feedback
            .as_ref()
            .unwrap()
            .shows(&key, FeedbackKind::Exported)
    );
}

#[test]
fn a_failed_export_toasts_even_for_another_session() {
    let mut f = fixture();
    let key = running(&mut f);

    let out = f.apply(done(7, &key, Err("disk full".to_owned())));

    assert_eq!(out.toasts.len(), 1);
    assert!(f.read.inspector.feedback.is_none());
    assert!(f.read.export_running());
}

#[test]
fn a_running_export_survives_the_session_ending_and_a_new_one() {
    let mut f = fixture();
    running(&mut f);

    f.read.end_session();
    f.read.start_session(2, Duration::from_secs(1));

    assert!(f.read.export_running());
}

#[test]
fn an_open_dialog_is_dropped_when_the_session_ends() {
    let mut f = fixture();
    picking(&mut f);

    f.read.end_session();

    assert!(f.read.inspector.export.is_idle());
}

#[test]
fn a_file_picked_after_the_session_ended_is_not_written_and_says_so() {
    let mut f = fixture();
    let key = picking(&mut f);
    f.read.end_session();

    let out = f.apply(answer(&key, Ok(Some(PathBuf::from("x.txt")))));

    assert!(out.effects.is_empty());
    assert!(matches!(out.toasts.as_slice(), [(ToastKind::Info, _)]));
}

#[test]
fn an_ended_session_clears_its_live_numbers() {
    let mut f = fixture();
    let now = Utc::now();
    f.read.delivered(now, Some(stats_at(Some(now))));
    f.read.tick(now, true);
    f.read.feed_stale = true;

    f.read.end_session();

    assert!(f.read.stats.is_none() && f.read.shown.is_none());
    assert!(!f.read.feed_stale && f.read.last_delivery_at.is_none());
}

#[test]
fn the_read_state_is_a_row_source_over_the_displayed_or_matching_rows() {
    let mut f = fixture();
    f.arrive_and_flush(&["bacon", "eggs"]);

    assert_eq!(RowSource::count(&f.read), 2);
    assert_eq!(RowSource::row(&f.read, 1).unwrap().key_str, "p0-s1");
    assert_eq!(RowSource::epoch(&f.read), f.read.rows.epoch);
}

fn log_line(text: &str) -> LogLine {
    LogLine {
        at: Utc::now(),
        kind: LogKind::Error,
        partition: Some("2".to_owned()),
        text: text.to_owned(),
    }
}

#[test]
fn the_session_log_caps_and_counts_unseen_lines_while_its_popover_is_closed() {
    let mut f = fixture();
    for index in 0..SESSION_LOG_CAP + 5 {
        f.read.log(log_line(&index.to_string()), false);
    }
    f.read.log(log_line("seen"), true);

    assert_eq!(f.read.session_log.len(), SESSION_LOG_CAP);
    assert_eq!(f.read.session_log.front().unwrap().text, "6");
    assert_eq!(f.read.session_log_unseen, (SESSION_LOG_CAP + 5) as u32);
    f.read.log_seen();
    assert_eq!(f.read.session_log_unseen, 0);
}

#[test]
fn a_new_session_clears_the_list_and_keeps_the_rules_and_generation() {
    let mut f = fixture();
    f.arrive_and_flush(&["bacon"]);
    f.search("bacon", false);
    f.read.log(log_line("x"), false);
    f.read.rules_applied = CaptureFilters {
        time: None,
        text: Some(TextFilter::Contains(vec!["bacon".to_owned()])),
    };
    let generation = f.read.generation.load(Ordering::Acquire);

    f.read.start_session(2, Duration::from_secs(3));

    assert!(f.read.rows.displayed().is_empty());
    assert!(matches!(f.read.search, SearchState::Off));
    assert!(f.read.session_log.is_empty());
    assert_eq!(rules::rule_count(&f.read.rules_applied), 1);
    assert!(f.read.generation.load(Ordering::Acquire) > generation);
    assert_eq!(f.read.feedback_interval, Duration::from_secs(3));
}

#[test]
fn an_ended_session_drops_its_rows_and_keeps_its_log() {
    let mut f = fixture();
    f.arrive_and_flush(&["a"]);
    f.apply(ReadInput::Select("p0-s0".to_owned()));
    f.read.search_edit = TextEdit::new("a");
    f.read.log(log_line("x"), false);

    f.read.end_session();

    assert!(f.read.rows.displayed().is_empty());
    assert!(f.read.inspector.selected.is_none());
    assert!(f.read.search_edit.text.is_empty());
    assert_eq!(f.read.session_log.len(), 1);
}

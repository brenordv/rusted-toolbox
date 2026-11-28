use super::*;
use crate::config::{ConfigNote, ConfigStatus, RecipientEntry, SealConfig};
use crate::design::{BOOK_MIN_ROWS, WINDOW_MIN, WINDOW_SIZE};
use crate::state::keys::{Generated, KeygenStep};
use crate::state::notes::RunningJob;
use crate::test_support::{Recording, recording_state, state_with_book};
use crate::worker::{JobEvent, JobOutcome, JobRequest};
use common_cli::app_logger::FileLogging;
use common_gui_rune::parts::list_height;
use common_gui_rune::test_harness::{
    self, a11y_node, a11y_nodes, answer_dialog, bottom, clipboard_text, focus_is_inside, has_text,
    is_disabled, press, press_key, rect, runtime_with_keys, settle, use_memory_clipboard,
};
use rune::{FileDialogKind, Key, Runtime};
use seal_crypto::keys::{generate_identity, public_key_of};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::rc::Rc;

fn fresh_key() -> String {
    public_key_of(&generate_identity())
}

fn keys_window_of(state: AppState, size: (f32, f32)) -> (Runtime<AppState>, Rc<Recording>) {
    let mut state = state;
    state.tab = Tab::Keys;
    let (state, services) = recording_state(state);
    (runtime_with_keys(state, view, window_key, size), services)
}

/// A window on the Keys tab whose book holds `labels`.
fn keys_window(labels: &[&str], size: (f32, f32)) -> (Runtime<AppState>, Rc<Recording>) {
    keys_window_of(state_with_book(labels), size)
}

fn deliver(rt: &mut Runtime<AppState>, input: Input) {
    test_harness::deliver(rt, move |s, tx| dispatch(s, tx, input));
}

fn node(rt: &Runtime<AppState>, key: &str) -> Value {
    let tree = rt.accessibility_json();
    a11y_node(&tree, key)
        .cloned()
        .unwrap_or_else(|| panic!("no accessibility node keyed {key}"))
}

fn has_state(node: &Value, state: &str) -> bool {
    node["states"]
        .as_array()
        .is_some_and(|states| states.iter().any(|named| named == state))
}

fn live_keys(rt: &Runtime<AppState>) -> Vec<String> {
    let tree = rt.accessibility_json();
    a11y_nodes(&tree)
        .into_iter()
        .filter(|node| has_state(node, "live-polite"))
        .filter_map(|node| node["key"].as_str().map(str::to_owned))
        .collect()
}

fn keygen_exists(rt: &mut Runtime<AppState>, path: &str) {
    rt.state.work = Work::Running(RunningJob::Keygen);
    deliver(
        rt,
        Input::Job(JobEvent::Finished(JobOutcome::KeygenExists {
            path: PathBuf::from(path),
        })),
    );
}

fn keygen_done(rt: &mut Runtime<AppState>, key: &str, path: &str) {
    rt.state.work = Work::Running(RunningJob::Keygen);
    deliver(
        rt,
        Input::Job(JobEvent::Finished(JobOutcome::Keygen {
            public_key: key.to_owned(),
            path: PathBuf::from(path),
        })),
    );
}

/// Whether the element keyed `inner` lies inside the one keyed `outer`, top to bottom.
fn inside(rt: &Runtime<AppState>, inner: &str, outer: &str) -> bool {
    let [_, top, _, _] = rect(rt, outer);
    rect(rt, inner)[1] >= top - 0.5 && bottom(rt, inner) <= bottom(rt, outer) + 0.5
}

#[test]
fn h1_generate_save_the_file_then_add_the_key_to_the_book() {
    let (mut rt, services) = keys_window(&[], WINDOW_SIZE);
    let key = fresh_key();

    press(&mut rt, GENERATE_KEY);
    answer_dialog(&mut rt, FileDialogKind::Save, Some(Path::new("laptop.txt")));
    let runs = services.take_runs();
    keygen_done(&mut rt, &key, "laptop.txt");
    let shown = has_text(&rt, &key);
    press(&mut rt, ADD_GENERATED_KEY);

    assert!(matches!(
        runs.as_slice(),
        [JobRequest::Keygen { path, replace: false }] if path == Path::new("laptop.txt")
    ));
    assert!(shown);
    assert_eq!(rt.state.book.entries()[0].label, "laptop");
    assert_eq!(services.saves.borrow().len(), 1);
    assert!(is_disabled(&rt, ADD_GENERATED_KEY));
}

#[test]
fn h1b_a_free_stem_joins_the_book_in_view_and_a_taken_one_fills_the_form() {
    let labels: Vec<String> = (1..=30).map(|n| format!("person-{n:02}")).collect();
    let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
    let (mut rt, _) = keys_window(&labels, WINDOW_SIZE);
    let key = fresh_key();
    keygen_done(&mut rt, &key, "laptop.txt");

    press(&mut rt, ADD_GENERATED_KEY);
    let entry = rt.state.book.entries().last().cloned().unwrap();
    let row_in_view = inside(&rt, &book_row_key(&entry), BOOK_KEY);

    assert_eq!(entry.label, "laptop");
    assert!(row_in_view);
    assert!(has_text(&rt, "Added laptop to the recipient book."));
    assert!(is_disabled(&rt, ADD_GENERATED_KEY));

    let (mut taken, _) = keys_window(&["laptop"], WINDOW_SIZE);
    let other = fresh_key();
    keygen_done(&mut taken, &other, "laptop.txt");
    press(&mut taken, ADD_GENERATED_KEY);

    assert_eq!(taken.state.book.entries().len(), 1);
    assert_eq!(taken.state.keys.label.text, "laptop");
    assert_eq!(taken.state.keys.key.text, other.as_str());
    assert!(focus_is_inside(&taken, LABEL_KEY));
    assert!(has_text(&taken, "laptop is already in the book."));
}

#[test]
fn h2_the_confirm_shows_without_moving_focus_and_replace_replaces() {
    let (mut rt, services) = keys_window(&[], WINDOW_SIZE);
    press(&mut rt, LABEL_KEY);

    keygen_exists(&mut rt, "id.txt");
    let focus_kept = focus_is_inside(&rt, LABEL_KEY);
    let shown = has_text(
        &rt,
        "id.txt already exists. Replacing it destroys the key in it.",
    );
    press(&mut rt, REPLACE_KEY);

    assert!(focus_kept);
    assert!(shown);
    assert!(matches!(
        services.take_runs().as_slice(),
        [JobRequest::Keygen { replace: true, .. }]
    ));
}

#[test]
fn h2_replace_waits_with_the_rest_while_another_job_runs() {
    let (mut rt, services) = keys_window(&[], WINDOW_SIZE);
    keygen_exists(&mut rt, "id.txt");

    test_harness::deliver(&mut rt, |s, _| {
        s.work = Work::Running(RunningJob::Text(seal_crypto::Direction::Encrypt));
        true
    });
    press(&mut rt, REPLACE_KEY);

    assert!(is_disabled(&rt, REPLACE_KEY));
    assert_eq!(services.run_count(), 0);
    assert!(matches!(
        rt.state.keys.step,
        KeygenStep::ConfirmReplace { .. }
    ));
}

#[test]
fn h2_return_in_the_label_field_never_replaces() {
    let (mut rt, services) = keys_window(&[], WINDOW_SIZE);
    keygen_exists(&mut rt, "id.txt");
    press(&mut rt, LABEL_KEY);

    press_key(&mut rt, Key::Enter);

    assert_eq!(services.run_count(), 0);
    assert_eq!(rt.state.keys.step, KeygenStep::Idle);
}

#[test]
fn h3_copy_on_a_book_row_copies_the_whole_key_and_says_so() {
    let (mut rt, _) = keys_window(&["Alice"], WINDOW_SIZE);
    let clipboard = use_memory_clipboard(&mut rt);
    let entry = rt.state.book.entries()[0].clone();

    press(&mut rt, &copy_entry_key(entry.id.number()));

    assert_eq!(
        clipboard_text(&clipboard).as_deref(),
        Some(entry.public_key.as_str())
    );
    assert!(has_text(&rt, "Copied the public key."));
}

/// Whether a wheel over the top of the Keys tab's body moves the book.
fn body_scrolls(rt: &mut Runtime<AppState>) -> bool {
    let before = rect(rt, BOOK_KEY)[1];
    let [x, y, width, _] = rect(rt, KEYS_SCROLL_KEY);
    rt.wheel(x + width / 2.0, y + 4.0, -240.0);
    settle(rt);
    rect(rt, BOOK_KEY)[1] < before
}

#[test]
fn h4_at_rest_with_thirty_entries_the_tab_fits_and_the_book_shows_three_rows_or_more() {
    let labels: Vec<String> = (1..=30).map(|n| format!("person-{n:02}")).collect();
    let labels: Vec<&str> = labels.iter().map(String::as_str).collect();

    for size in [WINDOW_MIN, WINDOW_SIZE] {
        let (mut rt, _) = keys_window(&labels, size);
        let book = rect(&rt, BOOK_KEY)[3];

        assert!(!body_scrolls(&mut rt), "the body scrolls at {size:?}");
        assert!(
            book >= list_height(BOOK_MIN_ROWS) - 0.5,
            "{book} at {size:?}"
        );
    }
}

#[test]
fn h4_with_everything_showing_the_outer_scroll_reaches_every_box() {
    let long_folder = PathBuf::from("logs".repeat(75)).join("seal-gui.log");
    for size in [WINDOW_MIN, WINDOW_SIZE] {
        let mut state = state_with_book(&["Alice"]);
        state.file_logging = FileLogging::Writing(long_folder.clone());
        state.config_status = ConfigStatus::Failed("disk full".to_owned());
        let (mut rt, _) = keys_window_of(state, size);
        keygen_done(&mut rt, &fresh_key(), "laptop.txt");
        keygen_exists(&mut rt, "laptop.txt");
        rt.state.work = Work::Stopped;
        settle(&mut rt);
        let add_in_view = inside(&rt, ADD_RECIPIENT_KEY, KEYS_SCROLL_KEY);

        for _ in 0..12 {
            let [x, y, width, _] = rect(&rt, KEYS_SCROLL_KEY);
            rt.wheel(x + width / 2.0, y + 4.0, -400.0);
            settle(&mut rt);
        }

        assert!(add_in_view, "Add recipient is out of view at {size:?}");
        for key in [
            REPLACE_KEY,
            CANCEL_KEY,
            COPY_GENERATED_KEY,
            ADD_GENERATED_KEY,
            LOG_FOLDER_KEY,
        ] {
            assert!(
                inside(&rt, key, KEYS_SCROLL_KEY),
                "{key} is out of reach at {size:?}"
            );
        }
    }
}

#[test]
fn h5_the_confirm_is_the_one_new_live_region_and_row_buttons_carry_the_clean_label() {
    let (mut rt, _) = keys_window(&["Al\u{7}ice"], WINDOW_SIZE);
    let before = live_keys(&rt);

    keygen_exists(&mut rt, "id.txt");
    let mut added: Vec<String> = live_keys(&rt)
        .into_iter()
        .filter(|key| !before.contains(key))
        .collect();
    added.sort();
    let confirm_line = node(&rt, CONFIRM_KEY);
    let number = rt.state.book.entries()[0].id.number();
    press(&mut rt, &delete_key(number));
    let confirm = node(&rt, &delete_key(number));
    let copy = node(&rt, &copy_entry_key(number));

    assert_eq!(added, [CONFIRM_KEY.to_owned()]);
    assert_eq!(confirm_line["role"], "status");
    assert_eq!(confirm["name"], "Confirm delete Al ice");
    assert_eq!(copy["name"], "Copy Al ice");
}

#[test]
fn h6_a_label_from_the_config_shows_with_its_control_characters_as_spaces() {
    let (rt, _) = keys_window(&["Bob\u{1b}[31m"], WINDOW_SIZE);

    assert!(has_text(&rt, "Bob [31m"));
    assert!(!has_text(&rt, "\u{1b}"));
}

/// Replace asks for focus on Generate too (state test C4), but while the keygen it starts runs,
/// Generate is a Loading button, which takes no focus; the finished keygen lands it there.
#[test]
fn h7_cancel_and_a_finished_keygen_land_on_generate() {
    let (mut rt, _) = keys_window(&[], WINDOW_SIZE);
    keygen_exists(&mut rt, "id.txt");
    press(&mut rt, CANCEL_KEY);
    let after_cancel = focus_is_inside(&rt, GENERATE_KEY);
    keygen_exists(&mut rt, "id.txt");
    press(&mut rt, REPLACE_KEY);
    press(&mut rt, LABEL_KEY);

    keygen_done(&mut rt, &fresh_key(), "id.txt");

    assert!(after_cancel);
    assert!(focus_is_inside(&rt, GENERATE_KEY));
}

#[test]
fn h7_confirm_delete_lands_on_the_next_row_and_an_add_stays_in_the_label_field() {
    let (mut rt, _) = keys_window(&["Alice", "Bob"], WINDOW_SIZE);
    let (alice, bob) = (
        rt.state.book.entries()[0].id.number(),
        rt.state.book.entries()[1].id.number(),
    );

    press(&mut rt, &delete_key(alice));
    press(&mut rt, &delete_key(alice));
    let on_next = focus_is_inside(&rt, &delete_key(bob));
    rt.state.keys.label = rune::TextEdit::new("Carol");
    rt.state.keys.key = rune::TextEdit::new(fresh_key().as_str());
    press(&mut rt, ADD_RECIPIENT_KEY);

    assert!(on_next);
    assert!(focus_is_inside(&rt, LABEL_KEY));
    assert_eq!(rt.state.book.entries().len(), 2);
}

#[test]
fn h8_each_config_caption_shows_on_every_tab() {
    for (status, words) in [
        (
            ConfigStatus::Failed("disk full".to_owned()),
            "Settings could not be saved: disk full",
        ),
        (
            ConfigStatus::Refused(ConfigNote::Foreign),
            "Another tool's config.json is in seal's settings folder",
        ),
        (
            ConfigStatus::Refused(ConfigNote::Unreadable),
            "config.json could not be read",
        ),
        (
            ConfigStatus::NoLocation,
            "No settings folder was found; changes are not saved.",
        ),
    ] {
        let mut state = state_with_book(&[]);
        state.config_status = status;
        let (mut rt, _) = keys_window_of(state, WINDOW_SIZE);
        for tab in Tab::ALL {
            deliver(&mut rt, Input::TabPicked(tab));
            assert!(has_text(&rt, words), "{words} on {tab:?}");
        }
    }
}

#[test]
fn the_about_group_names_the_log_folder_or_why_there_is_none() {
    let folder = PathBuf::from("logs-here");
    let mut writing = state_with_book(&[]);
    writing.file_logging = FileLogging::Writing(folder.join("seal-gui.log"));
    let mut failed = state_with_book(&[]);
    failed.file_logging = FileLogging::Failed {
        path: folder.join("seal-gui.log"),
        error: "denied".to_owned(),
    };

    let (writing, _) = keys_window_of(writing, WINDOW_SIZE);
    let (off, _) = keys_window(&[], WINDOW_SIZE);
    let (failed, _) = keys_window_of(failed, WINDOW_SIZE);

    assert!(has_text(&writing, "logs-here"));
    assert!(has_text(&off, "File logging is off."));
    assert!(has_text(
        &failed,
        "The log file could not be opened: denied"
    ));
}

#[test]
fn a_saved_book_round_trips_through_the_snapshot() {
    let key = fresh_key();
    let config = SealConfig {
        recipients: vec![RecipientEntry {
            label: "Alice".to_owned(),
            public_key: key,
        }],
        last_identity_file: Some(PathBuf::from("id.txt")),
        last_output_folder: Some(PathBuf::from("safe")),
        overwrite_existing: true,
        ..SealConfig::default()
    };

    let state = AppState::new(
        (config.clone(), ConfigStatus::Clean),
        FileLogging::Off,
        None,
        None,
    );

    assert_eq!(state.snapshot(), config);
}

#[test]
fn the_generated_key_row_shows_the_key_and_its_copy_names_itself() {
    let (mut rt, _) = keys_window(&[], WINDOW_SIZE);
    test_harness::deliver(&mut rt, |s, _| {
        s.keys.generated = Some(Generated {
            public_key: "age1shown".to_owned(),
            path: PathBuf::from("id.txt"),
        });
        true
    });

    assert!(has_text(&rt, "age1shown"));
    assert_eq!(node(&rt, COPY_GENERATED_KEY)["name"], "Copy the public key");
}

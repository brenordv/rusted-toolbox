use super::parts::list_height;
use super::*;
use crate::design::{FILES_MIN_ROWS, WINDOW_MIN, WINDOW_SIZE};
use crate::state::files::{PathKind, RowStatus};
use crate::state::recipients::Picker;
use crate::test_support::{Recording, recording_state, state_with_book};
use crate::worker::{FileResult, JobEvent, JobOutcome, JobRequest};
use common_gui_rune::test_harness::{
    self, FRAME, a11y_node, answer_dialog, bottom, command, focus_is_inside, has_text, is_disabled,
    press, rect, runtime_with_keys, settle,
};
use rune::{FileDialogKind, Key, Runtime};
use serde_json::Value;
use shared_crypto::Direction;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// A window on the Files tab whose book holds `book`.
fn files_window(book: &[&str], size: (f32, f32)) -> (Runtime<AppState>, Rc<Recording>) {
    let mut state = state_with_book(book);
    state.tab = Tab::Files;
    let (state, services) = recording_state(state);
    (runtime_with_keys(state, view, window_key, size), services)
}

fn deliver(rt: &mut Runtime<AppState>, input: Input) {
    test_harness::deliver(rt, move |s, tx| dispatch(s, tx, input));
}

fn add_rows(rt: &mut Runtime<AppState>, names: &[&str]) {
    let paths = names.iter().map(PathBuf::from).collect();
    deliver(rt, Input::Files(FilesInput::AddPaths(paths)));
}

fn centre(rt: &Runtime<AppState>, key: &str) -> (f32, f32) {
    let [x, y, width, height] = rect(rt, key);
    (x + width / 2.0, y + height / 2.0)
}

fn node(rt: &Runtime<AppState>, key: &str) -> Value {
    let tree = rt.accessibility_json();
    a11y_node(&tree, key)
        .cloned()
        .unwrap_or_else(|| panic!("no accessibility node keyed {key}"))
}

fn row_paths(rt: &Runtime<AppState>) -> Vec<String> {
    rt.state
        .files
        .rows
        .iter()
        .map(|row| row.path.display().to_string())
        .collect()
}

/// Whether a wheel over the top of the Files tab's body moves the Mode row.
fn body_scrolls(rt: &mut Runtime<AppState>) -> bool {
    let before = rect(rt, MODE_KEY)[1];
    let [x, y, width, _] = rect(rt, FILES_SCROLL_KEY);
    rt.wheel(x + width / 2.0, y + 4.0, -240.0);
    settle(rt);
    rect(rt, MODE_KEY)[1] < before
}

fn finished_files(done: usize) -> Input {
    Input::Job(JobEvent::Finished(JobOutcome::Files {
        direction: Direction::Encrypt,
        done,
        skipped: 0,
        failed: 0,
    }))
}

#[test]
fn g1_drop_three_files_check_a_recipient_and_mod_enter_seals_them() {
    let (mut rt, services) = files_window(&["Alice"], WINDOW_MIN);
    let names = ["a.txt", "b.txt", "c.txt"];
    let at = centre(&rt, FILE_LIST_KEY);

    rt.drop_files(names.iter().map(PathBuf::from).collect(), Some(at));
    settle(&mut rt);
    let alice = rt.state.book.entries()[0].id;
    press(&mut rt, &recipient_key(Picker::Files, alice));
    command(&mut rt, Key::Enter);
    let runs = services.take_runs();
    for name in names {
        deliver(
            &mut rt,
            Input::Job(JobEvent::FileProgress {
                path: PathBuf::from(name),
            }),
        );
        deliver(
            &mut rt,
            Input::Job(JobEvent::FileFinished {
                path: PathBuf::from(name),
                result: FileResult::Done(10),
            }),
        );
    }
    deliver(&mut rt, finished_files(3));

    assert!(matches!(
        runs.as_slice(),
        [JobRequest::EncryptFiles { files, recipients, .. }]
            if files == &names.map(PathBuf::from) && recipients.len() == 1
    ));
    assert!(
        rt.state
            .files
            .rows
            .iter()
            .all(|row| row.status == RowStatus::Done(10))
    );
    assert!(has_text(&rt, "3 sealed, 0 skipped, 0 failed."));
}

#[test]
fn g2_add_files_opens_one_dialog_and_its_answer_is_inspected() {
    let (mut rt, services) = files_window(&[], WINDOW_SIZE);

    press(&mut rt, ADD_FILES_KEY);
    let title = rt
        .file_dialog()
        .map(|(_, options)| options.title.clone())
        .unwrap_or_default();
    let disabled_while_open = is_disabled(&rt, ADD_FILES_KEY);
    answer_dialog(&mut rt, FileDialogKind::Open, Some(&PathBuf::from("a.txt")));
    let picked = services.take_checks();
    press(&mut rt, ADD_FILES_KEY);
    answer_dialog(&mut rt, FileDialogKind::Open, None);

    assert_eq!(title, "Add files");
    assert!(disabled_while_open);
    assert_eq!(picked, [("inspect", 1), ("probe", 1)]);
    assert!(services.take_checks().is_empty());
    assert_eq!(row_paths(&rt), ["a.txt"]);
    assert!(!is_disabled(&rt, ADD_FILES_KEY));
}

#[test]
fn g3_a_drag_lights_the_list_until_it_leaves_and_a_drop_elsewhere_adds_nothing() {
    let (mut rt, _) = files_window(&[], WINDOW_SIZE);
    let at = centre(&rt, FILE_LIST_KEY);

    rt.drag_files(vec![PathBuf::from("a.txt")], Some(at));
    settle(&mut rt);
    let lit = rt.state.files.drop_hover;
    rt.drag_files_leave();
    settle(&mut rt);
    let left = rt.state.files.drop_hover;
    rt.drag_files(vec![PathBuf::from("a.txt")], Some(at));
    settle(&mut rt);
    rt.drop_files(vec![PathBuf::from("a.txt")], Some(at));
    settle(&mut rt);
    let after_drop = rt.state.files.drop_hover;
    command(&mut rt, Key::Command('1'));
    let window_centre = (WINDOW_SIZE.0 / 2.0, WINDOW_SIZE.1 / 2.0);
    rt.drop_files(vec![PathBuf::from("b.txt")], Some(window_centre));
    settle(&mut rt);

    assert!(lit);
    assert!(!left);
    assert!(!after_drop);
    assert_eq!(row_paths(&rt), ["a.txt"]);
}

#[test]
fn g4_remove_lands_focus_on_the_row_that_takes_its_place_then_the_one_before_then_add() {
    let (mut rt, _) = files_window(&[], WINDOW_SIZE);
    add_rows(&mut rt, &["a.txt", "b.txt", "c.txt"]);

    press(&mut rt, &remove_key(1));
    let after_middle = (row_paths(&rt), focus_is_inside(&rt, &remove_key(1)));
    press(&mut rt, &remove_key(1));
    let after_last = (row_paths(&rt), focus_is_inside(&rt, &remove_key(0)));
    press(&mut rt, &remove_key(0));

    assert_eq!(
        after_middle,
        (vec!["a.txt".to_owned(), "c.txt".to_owned()], true)
    );
    assert_eq!(after_last, (vec!["a.txt".to_owned()], true));
    assert!(rt.state.files.rows.is_empty());
    assert!(focus_is_inside(&rt, ADD_FILES_KEY));
}

#[test]
fn g5_at_rest_with_twelve_rows_the_tab_fits_and_the_list_shows_three_rows_or_more() {
    let names: Vec<String> = (1..=12).map(|n| format!("file-{n}.txt")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();

    for size in [WINDOW_MIN, WINDOW_SIZE] {
        let (mut rt, _) = files_window(&["Alice", "Bob", "Carol"], size);
        add_rows(&mut rt, &names);

        let list = rect(&rt, FILE_LIST_KEY)[3];
        let submit_bottom = bottom(&rt, FILES_SUBMIT_KEY);

        assert!(!body_scrolls(&mut rt), "the body scrolls at {size:?}");
        assert!(
            list >= list_height(FILES_MIN_ROWS) - 0.5,
            "{list} at {size:?}"
        );
        assert!(submit_bottom <= size.1, "{submit_bottom} at {size:?}");
    }
}

#[test]
fn g5_long_paths_and_the_stopped_caption_keep_every_control_reachable() {
    let names: Vec<String> = (1..=3)
        .map(|n| format!("{}-{n}.txt", "long".repeat(75)))
        .collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();

    for size in [WINDOW_MIN, WINDOW_SIZE] {
        let (mut rt, _) = files_window(&["Alice"], size);
        add_rows(&mut rt, &names);
        deliver(&mut rt, Input::Stopped);
        let [_, viewport_top, _, _] = rect(&rt, FILES_SCROLL_KEY);
        let viewport_bottom = bottom(&rt, FILES_SCROLL_KEY);

        let add_in_view = rect(&rt, ADD_FILES_KEY)[1] >= viewport_top
            && bottom(&rt, ADD_FILES_KEY) <= viewport_bottom;
        let submit_bottom = bottom(&rt, FILES_SUBMIT_KEY);
        for _ in 0..10 {
            let [x, y, width, _] = rect(&rt, FILES_SCROLL_KEY);
            rt.wheel(x + width / 2.0, y + 4.0, -400.0);
            settle(&mut rt);
        }

        assert!(add_in_view, "Add files is out of view at {size:?}");
        assert!(submit_bottom <= size.1, "{submit_bottom} at {size:?}");
        assert!(
            bottom(&rt, OUTPUT_CHOICE_KEY) <= viewport_bottom + 0.5,
            "the output choice is out of reach at {size:?}"
        );
    }
}

#[test]
fn g6_rows_remove_buttons_the_list_and_the_summary_are_named_for_assistive_technology() {
    let (mut rt, _) = files_window(&["Alice"], WINDOW_SIZE);
    add_rows(&mut rt, &["a.txt"]);
    deliver(
        &mut rt,
        Input::Files(FilesInput::ModePicked(Direction::Decrypt)),
    );

    let row = node(&rt, &file_row_key(0));
    let remove = node(&rt, &remove_key(0));
    let list = node(&rt, FILE_LIST_KEY);
    deliver(
        &mut rt,
        Input::Files(FilesInput::ModePicked(Direction::Encrypt)),
    );
    let alice = rt.state.book.entries()[0].id;
    press(&mut rt, &recipient_key(Picker::Files, alice));
    press(&mut rt, FILES_SUBMIT_KEY);
    let running = node(&rt, FILES_STATUS_KEY);
    deliver(&mut rt, finished_files(1));
    let summary = node(&rt, FILES_STATUS_KEY);

    assert_eq!(row["name"], "Invalid: a.txt");
    assert_eq!(row["description"], "Unseal needs a .age file.");
    assert_eq!(remove["name"], "Remove a.txt");
    assert_eq!(list["role"], "group");
    assert_eq!(list["name"], "Files");
    assert_eq!(running["name"], "Sealing files");
    assert_eq!(summary["role"], "status");
    assert_eq!(summary["name"], "1 sealed, 0 skipped, 0 failed.");
    assert!(
        summary["states"]
            .as_array()
            .is_some_and(|states| states.iter().any(|state| state == "live-polite"))
    );
}

#[test]
fn g7_a_right_to_left_override_in_a_file_name_shows_as_a_space() {
    let (mut rt, _) = files_window(&[], WINDOW_SIZE);

    add_rows(&mut rt, &["invoice\u{202E}fdp.exe"]);
    let row = node(&rt, &file_row_key(0));

    assert!(has_text(&rt, "invoice fdp.exe"));
    assert!(!has_text(&rt, "\u{202E}"));
    assert_eq!(row["name"], "Pending: invoice fdp.exe");
}

#[test]
fn the_skip_warning_shows_at_rest_and_not_while_the_batch_runs() {
    let (mut rt, services) = files_window(&["Alice"], WINDOW_SIZE);
    services
        .existing
        .borrow_mut()
        .insert(PathBuf::from("a.txt.age"));
    add_rows(&mut rt, &["a.txt"]);
    let at_rest = has_text(&rt, MSG_OUTPUT_EXISTS);
    let alice = rt.state.book.entries()[0].id;
    press(&mut rt, &recipient_key(Picker::Files, alice));

    press(&mut rt, FILES_SUBMIT_KEY);

    assert!(at_rest);
    assert!(rt.state.running_on(Tab::Files));
    assert!(!has_text(&rt, MSG_OUTPUT_EXISTS));
}

#[test]
fn f14_one_dispatch_adds_the_rows_and_marks_their_existing_outputs() {
    let (mut rt, services) = files_window(&[], WINDOW_SIZE);
    services
        .existing
        .borrow_mut()
        .insert(PathBuf::from("a.txt.age"));
    services
        .kinds
        .borrow_mut()
        .insert(PathBuf::from("photos"), PathKind::Folder);

    add_rows(&mut rt, &["a.txt", "b.txt", "photos"]);

    assert_eq!(services.take_checks(), [("inspect", 3), ("probe", 2)]);
    assert_eq!(
        rt.state
            .files
            .rows
            .iter()
            .map(|row| row.output_exists)
            .collect::<Vec<_>>(),
        [true, false]
    );
    assert!(has_text(&rt, MSG_OUTPUT_EXISTS));
    assert!(has_text(
        &rt,
        "Folders cannot be added; add the files inside instead."
    ));
}

/// The median, 95th percentile, and slowest of `frames`.
fn spread(mut frames: Vec<Duration>) -> String {
    frames.sort();
    let at = |share: f64| frames[((frames.len() - 1) as f64 * share) as usize];
    format!(
        "median {:?}, p95 {:?}, max {:?}",
        at(0.5),
        at(0.95),
        at(1.0)
    )
}

/// G9, the large-batch measurement: lists of 200, 500, and 2,000 rows, the first 200 files'
/// progress and finish delivered as the worker posts them with one frame each, then frames
/// with nothing new. Run in a release build:
/// `cargo test -p seal-gui --release --lib -- --ignored --nocapture g9`.
#[test]
#[ignore = "a release-build measurement, recorded in the PR"]
fn g9_a_large_batch_measured_frame_by_frame() {
    const MEASURED_FILES: usize = 200;
    for size in [200, 500, 2_000] {
        let (mut rt, _) = files_window(&["Alice"], WINDOW_SIZE);
        let names: Vec<PathBuf> = (0..size)
            .map(|n| PathBuf::from(format!("batch/file-{n:04}.txt")))
            .collect();
        deliver(&mut rt, Input::Files(FilesInput::AddPaths(names.clone())));
        let alice = rt.state.book.entries()[0].id;
        deliver(&mut rt, Input::Files(FilesInput::RecipientToggled(alice)));
        deliver(&mut rt, Input::Files(FilesInput::StartPressed));

        let mut busy = Vec::with_capacity(2 * MEASURED_FILES);
        for path in names.into_iter().take(MEASURED_FILES) {
            for event in [
                JobEvent::FileProgress { path: path.clone() },
                JobEvent::FileFinished {
                    path,
                    result: FileResult::Done(1),
                },
            ] {
                rt.update_if(move |s, tx| dispatch(s, tx, Input::Job(event)));
                let started = Instant::now();
                rt.frame(FRAME);
                busy.push(started.elapsed());
            }
        }
        let idle = (0..30)
            .map(|_| {
                let started = Instant::now();
                rt.frame(FRAME);
                started.elapsed()
            })
            .collect();

        eprintln!(
            "g9 {size} rows: after an event {}; idle {}",
            spread(busy),
            spread(idle)
        );
        assert_eq!(
            rt.state
                .files
                .rows
                .iter()
                .filter(|row| row.status == RowStatus::Done(1))
                .count(),
            MEASURED_FILES
        );
    }
}

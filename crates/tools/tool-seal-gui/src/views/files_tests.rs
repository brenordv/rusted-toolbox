use super::parts::list_height;
use super::*;
use crate::design::{FILES_MIN_ROWS, WINDOW_MIN, WINDOW_SIZE};
use crate::state::files::{PathKind, RowStatus};
use crate::state::recipients::Picker;
use crate::test_support::{Recording, recording_state, state_with_book};
use crate::worker::{FileResult, JobEvent, JobOutcome, JobRequest};
use common_gui_rune::test_harness::{
    self, FRAME, a11y_node, answer_dialog, bottom, command, focus_is_inside, focused_key, has_key,
    has_text, is_disabled, press, rect, runtime_at, runtime_with_keys, settle, tooltip_of,
};
use common_gui_rune::tokens::{CONTROL_HEIGHT, SPACE_XS};
use rstest::rstest;
use rune::{FileDialogKind, Key, KeyEvent, Mods, Runtime};
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

/// `count` paths of `folder/file-NNNN.txt`.
fn batch(folder: &str, count: usize) -> Vec<PathBuf> {
    (0..count)
        .map(|n| PathBuf::from(format!("{folder}/file-{n:04}.txt")))
        .collect()
}

/// How many file rows are built, out of however many the list holds.
fn built_rows(rt: &Runtime<AppState>) -> usize {
    rt.elements()
        .iter()
        .filter(|element| {
            !element.exiting
                && element
                    .key
                    .as_deref()
                    .is_some_and(|key| key.starts_with("seal-files-row-"))
        })
        .count()
}

/// Scrolls the file list to its end, then settles.
fn scroll_list_to_end(rt: &mut Runtime<AppState>) {
    let viewport = format!("{FILE_LIST_KEY}-scroll");
    test_harness::deliver(rt, move |_, tx| {
        tx.scroll_to_bottom(viewport);
        true
    });
}

/// Presses Tab, a few frames apart, until focus is on the element keyed `key`; how many presses
/// that took, or `None` when `limit` presses did not get there.
fn tab_until(rt: &mut Runtime<AppState>, key: &str, limit: usize) -> Option<usize> {
    (1..=limit).find(|_| {
        rt.key(KeyEvent {
            key: Key::Tab,
            mods: Mods::default(),
        });
        for _ in 0..3 {
            rt.frame(FRAME);
        }
        focused_key(rt).as_deref() == Some(key)
    })
}

#[test]
fn a_path_that_fits_shows_no_tooltip() {
    let (mut rt, _) = files_window(&[], WINDOW_SIZE);
    add_rows(&mut rt, &["a.txt"]);

    let tooltip = tooltip_of(&mut rt, &file_path_key(0));

    assert_eq!(tooltip, None);
}

#[test]
fn a_long_path_takes_one_line_and_its_tooltip_is_the_whole_cleaned_path() {
    let folders = "deep/".repeat(60);
    let name = format!("{folders}invoice\u{202E}fdp\u{1E}x\u{1F}y.exe");
    let (mut rt, _) = files_window(&[], WINDOW_SIZE);
    add_rows(&mut rt, &[name.as_str()]);

    let height = rect(&rt, &file_row_key(0))[3];
    let tooltip =
        tooltip_of(&mut rt, &file_path_key(0)).map(|(text, _)| text.replace('\u{200B}', ""));

    assert!((height - CONTROL_HEIGHT).abs() < 0.5, "{height}");
    assert_eq!(tooltip, Some(format!("{folders}invoice fdp x y.exe")));
}

#[rstest]
#[case::one_long_name("x".repeat(300))]
#[case::backslash_path(format!("C:\\{}\\file.txt", ["folder"; 40].join("\\")))]
#[case::slash_path(format!("/{}/file.txt", ["folder"; 40].join("/")))]
fn the_tooltip_of_a_long_path_of_any_shape_stays_inside_the_window(#[case] name: String) {
    let (mut rt, _) = files_window(&[], WINDOW_MIN);
    add_rows(&mut rt, &[name.as_str()]);

    let [x, y, width, height] = tooltip_of(&mut rt, &file_path_key(0))
        .map(|(_, at)| at)
        .unwrap_or_else(|| panic!("no tooltip for {name}"));

    assert!(x >= 0.0 && x + width <= WINDOW_MIN.0, "{x} + {width}");
    assert!(y >= 0.0 && y + height <= WINDOW_MIN.1, "{y} + {height}");
}

#[rstest]
#[case::letters("abc", "a\u{200B}b\u{200B}c")]
#[case::one_letter("a", "a")]
#[case::empty("", "")]
#[case::combining_mark("e\u{301}x", "e\u{301}\u{200B}x")]
#[case::emoji_sequence("\u{1F468}\u{200D}\u{1F469}a", "\u{1F468}\u{200D}\u{1F469}\u{200B}a")]
fn breakable_puts_a_zero_width_space_between_graphemes(#[case] text: &str, #[case] expected: &str) {
    assert_eq!(files_tab::breakable(text), expected);
}

#[test]
fn dropping_two_thousand_files_builds_only_the_rows_around_the_view() {
    let (mut rt, _) = files_window(&[], WINDOW_SIZE);
    let names = batch("batch", 2_000);
    let at = centre(&rt, FILE_LIST_KEY);

    rt.drop_files(names, Some(at));
    rt.frame(FRAME);
    let first_frame = built_rows(&rt);
    settle(&mut rt);
    let settled = built_rows(&rt);

    assert_eq!(rt.state.files.rows.len(), 2_000);
    assert!(
        first_frame < 200,
        "{first_frame} rows built in the first frame"
    );
    assert!(settled < 200, "{settled} rows built once settled");
}

#[test]
fn the_last_of_two_thousand_rows_shows_in_the_list_once_it_scrolls_to_the_end() {
    let (mut rt, _) = files_window(&[], WINDOW_SIZE);
    deliver(
        &mut rt,
        Input::Files(FilesInput::AddPaths(batch("batch", 2_000))),
    );

    scroll_list_to_end(&mut rt);
    let last = file_row_key(1_999);

    assert!(has_key(&rt, &last));
    let [_, top, _, _] = rect(&rt, &last);
    let [_, list_top, _, _] = rect(&rt, &format!("{FILE_LIST_KEY}-scroll"));
    assert!(top >= list_top, "{top} above {list_top}");
    assert!(
        bottom(&rt, &last) <= bottom(&rt, &format!("{FILE_LIST_KEY}-scroll")) + 0.5,
        "the last row is below the list"
    );
}

#[test]
fn remove_on_the_last_of_two_thousand_rows_removes_that_file() {
    let (mut rt, _) = files_window(&[], WINDOW_SIZE);
    deliver(
        &mut rt,
        Input::Files(FilesInput::AddPaths(batch("batch", 2_000))),
    );
    scroll_list_to_end(&mut rt);

    press(&mut rt, &remove_key(1_999));

    let paths = row_paths(&rt);
    assert_eq!(paths.len(), 1_999);
    assert_eq!(
        paths.last().map(String::as_str),
        Some("batch/file-1998.txt")
    );
}

#[test]
fn tab_from_the_first_remove_reaches_the_remove_of_the_last_row() {
    let (mut rt, _) = files_window(&[], WINDOW_MIN);
    deliver(
        &mut rt,
        Input::Files(FilesInput::AddPaths(batch("batch", 150))),
    );
    test_harness::deliver(&mut rt, |_, tx| {
        tx.focus(remove_key(0), true);
        true
    });

    let presses = tab_until(&mut rt, &remove_key(149), 400);

    assert!(presses.is_some(), "focus stopped at {:?}", focused_key(&rt));
}

/// The row at index 0 of a list with no rows, on its own.
fn row_past_the_end(s: &AppState, _: &mut Cx<AppState>) -> El {
    col()
        .w_full()
        .child(files_tab::file_row_at(s, true, 0).key("past-the-end"))
}

#[test]
fn a_row_index_past_the_rows_builds_an_empty_row_of_a_rows_height() {
    let (state, _) = recording_state(state_with_book(&[]));

    let rt = runtime_at(state, row_past_the_end, WINDOW_SIZE);

    assert!((rect(&rt, "past-the-end")[3] - CONTROL_HEIGHT).abs() < 0.5);
    assert!(!has_key(&rt, &remove_key(0)));
    assert!(!has_key(&rt, &file_path_key(0)));
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

/// How long one frame takes.
fn timed_frame(rt: &mut Runtime<AppState>) -> Duration {
    let started = Instant::now();
    rt.frame(FRAME);
    started.elapsed()
}

/// G9 with every row cut: 2,000 paths of 149 characters at the minimum size. Times the frame
/// after the drop, the first 200 files' progress and finish as the worker posts them, frames
/// with nothing new, and a scroll pass of one list viewport per frame to the end. Run in a
/// release build: `cargo test -p seal-gui --release --lib -- --ignored --nocapture g9`.
#[test]
#[ignore = "a release-build measurement, recorded in the PR"]
fn g9_long_paths_at_the_minimum_size_measured_frame_by_frame() {
    const MEASURED_FILES: usize = 200;
    let folder = format!("srv{}", "/projects/archive/2026".repeat(6));
    let names = batch(&folder, 2_000);
    let (mut rt, _) = files_window(&["Alice"], WINDOW_MIN);
    let at = centre(&rt, FILE_LIST_KEY);
    let started = Instant::now();
    rt.drop_files(names.clone(), Some(at));
    let handled = started.elapsed();
    let dropped = timed_frame(&mut rt);
    let built = built_rows(&rt);
    settle(&mut rt);
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
            busy.push(timed_frame(&mut rt));
        }
    }
    let idle = (0..30).map(|_| timed_frame(&mut rt)).collect();
    let [x, y, width, height] = rect(&rt, &format!("{FILE_LIST_KEY}-scroll"));
    let steps = (2_000.0 * (CONTROL_HEIGHT + SPACE_XS) / height).ceil() as usize + 2;
    let scroll = (0..steps)
        .map(|_| {
            rt.wheel(x + width / 2.0, y + height / 2.0, -height);
            timed_frame(&mut rt)
        })
        .collect();

    eprintln!(
        "g9 2,000 rows of {} characters at {WINDOW_MIN:?}: the drop's handler {handled:?}, the \
         frame after it {dropped:?} ({built} rows built); after an event {}; scrolling {}; \
         idle {}",
        folder.len() + "/file-0000.txt".len(),
        spread(busy),
        spread(scroll),
        spread(idle)
    );
    assert!(
        has_key(&rt, &file_row_key(1_999)),
        "the scroll pass did not reach the end"
    );
}

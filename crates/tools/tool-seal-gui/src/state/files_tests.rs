use super::*;
use crate::test_support::{Recording, recording_state, state_with_book};
use files::{MSG_ROW_NEEDS_AGE, OutputChoice, Skipped};
use rstest::rstest;
use std::rc::Rc;

fn files(s: &mut AppState, input: FilesInput) -> Outcome {
    s.apply(Input::Files(input))
}

fn regular(path: &str) -> (PathBuf, PathKind) {
    (PathBuf::from(path), PathKind::RegularFile { size: 10 })
}

fn paths(s: &AppState) -> Vec<String> {
    s.files
        .rows
        .iter()
        .map(|row| row.path.display().to_string())
        .collect()
}

fn status_of(s: &AppState, path: &str) -> RowStatus {
    s.files
        .rows
        .iter()
        .find(|row| row.path == Path::new(path))
        .map(|row| row.status.clone())
        .unwrap_or_else(|| panic!("no row for {path}"))
}

/// The Files tab showing `rows`, Alice checked, ready to seal.
fn ready_batch(rows: &[&str]) -> (AppState, Rc<Recording>) {
    let (mut s, services) = recording_state(state_with_book(&["Alice"]));
    s.tab = Tab::Files;
    s.apply(Input::Inspected(
        rows.iter().map(|path| regular(path)).collect(),
    ));
    let alice = s.book.entries()[0].id;
    s.files.picker.toggle(alice);
    (s, services)
}

fn running_files(s: &mut AppState, total: usize) {
    s.work = Work::Running(RunningJob::Files {
        direction: Direction::Encrypt,
        finished: 0,
        total,
    });
}

fn progress(path: &str) -> Input {
    Input::Job(JobEvent::FileProgress {
        path: PathBuf::from(path),
    })
}

fn file_finished(path: &str, result: FileResult) -> Input {
    Input::Job(JobEvent::FileFinished {
        path: PathBuf::from(path),
        result,
    })
}

fn batch_finished(done: usize, skipped: usize, failed: usize) -> Input {
    Input::Job(JobEvent::Finished(JobOutcome::Files {
        direction: Direction::Encrypt,
        done,
        skipped,
        failed,
    }))
}

fn probes(out: &Outcome) -> usize {
    out.effects
        .iter()
        .filter(|effect| matches!(effect, Effect::ProbeOutputs(_)))
        .count()
}

#[test]
fn f1_an_intake_adds_regular_files_once_and_says_why_the_first_other_path_was_left_out() {
    let mut s = state_with_book(&[]);

    s.apply(Input::Inspected(vec![
        regular("a.txt"),
        (PathBuf::from("photos"), PathKind::Folder),
        regular("b.txt"),
        (PathBuf::from("pipe"), PathKind::Other),
    ]));
    let note = s.note(Tab::Files).cloned();
    s.apply(Input::Inspected(vec![regular("a.txt")]));
    s.apply(Input::Inspected(vec![regular("c.txt"), regular("c.txt")]));

    assert_eq!(paths(&s), ["a.txt", "b.txt", "c.txt"]);
    assert_eq!(note, Some(Note::Skipped(Skipped::Folder)));
}

#[test]
fn f1_a_pipe_alone_is_named_as_not_a_regular_file() {
    let mut s = state_with_book(&[]);

    s.apply(Input::Inspected(vec![(
        PathBuf::from("pipe"),
        PathKind::Other,
    )]));

    assert!(s.files.rows.is_empty());
    assert_eq!(
        s.note(Tab::Files),
        Some(&Note::Skipped(Skipped::NotRegular))
    );
}

#[test]
fn f2_an_unreadable_path_is_added_with_size_zero() {
    let mut s = state_with_book(&[]);

    s.apply(Input::Inspected(vec![(
        PathBuf::from("gone.txt"),
        PathKind::Unreadable,
    )]));

    assert_eq!(s.files.rows[0].size, 0);
    assert_eq!(s.files.rows[0].status, RowStatus::Pending);
}

#[test]
fn f3_a_row_added_during_a_batch_stays_pending_through_its_end() {
    let (mut s, _) = ready_batch(&["a.txt"]);
    files(&mut s, FilesInput::StartPressed);

    s.apply(Input::Inspected(vec![regular("late.txt")]));
    s.apply(file_finished("a.txt", FileResult::Done(20)));
    s.apply(batch_finished(1, 0, 0));

    assert_eq!(status_of(&s, "late.txt"), RowStatus::Pending);
    assert_eq!(
        s.note(Tab::Files),
        Some(&Note::Summary {
            direction: Direction::Encrypt,
            done: 1,
            skipped: 0,
            failed: 0,
        })
    );
}

#[test]
fn f3_an_intake_lands_after_the_worker_stopped() {
    let mut s = state_with_book(&[]);
    s.apply(Input::Stopped);

    s.apply(Input::Inspected(vec![regular("a.txt")]));

    assert_eq!(paths(&s), ["a.txt"]);
}

#[test]
fn f4_remove_takes_its_row_only_and_clear_takes_them_all() {
    let (mut s, _) = ready_batch(&["a.txt", "b.txt", "c.txt"]);

    files(&mut s, FilesInput::RemoveRow(PathBuf::from("b.txt")));
    let after_remove = paths(&s);
    files(&mut s, FilesInput::ClearList);

    assert_eq!(after_remove, ["a.txt", "c.txt"]);
    assert!(s.files.rows.is_empty());
}

#[rstest]
#[case::remove(FilesInput::RemoveRow(PathBuf::from("a.txt")))]
#[case::clear(FilesInput::ClearList)]
fn f4_remove_and_clear_wait_for_a_batch_but_not_for_a_text_job(#[case] input: FilesInput) {
    let (mut s, _) = ready_batch(&["a.txt"]);
    running_files(&mut s, 1);
    let during_batch = files(&mut s, input.clone());
    let kept = paths(&s);
    s.work = Work::Running(RunningJob::Text(Direction::Encrypt));

    files(&mut s, input);

    assert!(!during_batch.changed);
    assert_eq!(kept, ["a.txt"]);
    assert!(s.files.rows.is_empty());
}

#[test]
fn f5_a_mode_switch_remarks_the_rows_and_waits_for_a_batch() {
    let (mut s, _) = ready_batch(&["plain.txt", "sealed.AGE"]);

    files(&mut s, FilesInput::ModePicked(Direction::Decrypt));
    let unseal = (status_of(&s, "plain.txt"), status_of(&s, "sealed.AGE"));
    running_files(&mut s, 2);
    let refused = files(&mut s, FilesInput::ModePicked(Direction::Encrypt));

    assert_eq!(
        unseal,
        (RowStatus::Invalid(MSG_ROW_NEEDS_AGE), RowStatus::Pending)
    );
    assert!(!refused.changed);
    assert_eq!(s.files.mode, Direction::Decrypt);
}

#[test]
fn f6_a_start_names_each_missing_thing_in_order_and_moves_focus_to_it() {
    let (mut s, services) = recording_state(state_with_book(&["Alice"]));
    s.tab = Tab::Files;
    let alice = s.book.entries()[0].id;
    let mut seen = Vec::new();
    let mut start = |s: &mut AppState| {
        let out = files(s, FilesInput::StartPressed);
        let note = match s.note(Tab::Files) {
            Some(Note::Missing(missing)) => Some(missing.clone()),
            Some(_) | None => None,
        };
        seen.push((note, out.focus));
    };

    start(&mut s);
    s.apply(Input::Inspected(vec![regular("a.txt"), regular("b.txt")]));
    files(&mut s, FilesInput::ModePicked(Direction::Decrypt));
    start(&mut s);
    files(&mut s, FilesInput::ModePicked(Direction::Encrypt));
    files(&mut s, FilesInput::OutputPicked(OutputChoice::Folder));
    start(&mut s);
    files(&mut s, FilesInput::OutputPicked(OutputChoice::NextToInput));
    start(&mut s);
    files(&mut s, FilesInput::ClearList);
    s.apply(Input::Inspected(vec![regular("a.txt.age")]));
    files(&mut s, FilesInput::ModePicked(Direction::Decrypt));
    start(&mut s);

    assert_eq!(
        seen,
        [
            (Some(Missing::Files), Some(Focus::AddFiles)),
            (Some(Missing::ValidRows(0)), Some(Focus::RemoveRow(0))),
            (Some(Missing::Folder), Some(Focus::ChooseFolder)),
            (
                Some(Missing::Recipients),
                Some(Focus::Recipient(Picker::Files, alice))
            ),
            (
                Some(Missing::Identity),
                Some(Focus::ChooseIdentity(Picker::Files))
            ),
        ]
    );
    assert_eq!(services.run_count(), 0);
    assert_eq!(s.work, Work::Idle);
}

#[test]
fn f7_a_valid_start_readies_every_row_and_runs_the_batch() {
    let (mut s, services) = ready_batch(&["a.txt", "b.txt", "c.txt"]);
    s.files.rows[0].status = RowStatus::Done(5);
    s.files.rows[1].status = RowStatus::Skipped;
    s.files.rows[2].status = RowStatus::Failed("denied".to_owned());

    let out = files(&mut s, FilesInput::StartPressed);
    for effect in out.effects {
        if let Effect::Run(request) = effect {
            s.services.as_ref().unwrap().run(request).unwrap();
        }
    }

    assert!(
        s.files
            .rows
            .iter()
            .all(|row| row.status == RowStatus::Pending)
    );
    assert_eq!(
        s.work,
        Work::Running(RunningJob::Files {
            direction: Direction::Encrypt,
            finished: 0,
            total: 3,
        })
    );
    assert!(matches!(
        services.take_runs().as_slice(),
        [JobRequest::EncryptFiles { files, overwrite: false, out_dir: None, .. }]
            if files.len() == 3
    ));
}

#[test]
fn f8_a_batch_moves_rows_by_path_counts_finished_files_and_ends_in_a_summary() {
    let (mut s, _) = ready_batch(&["a.txt", "b.txt", "c.txt"]);
    files(&mut s, FilesInput::StartPressed);

    s.apply(progress("a.txt"));
    let a_running = status_of(&s, "a.txt");
    s.apply(file_finished("a.txt", FileResult::Done(30)));
    let phrase = s.status(Tab::Files).map(|(_, words)| words);
    s.apply(file_finished(
        "b.txt",
        FileResult::Failed("denied".to_owned()),
    ));
    s.apply(progress("c.txt"));
    s.apply(file_finished("c.txt", FileResult::Skipped));
    s.apply(batch_finished(1, 1, 1));

    assert_eq!(a_running, RowStatus::Running);
    assert_eq!(phrase.as_deref(), Some("Sealing 2 of 3"));
    assert_eq!(status_of(&s, "a.txt"), RowStatus::Done(30));
    assert_eq!(
        status_of(&s, "b.txt"),
        RowStatus::Failed("denied".to_owned())
    );
    assert_eq!(status_of(&s, "c.txt"), RowStatus::Skipped);
    assert_eq!(s.work, Work::Idle);
    assert_eq!(
        s.status(Tab::Files),
        Some((Tone::Danger, "1 sealed, 1 skipped, 1 failed.".to_owned()))
    );
}

#[rstest]
#[case::clean(0, 0, Tone::Muted)]
#[case::skipped(1, 0, Tone::Warning)]
#[case::failed(1, 1, Tone::Danger)]
fn f8_the_summary_takes_the_tone_of_its_worst_result(
    #[case] skipped: usize,
    #[case] failed: usize,
    #[case] tone: Tone,
) {
    let (mut s, _) = ready_batch(&["a.txt"]);
    files(&mut s, FilesInput::StartPressed);

    s.apply(batch_finished(1, skipped, failed));

    assert_eq!(s.status(Tab::Files).map(|(shown, _)| shown), Some(tone));
}

#[test]
fn f8_the_count_stops_at_the_batch_size() {
    let job = RunningJob::Files {
        direction: Direction::Decrypt,
        finished: 3,
        total: 3,
    };

    assert_eq!(job.phrase(), "Unsealing 3 of 3");
}

#[rstest]
#[case::idle(Work::Idle)]
#[case::stopped(Work::Stopped)]
#[case::text(Work::Running(RunningJob::Text(Direction::Encrypt)))]
fn f9_file_events_outside_a_batch_change_nothing(#[case] work: Work) {
    let (mut s, _) = ready_batch(&["a.txt"]);
    s.work = work;

    let moved = s.apply(progress("a.txt"));
    let finished = s.apply(file_finished("a.txt", FileResult::Done(1)));

    assert!(!moved.changed && !finished.changed);
    assert_eq!(status_of(&s, "a.txt"), RowStatus::Pending);
    assert_eq!(s.work, work);
}

#[test]
fn f9_a_file_event_for_a_path_that_is_not_a_row_changes_nothing() {
    let (mut s, _) = ready_batch(&["a.txt"]);
    files(&mut s, FilesInput::StartPressed);
    let before = s.work;

    let moved = s.apply(progress("other.txt"));
    let finished = s.apply(file_finished("other.txt", FileResult::Done(1)));

    assert!(!moved.changed && !finished.changed);
    assert_eq!(s.work, before);
}

#[rstest]
#[case::failed(Input::Job(JobEvent::Failed("the worker broke".to_owned())))]
#[case::stopped(Input::Stopped)]
fn f10_a_failure_or_a_stop_returns_running_rows_to_pending(#[case] ending: Input) {
    let (mut s, _) = ready_batch(&["a.txt", "b.txt"]);
    files(&mut s, FilesInput::StartPressed);
    s.apply(file_finished("a.txt", FileResult::Done(1)));
    s.apply(progress("b.txt"));

    s.apply(ending);

    assert_eq!(status_of(&s, "a.txt"), RowStatus::Done(1));
    assert_eq!(status_of(&s, "b.txt"), RowStatus::Pending);
}

#[test]
fn f11_outputs_are_probed_at_each_of_the_five_places() {
    let (mut s, _) = ready_batch(&[]);

    let added = s.apply(Input::Inspected(vec![regular("a.txt")]));
    let mode = files(&mut s, FilesInput::ModePicked(Direction::Decrypt));
    let choice = files(&mut s, FilesInput::OutputPicked(OutputChoice::Folder));
    s.apply(Input::DialogOpened(DialogPurpose::OutputFolder));
    let folder = s.apply(Input::DialogAnswered(
        DialogPurpose::OutputFolder,
        Ok(Some(PathBuf::from("safe"))),
    ));
    files(&mut s, FilesInput::ModePicked(Direction::Encrypt));
    files(&mut s, FilesInput::StartPressed);
    let finished = s.apply(batch_finished(1, 0, 0));

    assert_eq!(
        [added, mode, choice, folder, finished].map(|out| probes(&out)),
        [1, 1, 1, 1, 1]
    );
}

#[test]
fn f11_a_probe_answer_marks_rows_by_path_and_ignores_removed_ones() {
    let (mut s, _) = ready_batch(&["a.txt", "b.txt"]);
    files(&mut s, FilesInput::RemoveRow(PathBuf::from("b.txt")));

    s.apply(Input::OutputsProbed(vec![
        (PathBuf::from("a.txt"), true),
        (PathBuf::from("b.txt"), true),
    ]));

    assert!(s.files.rows[0].output_exists);
    assert_eq!(paths(&s), ["a.txt"]);
}

#[test]
fn f12_a_tab_switch_clears_the_drop_highlight() {
    let (mut s, _) = ready_batch(&[]);
    files(&mut s, FilesInput::DropHover(true));
    let hovered = s.files.drop_hover;

    s.apply(Input::TabPicked(Tab::Text));

    assert!(hovered);
    assert!(!s.files.drop_hover);
}

#[test]
fn f12_a_drag_over_the_list_clears_no_note() {
    let (mut s, _) = ready_batch(&[]);
    s.apply(Input::Inspected(vec![(
        PathBuf::from("photos"),
        PathKind::Folder,
    )]));

    files(&mut s, FilesInput::DropHover(true));

    assert_eq!(s.note(Tab::Files), Some(&Note::Skipped(Skipped::Folder)));
}

#[test]
fn f13_other_tabs_count_the_batch_and_then_point_at_its_failure() {
    let (mut s, _) = ready_batch(&["a.txt", "b.txt", "c.txt"]);
    files(&mut s, FilesInput::StartPressed);
    s.apply(file_finished("a.txt", FileResult::Done(1)));

    let text_during = s.status(Tab::Text).map(|(_, words)| words);
    let keys_during = s.status(Tab::Keys).map(|(_, words)| words);
    let spoken = (s.status_spoken(Tab::Text), s.status_spoken(Tab::Files));
    s.apply(Input::Job(JobEvent::Failed("disk full".to_owned())));
    let pointer = s.status(Tab::Text).map(|(_, words)| words);
    s.apply(Input::Text(TextInput::InputEdited));
    let text_after_input = s.note(Tab::Text).cloned();
    let keys_before_visit = s.note(Tab::Keys).cloned();
    s.apply(Input::TabPicked(Tab::Files));

    assert_eq!(text_during.as_deref(), Some("Files: sealing 2 of 3"));
    assert_eq!(keys_during.as_deref(), Some("Files: sealing 2 of 3"));
    assert_eq!(
        spoken,
        (
            Some("Files: sealing files".to_owned()),
            Some("Sealing files".to_owned())
        )
    );
    assert_eq!(pointer.as_deref(), Some("The Files run failed. See Files."));
    assert_eq!(text_after_input, None);
    assert_eq!(keys_before_visit, Some(Note::OtherTabFailed(Tab::Files)));
    assert_eq!(s.note(Tab::Keys), None);
}

#[test]
fn f15_the_add_file_dialog_marks_itself_and_its_answer_inspects_the_path() {
    let (mut s, _) = ready_batch(&[]);

    let opened = s.apply(Input::DialogOpened(DialogPurpose::AddFile));
    let blocked = s.apply(Input::DialogOpened(DialogPurpose::OutputFolder));
    let marked = s.dialog;
    let answered = s.apply(Input::DialogAnswered(
        DialogPurpose::AddFile,
        Ok(Some(PathBuf::from("a.txt"))),
    ));

    assert!(matches!(
        opened.effects.as_slice(),
        [Effect::OpenDialog(DialogPurpose::AddFile)]
    ));
    assert!(blocked.effects.is_empty());
    assert_eq!(marked, Some(DialogPurpose::AddFile));
    assert_eq!(s.dialog, None);
    assert!(matches!(
        answered.effects.as_slice(),
        [Effect::Inspect(paths)] if paths == &[PathBuf::from("a.txt")]
    ));
}

#[test]
fn f15_the_folder_dialog_sets_the_folder_and_picks_it() {
    let (mut s, _) = ready_batch(&[]);
    s.apply(Input::DialogOpened(DialogPurpose::OutputFolder));

    s.apply(Input::DialogAnswered(
        DialogPurpose::OutputFolder,
        Ok(Some(PathBuf::from("safe"))),
    ));

    assert_eq!(s.dialog, None);
    assert_eq!(s.files.out_dir, Some(PathBuf::from("safe")));
    assert_eq!(s.files.out_choice, OutputChoice::Folder);
}

#[test]
fn f15_a_cancelled_folder_dialog_keeps_the_choice() {
    let (mut s, _) = ready_batch(&[]);
    s.apply(Input::DialogOpened(DialogPurpose::OutputFolder));

    let out = s.apply(Input::DialogAnswered(DialogPurpose::OutputFolder, Ok(None)));

    assert_eq!(s.dialog, None);
    assert_eq!(s.files.out_choice, OutputChoice::NextToInput);
    assert!(out.effects.is_empty());
}

#[test]
fn an_identity_chosen_on_one_tab_settles_both_tabs_missing_identity() {
    let (mut s, _) = ready_batch(&["a.txt.age"]);
    files(&mut s, FilesInput::ModePicked(Direction::Decrypt));
    files(&mut s, FilesInput::StartPressed);
    s.apply(Input::TabPicked(Tab::Text));
    s.text.input = rune::TextEdit::new("-----BEGIN AGE ENCRYPTED FILE-----");
    s.apply(Input::Text(TextInput::DirectionPicked(Direction::Decrypt)));
    s.apply(Input::Text(TextInput::StartPressed));
    let both_missing = (s.note(Tab::Text).cloned(), s.note(Tab::Files).cloned());

    s.apply(Input::DialogOpened(DialogPurpose::Identity(Picker::Text)));
    s.apply(Input::DialogAnswered(
        DialogPurpose::Identity(Picker::Text),
        Ok(Some(PathBuf::from("id.txt"))),
    ));

    assert_eq!(
        both_missing,
        (
            Some(Note::Missing(Missing::Identity)),
            Some(Note::Missing(Missing::Identity))
        )
    );
    assert_eq!(s.note(Tab::Text), None);
    assert_eq!(s.note(Tab::Files), None);
}

#[test]
fn a_second_finish_for_a_row_changes_nothing_and_is_not_counted() {
    let (mut s, _) = ready_batch(&["a.txt", "b.txt"]);
    files(&mut s, FilesInput::StartPressed);
    s.apply(file_finished("a.txt", FileResult::Done(5)));
    let after_first = s.work;

    let again = s.apply(file_finished(
        "a.txt",
        FileResult::Failed("late".to_owned()),
    ));

    assert!(!again.changed);
    assert_eq!(s.work, after_first);
    assert_eq!(status_of(&s, "a.txt"), RowStatus::Done(5));
}

#[test]
fn folder_with_no_folder_chosen_clears_every_output_mark() {
    let (mut s, _) = ready_batch(&["a.txt"]);
    s.apply(Input::OutputsProbed(vec![(PathBuf::from("a.txt"), true)]));

    let out = files(&mut s, FilesInput::OutputPicked(OutputChoice::Folder));
    for effect in out.effects {
        if let Effect::ProbeOutputs(pairs) = effect {
            assert!(pairs.is_empty());
            s.apply(Input::OutputsProbed(Vec::new()));
        }
    }

    assert!(!s.files.rows[0].output_exists);
}

#[test]
fn a_note_set_during_its_own_tabs_run_gives_way_to_the_summary() {
    let (mut s, _) = ready_batch(&["a.txt"]);
    files(&mut s, FilesInput::StartPressed);

    s.apply(Input::Inspected(vec![(
        PathBuf::from("photos"),
        PathKind::Folder,
    )]));
    let during = s.status(Tab::Files).map(|(_, words)| words);
    s.apply(batch_finished(1, 0, 0));

    assert_eq!(during.as_deref(), Some("Sealing 1 of 1"));
    assert!(matches!(s.note(Tab::Files), Some(Note::Summary { .. })));
}

#[test]
fn a_files_input_clears_the_files_note_only() {
    let (mut s, _) = ready_batch(&[]);
    s.notes_mut_for_tests()
        .set(Tab::Files, Note::Skipped(Skipped::Folder));
    s.notes_mut_for_tests().set(Tab::Text, Note::NotText);

    files(&mut s, FilesInput::OverwriteToggled);

    assert_eq!(s.note(Tab::Files), None);
    assert_eq!(s.note(Tab::Text), Some(&Note::NotText));
}

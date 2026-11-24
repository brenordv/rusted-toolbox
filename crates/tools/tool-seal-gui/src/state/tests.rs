use super::*;
use crate::config::RecipientEntry;
use crate::test_support::{Recording, recording_state, state_with_book};
use rstest::rstest;
use rune::{FileDialogError, TextEdit};
use shared_crypto::Direction;
use shared_crypto::keys::{generate_identity, public_key_of};

fn fresh_key() -> String {
    public_key_of(&generate_identity())
}

fn first_id(s: &AppState) -> EntryId {
    s.book.entries()[0].id
}

/// A Text tab ready to seal "hello" to its first book entry.
fn ready_to_seal() -> (AppState, std::rc::Rc<Recording>) {
    let (mut s, services) = recording_state(state_with_book(&["Alice"]));
    s.text.input = TextEdit::new("hello");
    let alice = first_id(&s);
    s.text.picker.toggle(alice);
    (s, services)
}

fn start(s: &mut AppState) -> Outcome {
    s.apply(Input::Text(TextInput::StartPressed))
}

fn running_text(s: &mut AppState) {
    s.work = Work::Running(RunningJob::Text(Direction::Encrypt));
}

fn finished_text(text: &str) -> Input {
    Input::Job(JobEvent::Finished(JobOutcome::Text(text.to_owned())))
}

#[test]
fn w1_a_valid_text_start_runs_one_seal_job() {
    let (mut s, _) = ready_to_seal();

    let out = start(&mut s);

    assert_eq!(s.work, Work::Running(RunningJob::Text(Direction::Encrypt)));
    assert!(matches!(
        out.effects.as_slice(),
        [Effect::Run(JobRequest::EncryptText { text, recipients })]
            if text == "hello" && recipients.len() == 1
    ));
}

#[test]
fn w2_a_blank_input_stays_idle_and_says_what_is_missing() {
    let (mut s, _) = ready_to_seal();
    s.text.input = TextEdit::new("  \n ");

    let out = start(&mut s);

    assert_eq!(s.work, Work::Idle);
    assert!(out.effects.is_empty());
    assert_eq!(s.note(Tab::Text), Some(&Note::Missing(Missing::Input)));
    assert_eq!(out.focus, Some(Focus::TextInput));
}

#[test]
fn w3_a_start_while_running_starts_nothing() {
    let (mut s, _) = ready_to_seal();
    running_text(&mut s);

    let out = start(&mut s);

    assert!(out.effects.is_empty());
    assert_eq!(s.work, Work::Running(RunningJob::Text(Direction::Encrypt)));
}

#[test]
fn w4_the_text_answer_returns_to_idle_and_fills_the_output() {
    let (mut s, _) = ready_to_seal();
    running_text(&mut s);

    let out = s.apply(finished_text("-----BEGIN AGE ENCRYPTED FILE-----"));

    assert!(out.changed);
    assert_eq!(s.work, Work::Idle);
    assert_eq!(s.text.output, "-----BEGIN AGE ENCRYPTED FILE-----");
}

#[test]
fn w5_an_answer_of_another_kind_is_ignored() {
    let (mut s, _) = ready_to_seal();
    running_text(&mut s);

    let out = s.apply(Input::Job(JobEvent::Finished(JobOutcome::KeygenExists {
        path: PathBuf::from("id.txt"),
    })));

    assert!(!out.changed);
    assert_eq!(s.work, Work::Running(RunningJob::Text(Direction::Encrypt)));
}

#[test]
fn w6_a_failure_returns_to_idle_clears_the_output_and_says_why() {
    let (mut s, _) = ready_to_seal();
    s.text.output = "partial".to_owned();
    running_text(&mut s);

    s.apply(Input::Job(JobEvent::Failed(
        "the key file is damaged".to_owned(),
    )));

    assert_eq!(s.work, Work::Idle);
    assert!(s.text.output.is_empty());
    assert_eq!(
        s.note(Tab::Text),
        Some(&Note::Failure("the key file is damaged".to_owned()))
    );
    assert_eq!(s.note(Tab::Files), Some(&Note::OtherTabFailed(Tab::Text)));
    assert_eq!(s.note(Tab::Keys), Some(&Note::OtherTabFailed(Tab::Text)));
}

#[rstest]
#[case::finished(finished_text("late"))]
#[case::failed(Input::Job(JobEvent::Failed("late".to_owned())))]
#[case::progress(Input::Job(JobEvent::FileProgress { path: PathBuf::from("a") }))]
fn w7_any_job_event_while_idle_changes_nothing(#[case] event: Input) {
    let (mut s, _) = ready_to_seal();

    let out = s.apply(event);

    assert!(!out.changed);
    assert_eq!(s.work, Work::Idle);
    assert!(s.text.output.is_empty());
}

#[rstest]
#[case::while_idle(false)]
#[case::while_running(true)]
fn w8_stopped_ends_the_worker_and_clears_every_note(#[case] running: bool) {
    let (mut s, _) = ready_to_seal();
    s.notes.set(Tab::Text, Note::Copied(Copied::Output));
    s.notes.set(Tab::Files, Note::OtherTabFailed(Tab::Text));
    if running {
        running_text(&mut s);
    }

    let out = s.apply(Input::Stopped);

    assert!(out.changed);
    assert_eq!(s.work, Work::Stopped);
    assert!(Tab::ALL.iter().all(|tab| s.note(*tab).is_none()));
}

#[test]
fn w8_after_the_stop_another_stop_or_a_job_event_changes_nothing() {
    let (mut s, _) = ready_to_seal();
    s.apply(Input::Stopped);

    let again = s.apply(Input::Stopped);
    let late = s.apply(finished_text("late"));

    assert!(!again.changed);
    assert!(!late.changed);
    assert_eq!(s.work, Work::Stopped);
}

#[test]
fn w9_a_start_after_the_stop_starts_nothing() {
    let (mut s, _) = ready_to_seal();
    s.apply(Input::Stopped);

    let out = start(&mut s);

    assert!(out.effects.is_empty());
    assert_eq!(s.work, Work::Stopped);
}

#[test]
fn w11_unsealed_bytes_that_are_not_text_clear_the_output_and_point_to_files() {
    let (mut s, _) = ready_to_seal();
    s.text.output = "earlier".to_owned();
    s.work = Work::Running(RunningJob::Text(Direction::Decrypt));

    s.apply(Input::Job(JobEvent::Finished(JobOutcome::TextRedirect)));

    assert!(s.text.output.is_empty());
    assert_eq!(s.note(Tab::Text), Some(&Note::NotText));
    assert_eq!(Note::NotText.words().0, Tone::Warning);
}

#[test]
fn w12_switching_direction_clears_the_output_and_keeps_the_input() {
    let (mut s, _) = ready_to_seal();
    s.text.output = "armor".to_owned();

    s.apply(Input::Text(TextInput::DirectionPicked(Direction::Decrypt)));

    assert_eq!(s.text.direction, Direction::Decrypt);
    assert!(s.text.output.is_empty());
    assert_eq!(s.text.input.text, "hello");
}

#[test]
fn w12_switching_direction_while_a_text_job_runs_is_refused() {
    let (mut s, _) = ready_to_seal();
    s.text.output = "armor".to_owned();
    running_text(&mut s);

    let out = s.apply(Input::Text(TextInput::DirectionPicked(Direction::Decrypt)));

    assert!(!out.changed);
    assert_eq!(s.text.direction, Direction::Encrypt);
    assert_eq!(s.text.output, "armor");
}

#[test]
fn w13_nothing_selected_says_so_and_focuses_the_first_recipient() {
    let (mut s, _) = recording_state(state_with_book(&["Alice"]));
    s.text.input = TextEdit::new("hello");

    let out = start(&mut s);

    assert_eq!(s.note(Tab::Text), Some(&Note::Missing(Missing::Recipients)));
    assert_eq!(out.focus, Some(Focus::TextRecipient(first_id(&s))));
}

#[test]
fn w13_a_saved_key_that_no_longer_parses_names_its_label() {
    let (mut s, _) = recording_state(AppState::new(
        SealConfig {
            recipients: vec![RecipientEntry {
                label: "Alice".to_owned(),
                public_key: "hand-edited garbage".to_owned(),
            }],
            ..SealConfig::default()
        },
        FileLogging::Off,
        None,
        None,
    ));
    s.text.input = TextEdit::new("hello");
    let alice = first_id(&s);
    s.text.picker.toggle(alice);

    let out = start(&mut s);

    assert_eq!(
        s.note(Tab::Text),
        Some(&Note::Missing(Missing::SavedKey {
            id: alice,
            label: "Alice".to_owned()
        }))
    );
    assert_eq!(out.focus, Some(Focus::TextRecipient(alice)));
}

#[test]
fn w13_a_bad_saved_key_shared_by_label_with_a_good_one_focuses_the_bad_entry() {
    let (mut s, _) = recording_state(AppState::new(
        SealConfig {
            recipients: vec![
                RecipientEntry {
                    label: "Alice".to_owned(),
                    public_key: fresh_key(),
                },
                RecipientEntry {
                    label: "Alice".to_owned(),
                    public_key: "hand-edited garbage".to_owned(),
                },
            ],
            ..SealConfig::default()
        },
        FileLogging::Off,
        None,
        None,
    ));
    s.text.input = TextEdit::new("hello");
    let good = s.book.entries()[0].id;
    let bad = s.book.entries()[1].id;
    s.text.picker.toggle(good);
    s.text.picker.toggle(bad);

    let out = start(&mut s);

    assert_eq!(out.focus, Some(Focus::TextRecipient(bad)));
}

#[test]
fn copy_while_a_text_job_runs_copies_nothing() {
    let (mut s, _) = ready_to_seal();
    s.text.output = "earlier".to_owned();
    running_text(&mut s);

    let out = s.apply(Input::Text(TextInput::CopyPressed));

    assert!(out.effects.is_empty());
    assert_eq!(s.note(Tab::Text), None);
}

#[test]
fn clearing_a_failure_takes_the_other_tabs_pointers_at_it_with_it() {
    let (mut s, _) = ready_to_seal();
    running_text(&mut s);
    s.apply(Input::Job(JobEvent::Failed("broken".to_owned())));

    s.apply(Input::Text(TextInput::InputEdited));

    assert_eq!(s.note(Tab::Text), None);
    assert_eq!(s.note(Tab::Files), None);
    assert_eq!(s.note(Tab::Keys), None);
}

#[test]
fn w13_a_bad_pasted_key_says_so_and_focuses_the_field() {
    let (mut s, _) = recording_state(state_with_book(&[]));
    s.text.input = TextEdit::new("hello");
    s.text.picker.other_key = TextEdit::new("age1-nope");

    let out = start(&mut s);

    assert_eq!(s.note(Tab::Text), Some(&Note::Missing(Missing::OtherKey)));
    assert_eq!(out.focus, Some(Focus::TextOtherKey));
}

#[test]
fn w13_selection_follows_the_entry_when_the_book_changes_around_it() {
    let (mut s, _) = recording_state(state_with_book(&["Alice", "Bea"]));
    s.text.input = TextEdit::new("hello");
    let bea = s.book.entries()[1].id;
    s.text.picker.toggle(bea);
    let carol = s.book.push("Carol".to_owned(), fresh_key());

    let out = start(&mut s);

    assert!(!s.text.picker.selected.contains(&carol));
    assert!(matches!(
        out.effects.as_slice(),
        [Effect::Run(JobRequest::EncryptText { recipients, .. })] if recipients.len() == 1
    ));
}

#[test]
fn w13_two_entries_sharing_a_label_toggle_separately() {
    let (mut s, _) = recording_state(state_with_book(&["Alice", "Alice"]));
    let second = s.book.entries()[1].id;

    s.apply(Input::Text(TextInput::RecipientToggled(second)));

    assert_eq!(s.text.picker.selected.len(), 1);
    assert!(s.text.picker.selected.contains(&second));
}

#[test]
fn w13_a_toggle_for_an_id_not_in_the_book_changes_nothing() {
    let (mut s, _) = recording_state(state_with_book(&["Alice"]));
    let gone = {
        let mut other = state_with_book(&["X", "Y", "Z"]);
        other.book.push("W".to_owned(), fresh_key())
    };

    let out = s.apply(Input::Text(TextInput::RecipientToggled(gone)));

    assert!(!out.changed);
    assert!(s.text.picker.selected.is_empty());
}

#[test]
fn w14_the_running_phrase_wins_over_a_note() {
    let (mut s, _) = ready_to_seal();
    s.notes.set(Tab::Text, Note::Copied(Copied::Output));
    running_text(&mut s);

    assert_eq!(
        s.status(Tab::Text),
        Some((Tone::Muted, "Sealing…".to_owned()))
    );
}

#[test]
fn w14_another_tab_shows_the_running_job_by_tab_name_over_its_note() {
    let (mut s, _) = ready_to_seal();
    s.notes.set(Tab::Files, Note::OtherTabFailed(Tab::Keys));
    running_text(&mut s);

    assert_eq!(
        s.status(Tab::Files),
        Some((Tone::Muted, "Text: sealing…".to_owned()))
    );
}

#[test]
fn w14_an_idle_tab_shows_its_note_or_nothing() {
    let (mut s, _) = ready_to_seal();
    s.notes.set(Tab::Text, Note::Copied(Copied::Output));

    assert_eq!(
        s.status(Tab::Text),
        Some((Tone::Muted, "Copied the output.".to_owned()))
    );
    assert_eq!(s.status(Tab::Keys), None);
}

#[test]
fn w14_after_the_stop_a_copy_shows_its_note() {
    let (mut s, _) = ready_to_seal();
    s.text.output = "armor".to_owned();
    s.apply(Input::Stopped);

    let out = s.apply(Input::Text(TextInput::CopyPressed));

    assert!(matches!(out.effects.as_slice(), [Effect::Copy(text)] if text == "armor"));
    assert_eq!(
        s.status(Tab::Text),
        Some((Tone::Muted, "Copied the output.".to_owned()))
    );
}

#[test]
fn copy_with_no_output_copies_nothing() {
    let (mut s, _) = ready_to_seal();

    let out = s.apply(Input::Text(TextInput::CopyPressed));

    assert!(out.effects.is_empty());
    assert_eq!(s.note(Tab::Text), None);
}

fn identity_purpose() -> DialogPurpose {
    DialogPurpose::Identity(Tab::Text)
}

#[test]
fn w15_opening_a_dialog_marks_it_and_asks_for_it() {
    let (mut s, _) = ready_to_seal();

    let out = s.apply(Input::DialogOpened(identity_purpose()));

    assert_eq!(s.dialog, Some(identity_purpose()));
    assert!(matches!(
        out.effects.as_slice(),
        [Effect::OpenDialog(purpose)] if *purpose == identity_purpose()
    ));
}

#[test]
fn w15_a_second_open_while_one_shows_is_refused() {
    let (mut s, _) = ready_to_seal();
    s.apply(Input::DialogOpened(identity_purpose()));

    let out = s.apply(Input::DialogOpened(identity_purpose()));

    assert!(out.effects.is_empty());
    assert_eq!(s.dialog, Some(identity_purpose()));
}

#[test]
fn w15_a_path_answer_sets_the_identity_and_clears_the_mark() {
    let (mut s, _) = ready_to_seal();
    s.apply(Input::DialogOpened(identity_purpose()));

    s.apply(Input::DialogAnswered(
        identity_purpose(),
        Ok(Some(PathBuf::from("id.txt"))),
    ));

    assert_eq!(s.dialog, None);
    assert_eq!(s.identity_path, Some(PathBuf::from("id.txt")));
}

#[test]
fn w15_a_cancel_clears_the_mark_and_keeps_the_identity() {
    let (mut s, _) = ready_to_seal();
    s.identity_path = Some(PathBuf::from("kept.txt"));
    s.apply(Input::DialogOpened(identity_purpose()));

    s.apply(Input::DialogAnswered(identity_purpose(), Ok(None)));

    assert_eq!(s.dialog, None);
    assert_eq!(s.identity_path, Some(PathBuf::from("kept.txt")));
}

#[test]
fn w15_an_error_clears_the_mark_and_becomes_the_note() {
    let (mut s, _) = ready_to_seal();
    s.apply(Input::DialogOpened(identity_purpose()));

    s.apply(Input::DialogAnswered(
        identity_purpose(),
        Err(FileDialogError("no portal".to_owned())),
    ));

    assert_eq!(s.dialog, None);
    assert_eq!(
        s.note(Tab::Text),
        Some(&Note::DialogError("no portal".to_owned()))
    );
}

#[test]
fn w15_an_answer_for_another_purpose_or_with_none_open_changes_nothing() {
    let (mut s, _) = ready_to_seal();

    let unasked = s.apply(Input::DialogAnswered(
        identity_purpose(),
        Ok(Some(PathBuf::from("id.txt"))),
    ));
    s.apply(Input::DialogOpened(identity_purpose()));
    let other = s.apply(Input::DialogAnswered(
        DialogPurpose::Identity(Tab::Files),
        Ok(Some(PathBuf::from("other.txt"))),
    ));

    assert!(!other.changed);
    assert!(!unasked.changed);
    assert_eq!(s.identity_path, None);
    assert_eq!(s.dialog, Some(identity_purpose()));
}

#[test]
fn w16_a_text_input_clears_the_text_note_only() {
    let (mut s, _) = ready_to_seal();
    s.notes.set(Tab::Text, Note::Copied(Copied::Output));
    s.notes.set(Tab::Keys, Note::Copied(Copied::Output));

    s.apply(Input::Text(TextInput::InputEdited));

    assert_eq!(s.note(Tab::Text), None);
    assert_eq!(s.note(Tab::Keys), Some(&Note::Copied(Copied::Output)));
}

#[test]
fn w16_a_job_event_clears_no_note() {
    let (mut s, _) = ready_to_seal();
    s.notes.set(Tab::Text, Note::Copied(Copied::Output));
    running_text(&mut s);

    s.apply(Input::Job(JobEvent::FileProgress {
        path: PathBuf::from("a"),
    }));

    assert_eq!(s.note(Tab::Text), Some(&Note::Copied(Copied::Output)));
}

#[test]
fn w16_a_failure_set_while_another_tab_shows_is_there_on_arrival() {
    let (mut s, _) = ready_to_seal();
    s.apply(Input::TabPicked(Tab::Files));
    running_text(&mut s);
    s.apply(Input::Job(JobEvent::Failed("broken".to_owned())));

    s.apply(Input::TabPicked(Tab::Text));

    assert_eq!(s.note(Tab::Text), Some(&Note::Failure("broken".to_owned())));
}

#[test]
fn w16_another_tabs_failure_goes_once_that_tab_is_visited() {
    let (mut s, _) = ready_to_seal();
    running_text(&mut s);
    s.apply(Input::Job(JobEvent::Failed("broken".to_owned())));
    s.apply(Input::TabPicked(Tab::Files));

    s.apply(Input::TabPicked(Tab::Text));

    assert_eq!(s.note(Tab::Files), None);
    assert_eq!(s.note(Tab::Keys), None);
    assert_eq!(s.note(Tab::Text), Some(&Note::Failure("broken".to_owned())));
}

#[test]
fn w16_another_tabs_failure_goes_once_its_tab_starts_again() {
    let (mut s, _) = ready_to_seal();
    running_text(&mut s);
    s.apply(Input::Job(JobEvent::Failed("broken".to_owned())));

    start(&mut s);

    assert_eq!(s.note(Tab::Files), None);
    assert_eq!(s.note(Tab::Keys), None);
}

#[test]
fn w16_a_tab_switch_keeps_each_tabs_own_note() {
    let (mut s, _) = ready_to_seal();
    s.notes.set(Tab::Text, Note::Copied(Copied::Output));
    s.notes.set(Tab::Files, Note::Failure("old".to_owned()));

    s.apply(Input::TabPicked(Tab::Files));

    assert_eq!(s.note(Tab::Text), Some(&Note::Copied(Copied::Output)));
    assert_eq!(s.note(Tab::Files), Some(&Note::Failure("old".to_owned())));
}

#[test]
fn picking_the_text_tab_focuses_the_input_and_the_same_tab_changes_nothing() {
    let (mut s, _) = ready_to_seal();
    s.apply(Input::TabPicked(Tab::Keys));

    let back = s.apply(Input::TabPicked(Tab::Text));
    let again = s.apply(Input::TabPicked(Tab::Text));

    assert_eq!(back.focus, Some(Focus::TextInput));
    assert!(!again.changed);
}

#[test]
fn unseal_without_an_identity_says_so() {
    let (mut s, _) = ready_to_seal();
    s.text.direction = Direction::Decrypt;

    let out = start(&mut s);

    assert_eq!(s.note(Tab::Text), Some(&Note::Missing(Missing::Identity)));
    assert_eq!(out.focus, Some(Focus::TextChooseIdentity));
}

#[test]
fn unseal_with_an_identity_runs_one_unseal_job() {
    let (mut s, _) = ready_to_seal();
    s.text.direction = Direction::Decrypt;
    s.identity_path = Some(PathBuf::from("id.txt"));

    let out = start(&mut s);

    assert_eq!(s.work, Work::Running(RunningJob::Text(Direction::Decrypt)));
    assert!(matches!(
        out.effects.as_slice(),
        [Effect::Run(JobRequest::DecryptText { identity_path, .. })]
            if identity_path == &PathBuf::from("id.txt")
    ));
}

#[test]
fn editing_the_other_key_marks_and_unmarks_it() {
    let (mut s, _) = ready_to_seal();
    s.text.picker.other_key = TextEdit::new("nope");

    s.apply(Input::Text(TextInput::OtherKeyEdited));
    let marked = s.text.picker.other_key_bad;
    s.text.picker.other_key = TextEdit::new("");
    s.apply(Input::Text(TextInput::OtherKeyEdited));

    assert!(marked);
    assert!(!s.text.picker.other_key_bad);
}

#[test]
fn debug_of_state_inputs_and_effects_shows_no_text() {
    const MARKER: &str = "marker-90210";
    let (mut s, _) = ready_to_seal();
    s.text.input = TextEdit::new(MARKER);
    s.text.output = MARKER.to_owned();
    s.text.picker.other_key = TextEdit::new(MARKER);
    let effect = Effect::Copy(MARKER.to_owned());
    let input = Input::Job(JobEvent::Finished(JobOutcome::Text(MARKER.to_owned())));

    let shown = format!("{s:?} {effect:?} {input:?}");

    assert!(!shown.contains(MARKER), "{shown}");
}

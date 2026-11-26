use super::keys_fixtures::{add_via_form, delete, fresh_key, id_of, keys, on_keys, runs, saves};
use super::*;
use crate::config::{ConfigNote, RecipientEntry};
use crate::test_support::recording_state;
use keys::{AddIssue, Generated};
use rstest::rstest;
use rune::FileDialogError;

#[test]
fn c4_generate_opens_the_save_dialog_and_its_path_starts_a_keygen_that_keeps_the_file() {
    let mut s = on_keys(&[]);

    let opened = keys(&mut s, KeysInput::GeneratePressed);
    let started = s.apply(Input::DialogAnswered(
        DialogPurpose::SaveIdentity,
        Ok(Some(PathBuf::from("id.txt"))),
    ));

    assert!(matches!(
        opened.effects.as_slice(),
        [Effect::OpenDialog(DialogPurpose::SaveIdentity)]
    ));
    assert!(matches!(
        runs(&started).as_slice(),
        [JobRequest::Keygen { path, replace: false }] if path == Path::new("id.txt")
    ));
    assert_eq!(s.work, Work::Running(RunningJob::Keygen));
}

#[rstest]
#[case::dialog_open(Some(DialogPurpose::AddFile), Work::Idle)]
#[case::running(None, Work::Running(RunningJob::Text(Direction::Encrypt)))]
#[case::stopped(None, Work::Stopped)]
fn c4_generate_is_refused_with_a_dialog_open_or_the_worker_busy(
    #[case] dialog: Option<DialogPurpose>,
    #[case] work: Work,
) {
    let mut s = on_keys(&[]);
    s.dialog = dialog;
    s.work = work;

    let out = keys(&mut s, KeysInput::GeneratePressed);

    assert!(out.effects.is_empty());
    assert_eq!(s.dialog, dialog);
}

#[rstest]
#[case::cancelled(Ok(None))]
#[case::failed(Err(FileDialogError("no portal".to_owned())))]
fn c4_a_cancelled_or_failed_save_dialog_starts_nothing(#[case] result: rune::FileDialogResult) {
    let mut s = on_keys(&[]);
    keys(&mut s, KeysInput::GeneratePressed);

    let out = s.apply(Input::DialogAnswered(DialogPurpose::SaveIdentity, result));

    assert!(runs(&out).is_empty());
    assert_eq!(s.work, Work::Idle);
    assert_eq!(s.keys.step, KeygenStep::Idle);
}

fn confirming(path: &str) -> AppState {
    let mut s = on_keys(&[]);
    s.work = Work::Running(RunningJob::Keygen);
    s.apply(Input::Job(JobEvent::Finished(JobOutcome::KeygenExists {
        path: PathBuf::from(path),
    })));
    s
}

#[test]
fn c4_an_existing_file_raises_the_confirm_and_replace_starts_a_keygen_that_replaces_it() {
    let mut s = confirming("id.txt");
    let step = s.keys.step.clone();

    let out = keys(&mut s, KeysInput::ReplacePressed);

    assert_eq!(
        step,
        KeygenStep::ConfirmReplace {
            path: PathBuf::from("id.txt")
        }
    );
    assert!(matches!(
        runs(&out).as_slice(),
        [JobRequest::Keygen { replace: true, .. }]
    ));
    assert_eq!(s.keys.step, KeygenStep::Idle);
    assert_eq!(out.focus, Some(Focus::Generate));
}

#[test]
fn c4_replace_while_a_job_runs_is_refused_and_the_confirm_stays() {
    let mut s = confirming("id.txt");
    s.work = Work::Running(RunningJob::Text(Direction::Encrypt));

    let out = keys(&mut s, KeysInput::ReplacePressed);

    assert!(runs(&out).is_empty());
    assert!(matches!(s.keys.step, KeygenStep::ConfirmReplace { .. }));
}

#[rstest]
#[case::cancel(KeysInput::CancelPressed)]
#[case::label_edit(KeysInput::LabelEdited)]
#[case::key_edit(KeysInput::KeyEdited)]
#[case::add(KeysInput::AddPressed)]
#[case::copy_generated(KeysInput::CopyGenerated)]
#[case::add_generated(KeysInput::AddGeneratedPressed)]
fn c4_cancel_and_each_disarming_input_drop_the_confirm(#[case] input: KeysInput) {
    let mut s = confirming("id.txt");

    let out = keys(&mut s, input);

    assert_eq!(s.keys.step, KeygenStep::Idle);
    assert!(runs(&out).is_empty());
}

#[test]
fn c4_delete_or_copy_on_a_book_row_drops_the_replace_confirm() {
    for input in ["delete", "copy"] {
        let mut s = on_keys(&["Alice"]);
        s.work = Work::Running(RunningJob::Keygen);
        s.apply(Input::Job(JobEvent::Finished(JobOutcome::KeygenExists {
            path: PathBuf::from("id.txt"),
        })));
        let alice = id_of(&s, 0);

        keys(
            &mut s,
            match input {
                "delete" => KeysInput::DeletePressed(alice),
                _ => KeysInput::CopyEntry(alice),
            },
        );

        assert_eq!(s.keys.step, KeygenStep::Idle, "{input}");
    }
}

#[test]
fn c4_an_existing_file_answered_while_a_delete_is_armed_leaves_one_confirm() {
    let mut s = on_keys(&["Alice"]);
    let alice = id_of(&s, 0);
    s.work = Work::Running(RunningJob::Keygen);
    keys(&mut s, KeysInput::DeletePressed(alice));

    s.apply(Input::Job(JobEvent::Finished(JobOutcome::KeygenExists {
        path: PathBuf::from("id.txt"),
    })));

    assert_eq!(s.keys.armed, None);
    assert!(matches!(s.keys.step, KeygenStep::ConfirmReplace { .. }));
}

#[test]
fn c9_deleting_the_entry_a_duplicate_label_named_clears_that_issue() {
    let mut s = on_keys(&["Alice", "Bob"]);
    add_via_form(&mut s, "Alice", &fresh_key());
    let issue = s.keys.issue.clone();
    let (alice, bob) = (id_of(&s, 0), id_of(&s, 1));

    delete(&mut s, bob);
    let after_other = s.keys.issue.clone();
    delete(&mut s, alice);

    assert_eq!(issue, Some(AddIssue::DuplicateLabel("Alice".to_owned())));
    assert_eq!(after_other, issue);
    assert_eq!(s.keys.issue, None);
}

#[test]
fn c4_return_in_the_label_field_while_armed_disarms_and_adds_without_a_keygen() {
    let mut s = confirming("id.txt");

    let out = add_via_form(&mut s, "Bob", &fresh_key());

    assert_eq!(s.keys.step, KeygenStep::Idle);
    assert!(runs(&out).is_empty());
    assert_eq!(s.book.entries().len(), 1);
}

#[test]
fn c4_generate_during_the_confirm_changes_nothing() {
    let mut s = confirming("id.txt");

    let out = keys(&mut s, KeysInput::GeneratePressed);

    assert!(!out.changed && out.effects.is_empty());
    assert!(matches!(s.keys.step, KeygenStep::ConfirmReplace { .. }));
}

#[test]
fn c4_a_stop_drops_the_confirm_and_replace_at_rest_does_nothing() {
    let mut s = confirming("id.txt");

    s.apply(Input::Stopped);
    let at_rest = keys(&mut s, KeysInput::ReplacePressed);

    assert_eq!(s.keys.step, KeygenStep::Idle);
    assert!(!at_rest.changed && at_rest.effects.is_empty());
}

#[test]
fn c4_c6_a_failed_keygen_keeps_the_previous_key_and_the_step_at_rest() {
    let mut s = on_keys(&[]);
    let previous = Generated {
        public_key: "age1previous".to_owned(),
        path: PathBuf::from("old.txt"),
    };
    s.keys.generated = Some(previous.clone());
    s.work = Work::Running(RunningJob::Keygen);

    s.apply(Input::Job(JobEvent::Failed("denied".to_owned())));

    assert_eq!(s.keys.generated, Some(previous));
    assert_eq!(s.keys.step, KeygenStep::Idle);
}

#[test]
fn c6_a_finished_keygen_keeps_the_key_with_its_path_and_says_so() {
    let mut s = on_keys(&[]);
    s.work = Work::Running(RunningJob::Keygen);

    let out = s.apply(Input::Job(JobEvent::Finished(JobOutcome::Keygen {
        public_key: "age1new".to_owned(),
        path: PathBuf::from("new.txt"),
    })));

    assert_eq!(
        s.keys.generated,
        Some(Generated {
            public_key: "age1new".to_owned(),
            path: PathBuf::from("new.txt"),
        })
    );
    assert_eq!(s.note(Tab::Keys), Some(&Note::KeySaved));
    assert_eq!(out.focus, Some(Focus::Generate));
}

#[rstest]
#[case::running(Work::Running(RunningJob::Text(Direction::Encrypt)))]
#[case::stopped(Work::Stopped)]
fn c5_a_path_answered_while_the_worker_is_busy_or_stopped_is_dropped(#[case] work: Work) {
    let mut s = on_keys(&[]);
    keys(&mut s, KeysInput::GeneratePressed);
    s.work = work;

    let out = s.apply(Input::DialogAnswered(
        DialogPurpose::SaveIdentity,
        Ok(Some(PathBuf::from("id.txt"))),
    ));

    assert!(runs(&out).is_empty());
    assert_eq!(s.work, work);
    assert_eq!(s.note(Tab::Keys), None);
}

#[test]
fn c5b_a_confirm_raised_on_another_tab_waits_and_a_switch_away_from_keys_drops_it() {
    let mut s = confirming("id.txt");
    s.tab = Tab::Text;
    s.apply(Input::Job(JobEvent::Finished(JobOutcome::KeygenExists {
        path: PathBuf::from("id.txt"),
    })));

    s.apply(Input::TabPicked(Tab::Files));
    let after_other_switch = s.keys.step.clone();
    s.apply(Input::TabPicked(Tab::Keys));
    let on_arrival = s.keys.step.clone();
    s.apply(Input::TabPicked(Tab::Text));

    assert!(matches!(
        after_other_switch,
        KeygenStep::ConfirmReplace { .. }
    ));
    assert!(matches!(on_arrival, KeygenStep::ConfirmReplace { .. }));
    assert_eq!(s.keys.step, KeygenStep::Idle);
}

#[test]
fn c7_delete_arms_then_confirms_and_the_arm_moves_between_rows() {
    let mut s = on_keys(&["Alice", "Bob"]);
    let (alice, bob) = (id_of(&s, 0), id_of(&s, 1));

    keys(&mut s, KeysInput::DeletePressed(alice));
    let first_arm = s.keys.armed;
    keys(&mut s, KeysInput::DeletePressed(bob));
    let moved = s.keys.armed;
    let out = keys(&mut s, KeysInput::DeletePressed(bob));

    assert_eq!(first_arm, Some(alice));
    assert_eq!(moved, Some(bob));
    assert_eq!(s.keys.armed, None);
    assert_eq!(s.book.entries().len(), 1);
    assert_eq!(saves(&out).len(), 1);
}

#[rstest]
#[case::saved(Input::ConfigSaved(SaveOutcome::Saved))]
#[case::job(Input::Job(JobEvent::Failed("x".to_owned())))]
#[case::switch_to_keys(Input::TabPicked(Tab::Keys))]
#[case::stopped(Input::Stopped)]
fn c7_background_answers_and_a_switch_to_keys_keep_the_arm(#[case] input: Input) {
    let mut s = on_keys(&["Alice"]);
    let alice = id_of(&s, 0);
    keys(&mut s, KeysInput::DeletePressed(alice));

    s.apply(input);

    assert_eq!(s.keys.armed, Some(alice));
}

#[rstest]
#[case::label_edit(Input::Keys(KeysInput::LabelEdited))]
#[case::key_edit(Input::Keys(KeysInput::KeyEdited))]
#[case::add(Input::Keys(KeysInput::AddPressed))]
#[case::copy_generated(Input::Keys(KeysInput::CopyGenerated))]
#[case::add_generated(Input::Keys(KeysInput::AddGeneratedPressed))]
#[case::cancel(Input::Keys(KeysInput::CancelPressed))]
#[case::switch_away(Input::TabPicked(Tab::Text))]
fn c7_each_disarming_input_drops_the_arm(#[case] input: Input) {
    let mut s = on_keys(&["Alice", "Bob"]);
    let alice = id_of(&s, 0);
    keys(&mut s, KeysInput::DeletePressed(alice));

    s.apply(input);

    assert_eq!(s.keys.armed, None);
    assert_eq!(s.book.entries().len(), 2);
}

#[test]
fn c7_copy_on_another_row_and_generate_drop_the_arm() {
    let mut s = on_keys(&["Alice", "Bob"]);
    let (alice, bob) = (id_of(&s, 0), id_of(&s, 1));
    keys(&mut s, KeysInput::DeletePressed(alice));
    keys(&mut s, KeysInput::CopyEntry(bob));
    let after_copy = s.keys.armed;
    keys(&mut s, KeysInput::DeletePressed(alice));

    keys(&mut s, KeysInput::GeneratePressed);

    assert_eq!(after_copy, None);
    assert_eq!(s.keys.armed, None);
}

#[test]
fn c7_a_confirm_in_a_session_that_never_writes_removes_the_entry_and_saves_nothing() {
    let mut s = on_keys(&["Alice"]);
    s.config_status = ConfigStatus::Refused(ConfigNote::Unreadable);
    let alice = id_of(&s, 0);

    let out = delete(&mut s, alice);

    assert!(s.book.is_empty());
    assert!(saves(&out).is_empty());
}

#[test]
fn c7_confirm_lands_on_the_next_row_then_the_previous_then_the_label_field() {
    let mut s = on_keys(&["Alice", "Bob", "Carol"]);
    let (alice, bob, carol) = (id_of(&s, 0), id_of(&s, 1), id_of(&s, 2));

    let middle = delete(&mut s, bob);
    let last = delete(&mut s, carol);
    let only = delete(&mut s, alice);

    assert_eq!(middle.focus, Some(Focus::Delete(carol)));
    assert_eq!(last.focus, Some(Focus::Delete(alice)));
    assert_eq!(only.focus, Some(Focus::AddLabel));
}

#[test]
fn c8_deleting_an_entry_keeps_the_others_checked_by_entry() {
    let mut s = on_keys(&["Alice", "Bob", "Carol"]);
    let (bob, carol) = (id_of(&s, 1), id_of(&s, 2));
    s.files.picker.toggle(carol);
    s.text.picker.toggle(bob);

    delete(&mut s, bob);

    assert!(s.files.picker.selected.contains(&carol));
    assert!(!s.text.picker.selected.contains(&bob));
}

#[test]
fn c8_of_two_entries_sharing_a_label_deleting_one_leaves_the_other_and_its_check() {
    let key = fresh_key();
    let mut s = AppState::new(
        (
            SealConfig {
                recipients: vec![
                    RecipientEntry {
                        label: "Alice".to_owned(),
                        public_key: key.clone(),
                    },
                    RecipientEntry {
                        label: "Alice".to_owned(),
                        public_key: fresh_key(),
                    },
                ],
                ..SealConfig::default()
            },
            ConfigStatus::Clean,
        ),
        FileLogging::Off,
        None,
        None,
    );
    s.tab = Tab::Keys;
    let (first, second) = (id_of(&s, 0), id_of(&s, 1));
    s.text.picker.toggle(second);

    delete(&mut s, first);

    assert_eq!(s.book.entries().len(), 1);
    assert_eq!(s.book.entries()[0].id, second);
    assert!(s.text.picker.selected.contains(&second));
}

#[test]
fn deleting_an_entry_clears_a_note_that_named_its_key() {
    let mut s = on_keys(&["Alice"]);
    let alice = id_of(&s, 0);
    s.notes_mut_for_tests().set(
        Tab::Text,
        Note::Missing(Missing::SavedKey {
            id: alice,
            label: "Alice".to_owned(),
        }),
    );

    delete(&mut s, alice);

    assert_eq!(s.note(Tab::Text), None);
}

#[test]
fn c9_a_good_add_clears_the_form_names_the_entry_and_reveals_its_row() {
    let mut s = on_keys(&["Alice"]);
    let before = id_of(&s, 0);

    let out = add_via_form(&mut s, "Bob", &fresh_key());
    let added = id_of(&s, 1);

    assert_ne!(added, before);
    assert_eq!(s.keys.label.text, "");
    assert_eq!(s.keys.key.text, "");
    assert_eq!(s.note(Tab::Keys), Some(&Note::Added("Bob".to_owned())));
    assert_eq!(out.focus, Some(Focus::AddLabel));
    assert_eq!(out.reveal, Some(Focus::Delete(added)));
}

#[rstest]
#[case::empty("", Focus::AddLabel)]
#[case::taken("Alice", Focus::AddLabel)]
fn c9_a_label_issue_keeps_the_form_and_focuses_the_label(#[case] label: &str, #[case] at: Focus) {
    let mut s = on_keys(&["Alice"]);

    let out = add_via_form(&mut s, label, &fresh_key());

    assert_eq!(s.book.entries().len(), 1);
    assert!(s.keys.issue.as_ref().is_some_and(AddIssue::on_label));
    assert_eq!(s.keys.label.text, label);
    assert_eq!(out.focus, Some(at));
}

#[test]
fn c9_a_bad_key_focuses_the_key_and_an_edit_there_clears_the_issue() {
    let mut s = on_keys(&[]);

    let out = add_via_form(&mut s, "Bob", "age1nope");
    let issue = s.keys.issue.clone();
    keys(&mut s, KeysInput::KeyEdited);

    assert_eq!(issue, Some(AddIssue::BadKey));
    assert_eq!(out.focus, Some(Focus::AddKey));
    assert_eq!(s.keys.issue, None);
}

#[rstest]
#[case::running(Work::Running(RunningJob::Text(Direction::Encrypt)))]
#[case::stopped(Work::Stopped)]
fn c10_generate_waits_on_the_worker_and_add_recipient_does_not(#[case] work: Work) {
    let mut s = on_keys(&[]);
    s.work = work;

    let generate = keys(&mut s, KeysInput::GeneratePressed);
    add_via_form(&mut s, "Bob", &fresh_key());

    assert!(generate.effects.is_empty());
    assert_eq!(s.book.entries().len(), 1);
}

#[test]
fn add_to_book_uses_the_stem_and_then_has_nothing_to_add() {
    let mut s = on_keys(&[]);
    let key = fresh_key();
    s.keys.generated = Some(Generated {
        public_key: key.clone(),
        path: PathBuf::from("work-laptop.txt"),
    });

    let out = keys(&mut s, KeysInput::AddGeneratedPressed);
    let again = keys(&mut s, KeysInput::AddGeneratedPressed);

    assert_eq!(s.book.entries()[0].label, "work-laptop");
    assert_eq!(s.book.entries()[0].public_key, key);
    assert_eq!(saves(&out).len(), 1);
    assert!(saves(&again).is_empty());
    assert_eq!(s.book.entries().len(), 1);
}

#[test]
fn add_to_book_with_a_taken_stem_fills_the_form_and_focuses_the_label() {
    let mut s = on_keys(&["work-laptop"]);
    let key = fresh_key();
    s.keys.generated = Some(Generated {
        public_key: key.clone(),
        path: PathBuf::from("work-laptop.txt"),
    });

    let out = keys(&mut s, KeysInput::AddGeneratedPressed);

    assert_eq!(s.book.entries().len(), 1);
    assert_eq!(s.keys.label.text, "work-laptop");
    assert_eq!(s.keys.key.text, key.as_str());
    assert_eq!(
        s.keys.issue,
        Some(AddIssue::DuplicateLabel("work-laptop".to_owned()))
    );
    assert_eq!(out.focus, Some(Focus::AddLabel));
}

#[test]
fn copying_a_book_key_puts_it_on_the_clipboard_and_says_so() {
    let (mut s, _) = recording_state(on_keys(&["Alice"]));
    let alice = id_of(&s, 0);
    let key = s.book.entries()[0].public_key.clone();

    let out = keys(&mut s, KeysInput::CopyEntry(alice));

    assert!(matches!(out.effects.as_slice(), [Effect::Copy(copied)] if *copied == key));
    assert_eq!(s.note(Tab::Keys), Some(&Note::Copied(Copied::PublicKey)));
}

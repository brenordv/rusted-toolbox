use super::keys_fixtures::{add_via_form, answer, delete, fresh_key, id_of, on_keys, saves};
use super::*;
use crate::config::ConfigNote;
use rstest::rstest;

/// One change the config holds, applied to a window with one book entry.
type Change = fn(&mut AppState) -> Outcome;

/// The five changes the config holds, by name.
fn each_change() -> Vec<(&'static str, Change)> {
    vec![
        ("identity", |s| {
            answer(s, DialogPurpose::Identity(Picker::Text), "id.txt")
        }),
        ("folder", |s| answer(s, DialogPurpose::OutputFolder, "safe")),
        ("overwrite", |s| {
            s.apply(Input::Files(FilesInput::OverwriteToggled))
        }),
        ("added", |s| add_via_form(s, "Bob", &fresh_key())),
        ("deleted", |s| {
            let alice = id_of(s, 0);
            delete(s, alice)
        }),
    ]
}

#[test]
fn c1_each_change_saves_one_snapshot_holding_it_and_no_session_ids() {
    for (name, change) in each_change() {
        let mut s = on_keys(&["Alice"]);

        let out = change(&mut s);
        let saved = saves(&out);

        assert_eq!(saved.len(), 1, "{name}");
        let snapshot = saved[0];
        match name {
            "identity" => assert_eq!(snapshot.last_identity_file, Some(PathBuf::from("id.txt"))),
            "folder" => assert_eq!(snapshot.last_output_folder, Some(PathBuf::from("safe"))),
            "overwrite" => assert!(snapshot.overwrite_existing),
            "added" => assert_eq!(snapshot.recipients.len(), 2),
            _ => assert!(snapshot.recipients.is_empty(), "{name}"),
        }
        let json = serde_json::to_string(snapshot).unwrap();
        assert!(!json.contains("\"id\""), "{name}: {json}");
    }
}

#[test]
fn c2_a_failed_save_is_shown_until_a_save_succeeds_and_is_retried_on_close() {
    let mut s = on_keys(&[]);

    let quiet_close = s.apply(Input::Closing);
    s.apply(Input::ConfigSaved(SaveOutcome::Failed(
        "disk full".to_owned(),
    )));
    let failed = s.config_status.clone();
    let retry = s.apply(Input::Closing);
    s.apply(Input::ConfigSaved(SaveOutcome::Saved));

    assert!(saves(&quiet_close).is_empty());
    assert_eq!(failed, ConfigStatus::Failed("disk full".to_owned()));
    assert_eq!(saves(&retry).len(), 1);
    assert_eq!(s.config_status, ConfigStatus::Clean);
}

#[test]
fn c3_a_second_failure_replaces_the_first() {
    let mut s = on_keys(&[]);
    s.apply(Input::ConfigSaved(SaveOutcome::Failed("first".to_owned())));

    s.apply(Input::ConfigSaved(SaveOutcome::Failed("second".to_owned())));

    assert_eq!(s.config_status, ConfigStatus::Failed("second".to_owned()));
}

#[rstest]
#[case::refused(ConfigStatus::Refused(ConfigNote::Foreign))]
#[case::no_location(ConfigStatus::NoLocation)]
fn c3_a_session_that_never_writes_saves_nothing_and_ignores_answers(#[case] status: ConfigStatus) {
    for (name, change) in each_change() {
        let mut s = on_keys(&["Alice"]);
        s.config_status = status.clone();

        let out = change(&mut s);
        let closing = s.apply(Input::Closing);
        s.apply(Input::ConfigSaved(SaveOutcome::Saved));

        assert!(saves(&out).is_empty(), "{name}");
        assert!(saves(&closing).is_empty(), "{name}");
        assert_eq!(s.config_status, status, "{name}");
    }
}

#[rstest]
#[case::clean(ConfigStatus::Clean)]
#[case::failed(ConfigStatus::Failed("x".to_owned()))]
fn c3_a_refused_or_no_location_answer_in_a_writable_session_changes_nothing(
    #[case] status: ConfigStatus,
) {
    let mut s = on_keys(&[]);
    s.config_status = status.clone();

    let refused = s.apply(Input::ConfigSaved(SaveOutcome::Refused));
    let nowhere = s.apply(Input::ConfigSaved(SaveOutcome::NoLocation));

    assert!(!refused.changed && !nowhere.changed);
    assert_eq!(s.config_status, status);
}

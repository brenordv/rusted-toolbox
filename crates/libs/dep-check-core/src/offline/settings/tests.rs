use super::*;
use crate::offline::store::StoreMeta;
use chrono::Utc;
use rstest::rstest;

fn path_in(dir: &tempfile::TempDir) -> PathBuf {
    dir.path().join("dep-check").join("offline.json")
}

fn ready() -> DataStatus {
    let now = Utc::now();
    DataStatus::Ready {
        meta: StoreMeta {
            etag: None,
            archive_bytes: 1,
            records: 1,
            downloaded_at: now,
            current_at: now,
            last_failure: None,
        },
        file_bytes: 1,
    }
}

#[test]
fn a_missing_file_reads_as_the_defaults() {
    let dir = tempfile::tempdir().unwrap();

    let settings = load(Some(&path_in(&dir)));

    assert_eq!(
        settings,
        Settings {
            enabled: true,
            mode: OfflineMode::Fallback,
            auto_update: false
        }
    );
}

#[test]
fn no_home_folder_reads_as_the_defaults_and_cannot_change() {
    let changed = change(None, SettingChange::AutoUpdate(true));

    assert_eq!(load(None), Settings::default());
    assert!(changed.is_err());
}

#[test]
fn a_change_is_written_and_read_back() {
    let dir = tempfile::tempdir().unwrap();
    let path = path_in(&dir);

    let written = change(Some(&path), SettingChange::Mode(OfflineMode::Always)).unwrap();

    assert_eq!(written.mode, OfflineMode::Always);
    assert_eq!(load(Some(&path)), written);
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        json,
        serde_json::json!({"settings_version": 1, "enabled": true, "mode": "always", "auto_update": false})
    );
}

#[test]
fn a_change_merges_into_what_another_writer_saved() {
    let dir = tempfile::tempdir().unwrap();
    let path = path_in(&dir);
    change(Some(&path), SettingChange::AutoUpdate(true)).unwrap();

    let written = change(Some(&path), SettingChange::Enabled(false)).unwrap();

    assert_eq!(
        written,
        Settings {
            enabled: false,
            mode: OfflineMode::Fallback,
            auto_update: true
        }
    );
}

#[test]
fn an_unparseable_file_reads_as_the_defaults_and_a_change_replaces_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = path_in(&dir);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"{ not json").unwrap();

    let loaded = load(Some(&path));
    let written = change(Some(&path), SettingChange::AutoUpdate(true)).unwrap();

    assert_eq!(loaded, Settings::default());
    assert_eq!(load(Some(&path)), written);
}

#[test]
fn a_newer_file_is_read_for_what_this_version_knows_and_never_written() {
    let dir = tempfile::tempdir().unwrap();
    let path = path_in(&dir);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let newer = r#"{"settings_version": 2, "enabled": false, "mode": "always", "auto_update": true, "colour": "teal"}"#;
    std::fs::write(&path, newer).unwrap();

    let loaded = load(Some(&path));
    let error = change(Some(&path), SettingChange::Enabled(true)).unwrap_err();

    assert_eq!(
        loaded,
        Settings {
            enabled: false,
            mode: OfflineMode::Always,
            auto_update: true
        }
    );
    assert_eq!(
        error.to_string(),
        "these settings were saved by a newer dep-check"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), newer);
}

#[test]
fn a_newer_file_with_a_mode_this_version_does_not_know_reads_always_and_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let path = path_in(&dir);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let newer =
        r#"{"settings_version": 2, "enabled": false, "mode": "sealed", "auto_update": "daily"}"#;
    std::fs::write(&path, newer).unwrap();

    let loaded = load(Some(&path));
    let changed = change(Some(&path), SettingChange::Enabled(true));

    assert_eq!(
        loaded,
        Settings {
            enabled: false,
            mode: OfflineMode::Always,
            auto_update: false
        }
    );
    assert!(changed.is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), newer);
}

#[test]
fn a_version_that_is_not_a_number_is_never_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let path = path_in(&dir);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let odd = r#"{"settings_version": "2", "enabled": false}"#;
    std::fs::write(&path, odd).unwrap();

    let changed = change(Some(&path), SettingChange::Enabled(true));

    assert!(changed.is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), odd);
}

#[test]
fn a_lock_that_cannot_be_opened_fails_the_change() {
    let dir = tempfile::tempdir().unwrap();
    let path = path_in(&dir);
    std::fs::create_dir_all(lock_path(&path)).unwrap();

    let changed = change(Some(&path), SettingChange::Enabled(false));

    assert!(changed.is_err());
    assert!(!path.exists());
}

#[test]
fn a_change_waits_for_another_writer_to_finish() {
    let dir = tempfile::tempdir().unwrap();
    let path = path_in(&dir);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let held = locks::lock_waiting(&lock_path(&path)).unwrap();
    let target = path.clone();

    let writer = std::thread::spawn(move || change(Some(&target), SettingChange::AutoUpdate(true)));
    std::thread::sleep(std::time::Duration::from_millis(200));
    let finished_while_held = writer.is_finished();
    drop(held);
    let written = writer.join().unwrap().unwrap();

    assert!(!finished_while_held);
    assert!(written.auto_update);
    assert_eq!(load(Some(&path)), written);
}

#[test]
fn a_file_that_cannot_be_read_reads_as_the_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let path = path_in(&dir);
    std::fs::create_dir_all(&path).unwrap();

    assert_eq!(load(Some(&path)), Settings::default());
}

#[test]
fn a_folder_where_the_file_should_be_fails_the_change() {
    let dir = tempfile::tempdir().unwrap();
    let path = path_in(&dir);
    std::fs::create_dir_all(&path).unwrap();

    let changed = change(Some(&path), SettingChange::Enabled(false));

    assert!(changed.is_err());
    assert!(path.is_dir());
}

#[rstest]
#[case::default_fallback(true, OfflineMode::Fallback, false, OfflineUse::Fallback)]
#[case::off(false, OfflineMode::Fallback, false, OfflineUse::Off)]
#[case::off_in_always_mode(false, OfflineMode::Always, false, OfflineUse::Off)]
#[case::always_with_data(true, OfflineMode::Always, false, OfflineUse::Only)]
#[case::force_beats_off(false, OfflineMode::Fallback, true, OfflineUse::Only)]
#[case::force_beats_fallback(true, OfflineMode::Fallback, true, OfflineUse::Only)]
fn decide_follows_the_settings_and_the_force(
    #[case] enabled: bool,
    #[case] mode: OfflineMode,
    #[case] force: bool,
    #[case] expected: OfflineUse,
) {
    let settings = Settings {
        enabled,
        mode,
        auto_update: false,
    };

    assert_eq!(OfflineUse::decide(&settings, &ready(), force), expected);
}

#[test]
fn auto_update_changes_no_use() {
    let settings = Settings {
        auto_update: true,
        ..Settings::default()
    };

    assert_eq!(
        OfflineUse::decide(&settings, &ready(), false),
        OfflineUse::Fallback
    );
}

#[rstest]
#[case::not_downloaded(DataStatus::NotDownloaded, OfflineUse::Off)]
#[case::unavailable(DataStatus::Unavailable { reason: "busy".to_owned() }, OfflineUse::Only)]
#[case::locked(DataStatus::Locked, OfflineUse::Only)]
#[case::unreadable(DataStatus::Unreadable { reason: "corrupt".to_owned() }, OfflineUse::Only)]
#[case::newer_format(DataStatus::NewerFormat, OfflineUse::Only)]
fn always_mode_goes_online_only_with_no_data_at_all(
    #[case] status: DataStatus,
    #[case] expected: OfflineUse,
) {
    let settings = Settings {
        mode: OfflineMode::Always,
        ..Settings::default()
    };

    assert_eq!(OfflineUse::decide(&settings, &status, false), expected);
}

#[rstest]
#[case::off(OfflineUse::Off, TimeDelta::zero())]
#[case::fallback(OfflineUse::Fallback, TimeDelta::days(7))]
#[case::only(OfflineUse::Only, TimeDelta::hours(48))]
fn each_use_has_its_freshness_window(#[case] use_: OfflineUse, #[case] window: TimeDelta) {
    assert_eq!(use_.max_age(), window);
}

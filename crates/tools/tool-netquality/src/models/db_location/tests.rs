use super::*;
use common_file_utils::adoption::KeptReason;
use rstest::rstest;
use std::time::Duration;
use tempfile::{TempDir, tempdir};

const OLD_DB: &[u8] = b"the old database's bytes";

/// A temp folder with an old default beside a fake executable folder and a new database path
/// in a fake toolbox home.
struct Places {
    _dir: TempDir,
    legacy: PathBuf,
    target: PathBuf,
}

fn places() -> Places {
    let dir = tempdir().unwrap();
    let exe_folder = dir.path().join("bin");
    fs::create_dir_all(&exe_folder).unwrap();
    let (target, legacy) =
        default_db_path(Some(dir.path().join(".rusted-toolbox")), || Ok(exe_folder)).unwrap();
    Places {
        legacy: legacy.unwrap(),
        target,
        _dir: dir,
    }
}

fn storage(places: &Places) -> StorageConfig {
    StorageConfig {
        db_path: places.target.clone(),
        legacy_db: Some(places.legacy.clone()),
        cleanup_enabled: true,
        cleanup_interval: Duration::from_secs(3_600),
    }
}

fn beside(db: &Path, suffix: &str) -> PathBuf {
    let mut name = db.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

#[test]
fn the_default_is_in_netqualitys_folder_with_the_executables_folder_as_the_old_place() {
    let root = tempdir().unwrap();
    let exe = tempdir().unwrap();
    let exe_folder = exe.path().to_path_buf();

    let (db_path, legacy) =
        default_db_path(Some(root.path().to_path_buf()), || Ok(exe_folder)).unwrap();

    assert_eq!(
        db_path,
        root.path().join("netquality").join("netquality.db")
    );
    assert_eq!(legacy, Some(exe.path().join("netquality.db")));
}

#[test]
fn with_no_toolbox_home_the_default_stays_beside_the_executable() {
    let exe = tempdir().unwrap();
    let exe_folder = exe.path().to_path_buf();

    let (db_path, legacy) = default_db_path(None, || Ok(exe_folder)).unwrap();

    assert_eq!(db_path, exe.path().join("netquality.db"));
    assert_eq!(legacy, None);
}

#[test]
fn a_folder_of_the_executable_that_cannot_be_found_fails_the_default() {
    let root = tempdir().unwrap();

    let error = default_db_path(Some(root.path().to_path_buf()), || {
        Err(anyhow::anyhow!("no executable path"))
    })
    .unwrap_err();

    assert!(
        format!("{error:#}").contains("Failed to get tool path"),
        "{error:#}"
    );
}

#[test]
fn a_database_only_at_the_old_default_moves_to_netqualitys_folder() {
    let places = places();
    fs::write(&places.legacy, OLD_DB).unwrap();
    let mut storage = storage(&places);

    let adoption = storage.adopt_legacy_db().unwrap();

    assert!(matches!(&adoption.adoption, Adoption::Moved { .. }));
    assert_eq!(storage.db_path, places.target);
    assert_eq!(storage.legacy_db, None);
    assert_eq!(fs::read(&places.target).unwrap(), OLD_DB);
    assert!(!places.legacy.exists());
}

#[rstest]
#[case::rollback_journal("-journal")]
#[case::write_ahead_log("-wal")]
#[case::shared_memory("-shm")]
fn a_journal_beside_the_old_database_keeps_it_in_place(#[case] suffix: &str) {
    let places = places();
    fs::write(&places.legacy, OLD_DB).unwrap();
    fs::write(beside(&places.legacy, suffix), b"journal").unwrap();
    let mut storage = storage(&places);

    let adoption = storage.adopt_legacy_db().unwrap();

    assert!(matches!(
        &adoption.adoption,
        Adoption::KeptLegacy {
            reason: KeptReason::InUse(_),
            ..
        }
    ));
    assert_eq!(storage.db_path, places.legacy);
    assert_eq!(fs::read(&places.legacy).unwrap(), OLD_DB);
    assert!(!places.target.exists());
}

#[test]
fn a_journal_beside_the_new_database_keeps_the_old_one_in_place() {
    let places = places();
    fs::write(&places.legacy, OLD_DB).unwrap();
    fs::create_dir_all(places.target.parent().unwrap()).unwrap();
    fs::write(beside(&places.target, "-journal"), b"journal").unwrap();
    let mut storage = storage(&places);

    let adoption = storage.adopt_legacy_db().unwrap();

    assert!(matches!(
        &adoption.adoption,
        Adoption::KeptLegacy {
            reason: KeptReason::InUse(_),
            ..
        }
    ));
    assert_eq!(storage.db_path, places.legacy);
}

#[test]
fn a_journal_that_cannot_be_checked_keeps_the_old_database_in_place() {
    let places = places();
    let legacy = places.legacy.with_file_name("n".repeat(250));
    fs::write(&legacy, OLD_DB).unwrap();
    let mut storage = StorageConfig {
        legacy_db: Some(legacy.clone()),
        ..storage(&places)
    };

    let adoption = storage.adopt_legacy_db().unwrap();

    assert!(matches!(
        &adoption.adoption,
        Adoption::KeptLegacy {
            reason: KeptReason::InUse(_),
            ..
        }
    ));
    assert_eq!(storage.db_path, legacy);
    assert!(!places.target.exists());
}

#[test]
fn a_chosen_path_moves_nothing() {
    let places = places();
    fs::write(&places.legacy, OLD_DB).unwrap();
    let mut storage = StorageConfig {
        legacy_db: None,
        ..storage(&places)
    };

    let adoption = storage.adopt_legacy_db();

    assert!(adoption.is_none());
    assert_eq!(storage.db_path, places.target);
    assert_eq!(fs::read(&places.legacy).unwrap(), OLD_DB);
}

#[cfg(windows)]
#[test]
fn a_database_open_under_its_old_name_keeps_being_used_there() {
    let places = places();
    let open = rusqlite::Connection::open(&places.legacy).unwrap();
    open.execute_batch("CREATE TABLE kept (id INTEGER); INSERT INTO kept VALUES (7);")
        .unwrap();
    let mut storage = storage(&places);

    let adoption = storage.adopt_legacy_db().unwrap();

    assert!(matches!(
        &adoption.adoption,
        Adoption::KeptLegacy {
            reason: KeptReason::Failed(_),
            ..
        }
    ));
    assert_eq!(storage.db_path, places.legacy);
    let reopened = rusqlite::Connection::open(&storage.db_path).unwrap();
    let id: i64 = reopened
        .query_row("SELECT id FROM kept", [], |row| row.get(0))
        .unwrap();
    assert_eq!(id, 7);
}

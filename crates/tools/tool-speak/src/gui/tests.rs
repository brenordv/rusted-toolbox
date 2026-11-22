use super::*;
use tempfile::TempDir;

fn runtime() -> Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn settings_file(dir: &TempDir) -> PathBuf {
    dir.path().join("tool-speak").join("config.json")
}

#[test]
fn startup_config_with_no_file_is_a_first_run() {
    let dir = tempfile::tempdir().unwrap();

    let config = startup_config(&runtime(), &settings_file(&dir)).unwrap();

    assert_eq!(config, None);
}

#[test]
fn startup_config_returns_what_the_file_holds() {
    let dir = tempfile::tempdir().unwrap();
    let path = settings_file(&dir);
    let saved = SpeakConfig {
        url: "http://my-host:10157/v1/audio/speech".to_owned(),
        ..SpeakConfig::default()
    };
    saved.save(&path).unwrap();

    let config = startup_config(&runtime(), &path).unwrap();

    assert_eq!(config, Some(saved));
}

#[test]
fn startup_config_of_broken_json_is_an_error_naming_the_file_and_leaves_it_alone() {
    let dir = tempfile::tempdir().unwrap();
    let path = settings_file(&dir);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "{ not json").unwrap();

    let error = startup_config(&runtime(), &path).unwrap_err();

    assert!(
        format!("{error:#}").contains(&path.display().to_string()),
        "{error:#}"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ not json");
}

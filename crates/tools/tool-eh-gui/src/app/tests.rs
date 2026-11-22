use super::*;
use crate::form::AuthChoice;
use crate::state::NO_WORKSPACE;
use shared_eventhub_persistence::config::CONFIG_FILE_NAME;
use shared_eventhub_persistence::export::ExportFormat;

async fn preload(dir: &Path) -> Preload {
    preload_config(dir, Err(NO_WORKSPACE.to_owned()), FileLogging::Off).await
}

#[tokio::test]
async fn no_config_file_leaves_the_default_form_and_no_caption() {
    let dir = tempfile::tempdir().unwrap();

    let preload = preload(dir.path()).await;

    assert!(preload.recalled.caption.is_none());
    assert_eq!(preload.recalled.form.auth, AuthChoice::ConnectionString);
}

#[tokio::test]
async fn another_tools_config_is_not_loaded_and_the_caption_says_so() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(CONFIG_FILE_NAME), r#"{"tool":"other"}"#).unwrap();

    let preload = preload(dir.path()).await;

    assert_eq!(
        preload.recalled.caption.as_deref(),
        Some("A config.json here belongs to another tool; it was not loaded")
    );
}

#[tokio::test]
async fn an_eh_config_fills_the_form_and_the_caption_names_fields_only() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(CONFIG_FILE_NAME),
        r#"{"tool":"eh","schema_version":1,"target":{"namespace":"orders.servicebus.windows.net","event_hub":"orders","auth":{"method":"credential","credential":{"kind":"developer-tools"}}}}"#,
    )
    .unwrap();

    let preload = preload(dir.path()).await;

    let caption = preload.recalled.caption.as_deref().unwrap();
    assert_eq!(preload.recalled.form.auth, AuthChoice::Credential);
    assert_eq!(
        preload.recalled.form.namespace.text,
        "orders.servicebus.windows.net"
    );
    assert!(
        caption.starts_with("Loaded from config.json: namespace"),
        "{caption}"
    );
    assert!(!caption.contains("orders.servicebus"), "{caption}");
}

#[tokio::test]
async fn a_broken_eh_config_shows_why_without_echoing_its_values() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(CONFIG_FILE_NAME),
        r#"{"tool":"eh","schema_version":1,"target":{"auth":{"method":"connection-string","stored":"Endpoint=sb://x/;SharedAccessKey=marker-8812"}}}"#,
    )
    .unwrap();

    let preload = preload(dir.path()).await;

    let caption = preload.recalled.caption.as_deref().unwrap();
    assert!(
        caption.starts_with("config.json was not loaded"),
        "{caption}"
    );
    assert!(!caption.contains("marker-8812"), "{caption}");
    assert_eq!(preload.recalled.form.auth, AuthChoice::ConnectionString);
}

#[tokio::test]
async fn a_launch_config_naming_a_data_folder_outside_the_launch_folder_starts_on_the_default() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let outside_folder = outside.path().display().to_string().replace('\\', "/");
    std::fs::write(
        dir.path().join(CONFIG_FILE_NAME),
        format!(
            r#"{{"tool":"eh","schema_version":1,"target":{{"auth":{{"method":"credential","credential":{{"kind":"developer-tools"}}}}}},"storage":{{"base_data_folder":"{outside_folder}"}}}}"#
        ),
    )
    .unwrap();
    let preload = preload(dir.path()).await;
    let paths = preload.data_paths();

    let state = preload.into_state(Box::new(
        crate::test_harness::services::RecordingServices::new(
            crate::test_harness::services::CallLog::default(),
        ),
    ));

    assert_eq!(paths, DataPaths::new(".eh-data"));
    assert_eq!(state.data_paths(), paths);
    assert_eq!(
        state.folders_caption.as_deref(),
        Some("config.json names a data folder outside this folder; it was not used.")
    );
    assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
}

#[tokio::test]
async fn the_first_state_starts_on_the_launch_folder_with_the_recalled_settings() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(CONFIG_FILE_NAME),
        r#"{"tool":"eh","schema_version":1,"target":{"auth":{"method":"credential","credential":{"kind":"developer-tools"}}},"storage":{"base_data_folder":"captures","export":{"format":"json","include_metadata":false}}}"#,
    )
    .unwrap();
    let workspace = Workspace::at(dir.path().join("profiles"));
    let preload = preload_config(dir.path(), Ok(workspace.clone()), FileLogging::Off).await;
    let paths = preload.data_paths();

    let state = preload.into_state(Box::new(
        crate::test_harness::services::RecordingServices::new(
            crate::test_harness::services::CallLog::default(),
        ),
    ));

    assert_eq!(paths, DataPaths::new("captures"));
    assert_eq!(state.data_paths(), paths);
    assert_eq!(
        state.home(),
        &ConfigHome::CurrentFolder(dir.path().to_path_buf())
    );
    assert_eq!(state.workspace(), Ok(&workspace));
    assert_eq!(state.read.inspector.format, ExportFormat::Json);
    assert!(!state.archive.options.metadata);
}

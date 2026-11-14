use super::*;
use crate::form::AuthChoice;
use shared_eventhub_persistence::config::CONFIG_FILE_NAME;

#[tokio::test]
async fn no_config_file_leaves_the_default_form_and_no_caption() {
    let dir = tempfile::tempdir().unwrap();

    let preload = preload_config(dir.path(), FileLogging::Off).await;

    assert!(preload.config_caption.is_none());
    assert_eq!(preload.form.auth, AuthChoice::ConnectionString);
}

#[tokio::test]
async fn another_tools_config_is_not_loaded_and_the_caption_says_so() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(CONFIG_FILE_NAME), r#"{"tool":"other"}"#).unwrap();

    let preload = preload_config(dir.path(), FileLogging::Off).await;

    assert_eq!(
        preload.config_caption.as_deref(),
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

    let preload = preload_config(dir.path(), FileLogging::Off).await;

    let caption = preload.config_caption.unwrap();
    assert_eq!(preload.form.auth, AuthChoice::Credential);
    assert_eq!(preload.form.namespace.text, "orders.servicebus.windows.net");
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

    let preload = preload_config(dir.path(), FileLogging::Off).await;

    let caption = preload.config_caption.unwrap();
    assert!(
        caption.starts_with("config.json was not loaded"),
        "{caption}"
    );
    assert!(!caption.contains("marker-8812"), "{caption}");
    assert_eq!(preload.form.auth, AuthChoice::ConnectionString);
}

use super::*;
use crate::read::test_support::message;
use std::path::PathBuf;

#[test]
fn a_small_body_is_shown_whole() {
    assert_eq!(shown_body("hello"), ("hello", None));
}

#[test]
fn a_large_body_is_cut_on_a_char_boundary_with_a_caption() {
    let body = format!("{}é{}", "a".repeat(BODY_DISPLAY_CAP - 1), "b".repeat(10));

    let (shown, caption) = shown_body(&body);

    assert_eq!(shown.len(), BODY_DISPLAY_CAP - 1);
    assert_eq!(
        caption.as_deref(),
        Some("Showing the first 256.00 KB. Copy body copies all 256.01 KB.")
    );
}

#[test]
fn copy_as_json_writes_the_message_fields() {
    let json = message_json(&message("3", 42, "{\"a\":1}")).unwrap();

    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["key"]["partition_id"], "3");
    assert_eq!(value["key"]["sequence_number"], 42);
    assert_eq!(value["body"], "{\"a\":1}");
    assert_eq!(value["body_kind"], "Utf8");
}

#[test]
fn the_export_file_is_named_after_the_key_and_format() {
    let key = message("3", 42, "").key.clone();

    assert_eq!(export_file_name(&key, ExportFormat::Csv), "p3-s42.csv");
}

#[test]
fn export_options_write_one_file_without_touching_the_export_log() {
    let destination = Path::new("exports").join("one.json");

    let options = export_options(ExportFormat::Json, true, &destination);

    assert_eq!(
        options,
        ExportOptions {
            format: ExportFormat::Json,
            include_metadata: true,
            condense_output: false,
            use_local_time: false,
            export_folder: PathBuf::from("exports"),
            skip_already_exported: false,
        }
    );
}

#[test]
fn feedback_shows_only_for_its_key_and_kind() {
    let key = message("0", 1, "").key.clone();
    let other = message("0", 2, "").key.clone();
    let feedback = Feedback::new(key.clone(), FeedbackKind::CopiedBody, Utc::now());

    assert!(feedback.shows(&key, FeedbackKind::CopiedBody));
    assert!(!feedback.shows(&other, FeedbackKind::CopiedBody));
    assert!(!feedback.shows(&key, FeedbackKind::CopiedJson));
}

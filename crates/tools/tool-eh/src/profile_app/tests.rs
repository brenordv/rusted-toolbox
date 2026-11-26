use super::*;
use common_cli::test_writers::{ClosedPipe, FailingDisk};
use eh_persistence::config::CONFIG_FILE_NAME;

const CREDENTIAL_CONFIG: &str = r#"{"tool":"eh","schema_version":1,"target":{"namespace":"billing.servicebus.windows.net","event_hub":"invoices","auth":{"method":"credential","credential":{"kind":"developer-tools"}}}}"#;
const STRING_CONFIG: &str = r#"{"tool":"eh","schema_version":1,"target":{"event_hub":"orders","auth":{"method":"connection-string","stored":"local-db"}}}"#;
const INVALID_CONFIG: &str = r#"{"tool":"eh","schema_version":1,"target":{}}"#;

fn profile_folder(root: &Path, name: &str, config: &str) {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(CONFIG_FILE_NAME), config).unwrap();
}

async fn listing(root: &Path) -> String {
    let entries = Workspace::at(root.to_owned()).list().await.unwrap();
    list_text(root, &entries)
}

#[tokio::test]
async fn an_empty_workspace_says_how_to_save_a_profile() {
    let root = tempfile::tempdir().unwrap();

    let text = listing(root.path()).await;

    assert_eq!(
        text,
        format!(
            "No profiles in '{}' yet. Save one with: eh read --profile NAME --save-config \
             <target flags>\n",
            root.path().display()
        )
    );
}

#[tokio::test]
async fn each_profile_shows_its_target_and_auth_with_names_aligned() {
    let root = tempfile::tempdir().unwrap();
    profile_folder(root.path(), "billing-prod", CREDENTIAL_CONFIG);
    profile_folder(root.path(), "orders", STRING_CONFIG);

    let text = listing(root.path()).await;

    assert_eq!(
        text,
        format!(
            "Profiles in '{}':\n  \
             billing-prod  namespace billing.servicebus.windows.net, event hub invoices, auth \
             developer tools credential\n  \
             orders        namespace (from the connection string), event hub orders, auth \
             connection string (stored)\n",
            root.path().display()
        )
    );
}

#[tokio::test]
async fn a_profile_that_cannot_load_shows_why() {
    let root = tempfile::tempdir().unwrap();
    profile_folder(root.path(), "scratch", INVALID_CONFIG);

    let text = listing(root.path()).await;

    let line = text.lines().nth(1).unwrap();
    assert!(line.starts_with("  scratch  can't load: '"), "{line}");
    assert!(
        line.contains("is an eh config, but it is not valid"),
        "{line}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_hostile_folder_name_is_escaped() {
    let root = tempfile::tempdir().unwrap();
    profile_folder(root.path(), "evil\u{1b}]52;c;x\u{7}", CREDENTIAL_CONFIG);

    let text = listing(root.path()).await;

    assert!(!text.contains('\u{1b}'), "{text:?}");
    assert!(text.contains("\\u{1b}"), "{text:?}");
    assert!(text.contains("can't load: the folder name"), "{text:?}");
}

#[test]
fn a_closed_reader_is_not_a_failure() {
    print_list(&mut ClosedPipe, "Profiles\n").unwrap();
}

#[test]
fn any_other_write_failure_fails_the_listing() {
    let error = print_list(&mut FailingDisk, "Profiles\n").unwrap_err();

    assert!(
        format!("{error:#}").starts_with("Failed to print the profile list"),
        "{error:#}"
    );
}

#[test]
fn the_listing_is_written_whole() {
    let mut out = Vec::new();

    print_list(&mut out, "Profiles\n").unwrap();

    assert_eq!(out, b"Profiles\n");
}

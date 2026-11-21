use super::*;
use crate::config::CONFIG_FILE_NAME;
use rstest::rstest;

const READY_CONFIG: &str = r#"{"tool":"eh","schema_version":1,"target":{"namespace":"orders.servicebus.windows.net","event_hub":"orders","auth":{"method":"credential","credential":{"kind":"developer-tools"}}}}"#;
const PASTED_SECRET: &str = "SharedAccessKey=pasted-secret";

fn invalid_config() -> String {
    format!(
        r#"{{"tool":"eh","schema_version":1,"target":{{"auth":{{"method":"Endpoint=sb://x/;{PASTED_SECRET}"}}}}}}"#
    )
}

fn profile_folder(root: &Path, name: &str, config: Option<&str>) {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    if let Some(config) = config {
        std::fs::write(dir.join(CONFIG_FILE_NAME), config).unwrap();
    }
}

fn name(text: &str) -> ProfileName {
    ProfileName::parse(text).unwrap()
}

#[rstest]
#[case::plain("orders")]
#[case::mixed("Orders-Dev_2")]
#[case::one_letter("a")]
#[case::dotted("a.b")]
#[case::longest(&"a".repeat(64))]
#[case::device_prefix_longer("com10")]
#[case::device_word_inside("console")]
#[case::device_word_start("nullable")]
fn a_valid_name_parses(#[case] text: &str) {
    assert_eq!(ProfileName::parse(text).unwrap().as_str(), text);
}

#[rstest]
#[case::empty("")]
#[case::too_long(&"a".repeat(65))]
#[case::hidden(".hidden")]
#[case::parent("..")]
#[case::flag_like("-x")]
#[case::underscore_first("_x")]
#[case::trailing_dot("a.")]
#[case::space("a b")]
#[case::slash("a/b")]
#[case::backslash("a\\b")]
#[case::colon("a:b")]
#[case::star("a*")]
#[case::non_ascii("é")]
#[case::control("a\u{1b}")]
fn a_name_outside_the_allow_list_is_refused_without_quoting_it(#[case] text: &str) {
    let error = ProfileName::parse(text).unwrap_err().to_string();

    assert!(error.starts_with("A profile name uses letters"), "{error}");
    assert!(text.is_empty() || !error.contains(text), "{error}");
}

#[rstest]
#[case::lower("con")]
#[case::upper("CON")]
#[case::with_extension("nul.txt")]
#[case::mixed_case("Com1")]
#[case::two_extensions("lpt9.tar.gz")]
fn a_windows_device_name_is_refused(#[case] text: &str) {
    let error = ProfileName::parse(text).unwrap_err().to_string();

    assert_eq!(
        error,
        format!("{text} is a device name on Windows; pick another name")
    );
}

#[test]
fn an_absolute_workspace_variable_wins_over_the_home_folder() {
    let root = tempfile::tempdir().unwrap();

    let workspace = locate_with(
        Some(root.path().as_os_str().to_owned()),
        Some(PathBuf::from("home")),
    )
    .unwrap();

    assert_eq!(workspace.root(), root.path());
}

#[test]
fn a_relative_workspace_variable_is_refused_naming_it() {
    let error = locate_with(
        Some(OsString::from("profiles")),
        Some(PathBuf::from("home")),
    )
    .unwrap_err()
    .to_string();

    assert!(
        error.contains("EH_WORKSPACE must be an absolute folder path"),
        "{error}"
    );
}

#[rstest]
#[case::unset(None)]
#[case::empty(Some(OsString::new()))]
fn without_the_variable_the_workspace_is_under_the_home_folder(#[case] variable: Option<OsString>) {
    let home = tempfile::tempdir().unwrap();

    let workspace = locate_with(variable, Some(home.path().to_owned())).unwrap();

    assert_eq!(workspace.root(), home.path().join(".eh").join("profiles"));
}

#[test]
fn without_the_variable_or_a_home_folder_locating_fails_naming_the_variable() {
    let error = locate_with(None, None).unwrap_err().to_string();

    assert!(error.contains("set EH_WORKSPACE"), "{error}");
}

#[tokio::test]
async fn a_profile_in_a_missing_workspace_is_a_folder_under_the_root() {
    let root = tempfile::tempdir().unwrap();
    let workspace = Workspace::at(root.path().join("missing"));

    let profile = workspace.profile(&name("orders")).await.unwrap();

    assert_eq!(profile.name(), "orders");
    assert_eq!(profile.dir(), root.path().join("missing").join("orders"));
}

#[tokio::test]
async fn an_existing_folder_is_found_ignoring_case_and_keeps_its_spelling() {
    let root = tempfile::tempdir().unwrap();
    profile_folder(root.path(), "Orders", None);

    let profile = Workspace::at(root.path().to_owned())
        .profile(&name("orders"))
        .await
        .unwrap();

    assert_eq!(profile.name(), "Orders");
    assert_eq!(profile.dir(), root.path().join("Orders"));
}

#[tokio::test]
async fn a_file_with_the_profile_name_is_not_the_profile_folder() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("ORDERS"), "x").unwrap();

    let profile = Workspace::at(root.path().to_owned())
        .profile(&name("orders"))
        .await
        .unwrap();

    assert_eq!(profile.dir(), root.path().join("orders"));
}

#[tokio::test]
async fn a_workspace_root_that_is_a_file_fails_to_find_a_profile() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("not-a-folder");
    std::fs::write(&file, "x").unwrap();

    let error = Workspace::at(file)
        .profile(&name("orders"))
        .await
        .unwrap_err();

    assert!(
        error.to_string().contains("Failed to list the workspace"),
        "{error:#}"
    );
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn two_folders_that_match_ignoring_case_fail_naming_both() {
    let root = tempfile::tempdir().unwrap();
    profile_folder(root.path(), "Orders", None);
    profile_folder(root.path(), "orders", None);

    let error = Workspace::at(root.path().to_owned())
        .profile(&name("ORDERS"))
        .await
        .unwrap_err()
        .to_string();

    assert_eq!(
        error,
        "Two profile folders match ORDERS ignoring case: 'Orders' and 'orders'; rename one"
    );
}

#[tokio::test]
async fn a_new_profile_creates_its_folder_and_a_missing_root() {
    let root = tempfile::tempdir().unwrap();
    let workspace = Workspace::at(root.path().join("missing"));

    let profile = workspace.new_profile(&name("orders-new")).await.unwrap();

    assert_eq!(profile.name(), "orders-new");
    assert_eq!(
        profile.dir(),
        root.path().join("missing").join("orders-new")
    );
    assert!(profile.dir().is_dir());
}

#[cfg(unix)]
#[tokio::test]
async fn a_new_profile_folder_and_its_new_root_have_no_group_or_other_bits() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().unwrap();
    let workspace = Workspace::at(root.path().join("missing"));

    let profile = workspace.new_profile(&name("orders-new")).await.unwrap();

    for dir in [workspace.root(), profile.dir()] {
        let mode = std::fs::metadata(dir).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0, "{}", dir.display());
    }
}

#[tokio::test]
async fn a_folder_without_a_config_is_reused_with_its_spelling() {
    let root = tempfile::tempdir().unwrap();
    profile_folder(root.path(), "Orders", None);

    let profile = Workspace::at(root.path().to_owned())
        .new_profile(&name("orders"))
        .await
        .unwrap();

    assert_eq!(profile.name(), "Orders");
    assert_eq!(profile.dir(), root.path().join("Orders"));
}

#[tokio::test]
async fn a_folder_holding_a_config_is_refused_naming_the_profile() {
    let root = tempfile::tempdir().unwrap();
    profile_folder(root.path(), "orders", Some(READY_CONFIG));

    let error = Workspace::at(root.path().to_owned())
        .new_profile(&name("ORDERS"))
        .await
        .unwrap_err()
        .to_string();

    assert!(
        error.starts_with("A profile named orders already exists in "),
        "{error}"
    );
    let kept = std::fs::read_to_string(root.path().join("orders").join(CONFIG_FILE_NAME)).unwrap();
    assert_eq!(kept, READY_CONFIG);
}

#[tokio::test]
async fn a_file_named_like_the_new_profile_fails_naming_its_path() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("orders");
    std::fs::write(&file, "x").unwrap();

    let error = Workspace::at(root.path().to_owned())
        .new_profile(&name("orders"))
        .await
        .unwrap_err();

    assert!(
        format!("{error:#}").contains(&file.display().to_string()),
        "{error:#}"
    );
    assert!(file.is_file());
}

#[tokio::test]
async fn a_missing_workspace_lists_nothing() {
    let root = tempfile::tempdir().unwrap();

    let entries = Workspace::at(root.path().join("missing"))
        .list()
        .await
        .unwrap();

    assert!(entries.is_empty());
}

#[tokio::test]
async fn a_workspace_root_that_is_a_file_fails_to_list() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("not-a-folder");
    std::fs::write(&file, "x").unwrap();

    let error = Workspace::at(file).list().await.unwrap_err();

    assert!(
        error.to_string().contains("Failed to list the workspace"),
        "{error:#}"
    );
}

fn listed(entries: &[ProfileEntry]) -> Vec<(&str, Option<&str>)> {
    entries
        .iter()
        .map(|entry| {
            let reason = match &entry.state {
                ProfileState::Ready(_) => None,
                ProfileState::CannotLoad(reason) => Some(reason.as_str()),
            };
            (entry.profile.name(), reason)
        })
        .collect()
}

#[tokio::test]
async fn listing_shows_eh_profiles_sorted_ignoring_case_and_skips_the_rest() {
    let root = tempfile::tempdir().unwrap();
    profile_folder(root.path(), "orders", Some(READY_CONFIG));
    profile_folder(root.path(), "Billing", Some(READY_CONFIG));
    profile_folder(root.path(), "empty", None);
    profile_folder(root.path(), "other-tool", Some(r#"{"tool":"other"}"#));
    profile_folder(root.path(), "My Orders", Some(READY_CONFIG));
    std::fs::write(root.path().join("notes.txt"), "x").unwrap();

    let entries = Workspace::at(root.path().to_owned()).list().await.unwrap();

    assert_eq!(
        listed(&entries),
        [
            ("Billing", None),
            ("My Orders", Some(NOT_A_PROFILE_NAME)),
            ("orders", None),
        ]
    );
    assert_eq!(entries[2].profile.dir(), root.path().join("orders"));
}

#[tokio::test]
async fn a_profile_whose_config_is_invalid_is_listed_with_the_redacted_reason() {
    let root = tempfile::tempdir().unwrap();
    profile_folder(root.path(), "scratch", Some(&invalid_config()));

    let entries = Workspace::at(root.path().to_owned()).list().await.unwrap();

    let [entry] = entries.as_slice() else {
        panic!("expected one entry, got {entries:?}");
    };
    let ProfileState::CannotLoad(reason) = &entry.state else {
        panic!("expected a profile that cannot load, got {entry:?}");
    };
    assert!(
        reason.contains("is an eh config, but it is not valid"),
        "{reason}"
    );
    assert!(!reason.contains(PASTED_SECRET), "{reason}");
}

#[tokio::test]
async fn a_ready_profile_carries_its_config() {
    let root = tempfile::tempdir().unwrap();
    profile_folder(root.path(), "orders", Some(READY_CONFIG));

    let entries = Workspace::at(root.path().to_owned()).list().await.unwrap();

    let ProfileState::Ready(config) = &entries[0].state else {
        panic!("expected a ready profile, got {entries:?}");
    };
    assert_eq!(config.target.event_hub.as_deref(), Some("orders"));
}

#[cfg(unix)]
#[tokio::test]
async fn a_link_to_nowhere_is_skipped() {
    let root = tempfile::tempdir().unwrap();
    profile_folder(root.path(), "orders", Some(READY_CONFIG));
    std::os::unix::fs::symlink(root.path().join("nowhere"), root.path().join("dangling")).unwrap();

    let entries = Workspace::at(root.path().to_owned()).list().await.unwrap();

    assert_eq!(listed(&entries), [("orders", None)]);
}

#[test]
fn under_the_current_folder_a_data_folder_is_kept_as_given() {
    let absolute = tempfile::tempdir().unwrap();
    let home = ConfigHome::CurrentFolder(PathBuf::from("work"));

    assert_eq!(home.data_paths(".eh-data"), DataPaths::new(".eh-data"));
    assert_eq!(
        home.data_paths(absolute.path().to_str().unwrap()).base(),
        absolute.path()
    );
    assert_eq!(home.profile_name(), None);
    assert_eq!(home.dir(), Path::new("work"));
}

#[test]
fn under_a_profile_a_relative_data_folder_sits_inside_the_profile_folder() {
    let absolute = tempfile::tempdir().unwrap();
    let home = ConfigHome::Profile(Profile {
        name: "orders".to_owned(),
        dir: PathBuf::from("profiles").join("orders"),
    });

    assert_eq!(
        home.data_paths(".eh-data").base(),
        Path::new("profiles").join("orders").join(".eh-data")
    );
    assert_eq!(
        home.data_paths(absolute.path().to_str().unwrap()).base(),
        absolute.path()
    );
    assert_eq!(home.profile_name(), Some("orders"));
}

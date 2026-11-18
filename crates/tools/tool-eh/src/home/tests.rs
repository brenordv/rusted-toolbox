use super::*;
use rstest::rstest;
use std::path::Path;

const CONFIG: &str = r#"{"tool":"eh","schema_version":1,"target":{"namespace":"orders.servicebus.windows.net","event_hub":"orders","auth":{"method":"credential","credential":{"kind":"developer-tools"}}}}"#;
const OTHER_CONFIG: &str = r#"{"tool":"eh","schema_version":1,"target":{"namespace":"here.servicebus.windows.net","event_hub":"here","auth":{"method":"credential","credential":{"kind":"developer-tools"}}}}"#;
const FOREIGN: &str = r#"{"tool":"other"}"#;
const INVALID: &str = r#"{"tool":"eh","schema_version":1,"target":{}}"#;

/// A temp folder with a current folder `here` and a workspace `profiles`.
struct Fixture {
    root: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("here")).unwrap();
        Self { root }
    }

    fn here(&self) -> PathBuf {
        self.root.path().join("here")
    }

    fn workspace(&self) -> PathBuf {
        self.root.path().join("profiles")
    }

    fn write_config(dir: &Path, text: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(CONFIG_FILE_NAME), text).unwrap();
    }

    async fn load(
        &self,
        profile: Option<&str>,
        saving: Saving,
        read_config: bool,
    ) -> Result<LoadedHome> {
        let data = DataArgs {
            profile: profile.map(str::to_owned),
            ..DataArgs::default()
        };
        let root = self.workspace();
        load_home(&data, saving, read_config, self.here(), || {
            Ok(Workspace::at(root))
        })
        .await
    }
}

fn event_hub(recalled: Option<&ToolConfig>) -> Option<&str> {
    recalled.and_then(|config| config.target.event_hub.as_deref())
}

#[tokio::test]
async fn without_a_profile_the_current_folders_config_is_recalled() {
    let fixture = Fixture::new();
    Fixture::write_config(&fixture.here(), OTHER_CONFIG);

    let (home, recalled, source) = fixture.load(None, Saving::No, true).await.unwrap();

    assert_eq!(home, ConfigHome::CurrentFolder(fixture.here()));
    assert_eq!(event_hub(recalled.as_ref()), Some("here"));
    assert_eq!(source, ConfigSource::Loaded);
}

#[rstest]
#[case::missing(None, ConfigSource::Missing)]
#[case::foreign(Some(FOREIGN), ConfigSource::Foreign)]
#[tokio::test]
async fn without_a_profile_a_missing_or_foreign_config_recalls_nothing(
    #[case] config: Option<&str>,
    #[case] expected: ConfigSource,
) {
    let fixture = Fixture::new();
    if let Some(config) = config {
        Fixture::write_config(&fixture.here(), config);
    }

    let (_, recalled, source) = fixture.load(None, Saving::No, true).await.unwrap();

    assert!(recalled.is_none());
    assert_eq!(source, expected);
}

#[tokio::test]
async fn a_run_that_reads_no_config_never_opens_the_current_folders() {
    let fixture = Fixture::new();
    Fixture::write_config(&fixture.here(), INVALID);

    let (home, recalled, source) = fixture.load(None, Saving::No, false).await.unwrap();

    assert_eq!(home, ConfigHome::CurrentFolder(fixture.here()));
    assert!(recalled.is_none());
    assert_eq!(source, ConfigSource::Disabled);
}

#[rstest]
#[case::not_saving(Saving::No)]
#[case::saving(Saving::Yes)]
#[case::export(Saving::NotOffered)]
#[tokio::test]
async fn a_saved_profile_is_recalled_instead_of_the_current_folders_config(#[case] saving: Saving) {
    let fixture = Fixture::new();
    Fixture::write_config(&fixture.here(), OTHER_CONFIG);
    Fixture::write_config(&fixture.workspace().join("orders"), CONFIG);

    let (home, recalled, source) = fixture.load(Some("orders"), saving, true).await.unwrap();

    assert_eq!(home.profile_name(), Some("orders"));
    assert_eq!(home.dir(), fixture.workspace().join("orders"));
    assert_eq!(event_hub(recalled.as_ref()), Some("orders"));
    assert_eq!(source, ConfigSource::Loaded);
}

#[rstest]
#[case::no_folder(false)]
#[case::folder_without_config(true)]
#[tokio::test]
async fn a_missing_profile_is_a_new_one_when_saving(#[case] folder_exists: bool) {
    let fixture = Fixture::new();
    if folder_exists {
        std::fs::create_dir_all(fixture.workspace().join("orders")).unwrap();
    }

    let (home, recalled, source) = fixture
        .load(Some("orders"), Saving::Yes, true)
        .await
        .unwrap();

    assert_eq!(home.dir(), fixture.workspace().join("orders"));
    assert!(recalled.is_none());
    assert_eq!(source, ConfigSource::Missing);
}

#[tokio::test]
async fn a_missing_profile_without_saving_fails_saying_how_to_save_it() {
    let fixture = Fixture::new();

    let error = fixture
        .load(Some("orders"), Saving::No, true)
        .await
        .unwrap_err()
        .to_string();

    assert_eq!(
        error,
        format!(
            "No profile named orders in '{}'. `eh profile list` shows the saved ones; add \
             --save-config to save this run as profile orders",
            fixture.workspace().display()
        )
    );
    assert!(!fixture.workspace().exists());
}

#[tokio::test]
async fn a_missing_profile_for_a_run_that_cannot_save_leaves_out_the_save_hint() {
    let fixture = Fixture::new();

    let error = fixture
        .load(Some("orders"), Saving::NotOffered, true)
        .await
        .unwrap_err()
        .to_string();

    assert_eq!(
        error,
        format!(
            "No profile named orders in '{}'. `eh profile list` shows the saved ones",
            fixture.workspace().display()
        )
    );
}

#[tokio::test]
async fn a_profile_folder_with_a_foreign_config_is_refused() {
    let fixture = Fixture::new();
    Fixture::write_config(&fixture.workspace().join("orders"), FOREIGN);

    let error = fixture
        .load(Some("orders"), Saving::Yes, true)
        .await
        .unwrap_err()
        .to_string();

    assert!(
        error.ends_with("is not an eh config; a profile folder holds eh's config.json"),
        "{error}"
    );
}

#[tokio::test]
async fn a_profile_whose_config_is_invalid_fails_with_the_load_error() {
    let fixture = Fixture::new();
    Fixture::write_config(&fixture.workspace().join("orders"), INVALID);

    let error = fixture
        .load(Some("orders"), Saving::No, true)
        .await
        .unwrap_err()
        .to_string();

    assert!(
        error.contains("is an eh config, but it is not valid"),
        "{error}"
    );
}

#[rstest]
#[case::parent("../x", "A profile name uses letters")]
#[case::device("con", "con is a device name on Windows")]
#[tokio::test]
async fn a_bad_profile_name_fails_before_the_workspace_is_looked_up(
    #[case] name: &str,
    #[case] expected: &str,
) {
    let fixture = Fixture::new();
    let data = DataArgs {
        profile: Some(name.to_owned()),
        ..DataArgs::default()
    };

    let error = load_home(&data, Saving::Yes, true, fixture.here(), || {
        anyhow::bail!("the workspace was looked up")
    })
    .await
    .unwrap_err()
    .to_string();

    assert!(error.starts_with(expected), "{error}");
}

#[tokio::test]
async fn a_workspace_that_cannot_be_located_fails_the_profile_run() {
    let fixture = Fixture::new();
    let data = DataArgs {
        profile: Some("orders".to_owned()),
        ..DataArgs::default()
    };

    let error = load_home(&data, Saving::No, true, fixture.here(), || {
        anyhow::bail!("EH_WORKSPACE must be an absolute folder path")
    })
    .await
    .unwrap_err()
    .to_string();

    assert_eq!(error, "EH_WORKSPACE must be an absolute folder path");
}

#[tokio::test]
async fn a_workspace_that_cannot_be_listed_fails_the_profile_run() {
    let fixture = Fixture::new();
    std::fs::write(fixture.workspace(), "not a folder").unwrap();

    let error = fixture
        .load(Some("orders"), Saving::Yes, true)
        .await
        .unwrap_err()
        .to_string();

    assert!(error.contains("Failed to list the workspace"), "{error}");
}

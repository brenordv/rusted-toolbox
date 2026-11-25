//! The seal-gui settings file: the recipient book and convenience paths,
//! persisted as `config.json` in seal-gui's folder in the toolbox home
//! (`~/.rusted-toolbox/seal-gui`). The struct is structurally secret-free
//! (labels, public keys, paths, booleans; no field could hold key material),
//! and the file carries the house marker (`tool` + `schema_version`) so a
//! foreign file at the same path is ignored on load and never overwritten on
//! save.

use anyhow::Context as _;
use common_file_utils::adoption::{Shape, adopt};
use common_file_utils::atomic_write::write_via_temp;
use common_file_utils::permissions::create_dir_all_owner_only;
use common_utils::file_system::toolbox_home_with;
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// The marker value naming this tool in its config file.
pub const TOOL_MARKER: &str = "seal";

/// seal-gui's folder in the toolbox home: its package name, like every tool's.
const SETTINGS_FOLDER: &str = env!("CARGO_PKG_NAME");
const CONFIG_FILE: &str = "config.json";
/// What the settings file is called in the line that reports moving it.
const SETTINGS_DATA: &str = "seal-gui's settings";

/// The config layout this build reads and writes.
pub const SCHEMA_VERSION: u32 = 1;

/// Why the config file found at boot is not seal's to write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigNote {
    /// It carries another tool's marker.
    Foreign,
    /// It could not be read or parsed.
    Unreadable,
}

/// What the session may do with the config file: write it (and whether the last write
/// failed), or never, for the reason found at boot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigStatus {
    /// Nothing failed.
    Clean,
    /// The last save failed: its text.
    Failed(String),
    /// The file found at boot is not seal's; nothing is written this session.
    Refused(ConfigNote),
    /// The environment names no config folder; nothing is written this session.
    NoLocation,
}

impl ConfigStatus {
    /// The standing caption every tab shows for the status, when it has one.
    pub fn caption(&self) -> Option<String> {
        match self {
            Self::Clean => None,
            Self::Failed(text) => Some(format!("Settings could not be saved: {text}")),
            Self::Refused(ConfigNote::Foreign) => Some(
                "Another tool's config.json is in seal's settings folder; changes are not saved \
                 until it is moved."
                    .to_owned(),
            ),
            Self::Refused(ConfigNote::Unreadable) => Some(
                "config.json could not be read; changes are not saved until it is fixed or \
                 removed."
                    .to_owned(),
            ),
            Self::NoLocation => {
                Some("No settings folder was found; changes are not saved.".to_owned())
            }
        }
    }
}

/// One saved recipient: a human label for an `age1...` public key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipientEntry {
    pub label: String,
    pub public_key: String,
}

/// Everything the GUI persists between runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealConfig {
    /// Marker: which tool wrote this file. Defaulting it keeps a markerless
    /// file parseable, so it classifies as foreign instead of invalid.
    #[serde(default)]
    pub tool: String,
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub recipients: Vec<RecipientEntry>,
    #[serde(default)]
    pub last_identity_file: Option<PathBuf>,
    #[serde(default)]
    pub last_output_folder: Option<PathBuf>,
    #[serde(default)]
    pub overwrite_existing: bool,
}

impl Default for SealConfig {
    fn default() -> Self {
        Self {
            tool: TOOL_MARKER.to_string(),
            schema_version: SCHEMA_VERSION,
            recipients: Vec::new(),
            last_identity_file: None,
            last_output_folder: None,
            overwrite_existing: false,
        }
    }
}

impl SealConfig {
    fn owns_the_marker(&self) -> bool {
        self.tool == TOOL_MARKER && self.schema_version == SCHEMA_VERSION
    }
}

/// The folder that held `config.json` before seal-gui 2.0.0: the per-user OS config directory,
/// resolved from environment lookups alone so tests can inject them; `windows` picks the
/// platform branch for the same reason.
fn legacy_dir_for(windows: bool, get: &impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    if windows {
        get("APPDATA").map(|dir| PathBuf::from(dir).join(TOOL_MARKER))
    } else {
        get("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| get("HOME").map(|home| PathBuf::from(home).join(".config")))
            .map(|dir| dir.join(TOOL_MARKER))
    }
}

/// Where `config.json` was before seal-gui 2.0.0 on this platform (`%APPDATA%\seal`,
/// `$XDG_CONFIG_HOME/seal`, or `~/.config/seal`), or None when the environment names no such
/// folder.
pub fn legacy_config_path(get: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    legacy_dir_for(cfg!(windows), &get).map(|dir| dir.join(CONFIG_FILE))
}

/// `config.json` in seal-gui's folder in the toolbox home under `home`, or None when `home` is
/// None or not an absolute path.
pub fn config_path(home: Option<PathBuf>) -> Option<PathBuf> {
    toolbox_home_with(home).map(|root| root.join(SETTINGS_FOLDER).join(CONFIG_FILE))
}

/// What reading the config file found, before anything is logged.
enum Classified {
    Loaded(SealConfig),
    Missing,
    Foreign,
    /// The file could not be read: the error.
    Unreadable(String),
    /// The file did not parse: the error.
    Unparseable(String),
}

fn classify(path: &Path) -> Classified {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == ErrorKind::NotFound => return Classified::Missing,
        Err(error) => return Classified::Unreadable(error.to_string()),
    };
    match serde_json::from_str::<SealConfig>(&content) {
        Ok(config) if config.owns_the_marker() => Classified::Loaded(config),
        Ok(_) => Classified::Foreign,
        Err(error) => Classified::Unparseable(error.to_string()),
    }
}

/// The old settings file when it is seal's to move: it parses and carries seal's marker. A
/// file with another tool's marker is left alone silently, since a folder named `seal` can
/// belong to another program; one that cannot be read or parsed is left alone with a warning.
fn adoptable_legacy(path: PathBuf) -> Option<PathBuf> {
    match classify(&path) {
        Classified::Loaded(_) => Some(path),
        Classified::Missing | Classified::Foreign => None,
        Classified::Unreadable(error) | Classified::Unparseable(error) => {
            warn!(
                path = %path.display(),
                error = %error,
                "the settings file in seal-gui's old folder could not be read; it is left where \
                 it is"
            );
            None
        }
    }
}

/// What loading the config file found.
#[derive(Debug, PartialEq, Eq)]
pub enum LoadOutcome {
    Loaded(SealConfig),
    /// No file yet: first run.
    Missing,
    /// Parseable JSON whose marker names another tool or schema.
    Foreign,
    /// Unreadable or unparseable; treated like foreign, never overwritten.
    Invalid,
}

/// Reads and classifies the config file. Infallible by design: every bad
/// shape maps onto an outcome the boot path can act on.
pub fn load(path: &Path) -> LoadOutcome {
    match classify(path) {
        Classified::Loaded(config) => LoadOutcome::Loaded(config),
        Classified::Missing => LoadOutcome::Missing,
        Classified::Foreign => {
            warn!(path = %path.display(), "config file carries another tool's marker; leaving it alone");
            LoadOutcome::Foreign
        }
        Classified::Unreadable(error) => {
            warn!(path = %path.display(), error = %error, "config file could not be read");
            LoadOutcome::Invalid
        }
        Classified::Unparseable(error) => {
            warn!(path = %path.display(), error = %error, "config file did not parse; leaving it alone");
            LoadOutcome::Invalid
        }
    }
}

/// Writes the config through a temp file and rename, creating the config
/// directory (owner-only on Unix) on first save, so a crash never truncates the
/// recipient book.
///
/// # Errors
/// Fails when the directory cannot be created or the write or rename fails.
pub fn save(path: &Path, config: &SealConfig) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        create_dir_all_owner_only(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(config).context("failed to serialize the config")?;
    write_via_temp(path, |temp| {
        fs::write(temp, format!("{json}\n"))
            .with_context(|| format!("failed to write {}", temp.display()))
    })
}

/// The session's relationship with the config file: where it lives and whether saves are
/// allowed.
pub struct ConfigStore {
    path: Option<PathBuf>,
    poisoned: bool,
}

/// Everything boot-time config resolution produced.
pub struct Boot {
    pub store: ConfigStore,
    pub config: SealConfig,
    /// What the session may do with the file.
    pub status: ConfigStatus,
}

/// How one save ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveOutcome {
    Saved,
    /// The write failed: the error chain.
    Failed(String),
    /// The stored file is not ours; nothing was written.
    Refused,
    /// The environment names no config location.
    NoLocation,
}

impl ConfigStore {
    /// Resolves the path under the home folder `home` and loads the config once. A settings
    /// file only in the folder seal-gui used before 2.0.0 (found through the environment
    /// lookups `get`) is moved to the new path first when it is seal's, and the move, or why it
    /// did not happen, is logged; when it cannot be moved the old file stays in use. A foreign
    /// or unreadable file poisons the path: saves are refused for the whole session rather
    /// than overwriting a file seal does not own.
    pub fn boot(home: Option<PathBuf>, get: impl Fn(&str) -> Option<OsString>) -> Boot {
        let Some(target) = config_path(home) else {
            warn!("no home folder was found; settings will not persist");
            return Boot {
                store: ConfigStore {
                    path: None,
                    poisoned: false,
                },
                config: SealConfig::default(),
                status: ConfigStatus::NoLocation,
            };
        };
        let legacy = legacy_config_path(get).and_then(adoptable_legacy);
        let adoption = adopt(legacy.as_deref(), &target, Shape::File);
        adoption.report(SETTINGS_DATA, &target);
        let path = adoption.path_in_use(&target);

        let (config, status) = match load(&path) {
            LoadOutcome::Loaded(config) => (config, ConfigStatus::Clean),
            LoadOutcome::Missing => (SealConfig::default(), ConfigStatus::Clean),
            LoadOutcome::Foreign => (
                SealConfig::default(),
                ConfigStatus::Refused(ConfigNote::Foreign),
            ),
            LoadOutcome::Invalid => (
                SealConfig::default(),
                ConfigStatus::Refused(ConfigNote::Unreadable),
            ),
        };
        let poisoned = match status {
            ConfigStatus::Refused(_) => true,
            ConfigStatus::Clean | ConfigStatus::Failed(_) | ConfigStatus::NoLocation => false,
        };

        Boot {
            store: ConfigStore {
                path: Some(path),
                poisoned,
            },
            config,
            status,
        }
    }

    /// Writes the config unless the path is poisoned or absent. A failed write is logged at
    /// `warn` with the path, every time.
    pub fn flush(&self, config: &SealConfig) -> SaveOutcome {
        let Some(path) = self.path.as_ref() else {
            return SaveOutcome::NoLocation;
        };
        if self.poisoned {
            debug!("config save refused: the stored file is not ours");
            return SaveOutcome::Refused;
        }
        match save(path, config) {
            Ok(()) => SaveOutcome::Saved,
            Err(error) => {
                let rendered = format!("{error:#}");
                warn!(path = %path.display(), error = %rendered, "config save failed");
                SaveOutcome::Failed(rendered)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn env_with(pairs: &[(&str, &Path)]) -> impl Fn(&str) -> Option<OsString> {
        let owned: Vec<(String, OsString)> = pairs
            .iter()
            .map(|(name, path)| ((*name).to_string(), path.as_os_str().to_os_string()))
            .collect();
        move |name: &str| {
            owned
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
        }
    }

    /// Points every platform's lookup at the same base dir, so boot tests
    /// run identically on Windows and Unix.
    fn env_all(base: &Path) -> impl Fn(&str) -> Option<OsString> {
        let value = base.as_os_str().to_os_string();
        move |name: &str| matches!(name, "APPDATA" | "XDG_CONFIG_HOME").then(|| value.clone())
    }

    /// The settings file's path in seal-gui's folder under `home`.
    fn new_path(home: &Path) -> PathBuf {
        home.join(".rusted-toolbox")
            .join("seal-gui")
            .join("config.json")
    }

    /// The old settings file `env_all(base)` points at.
    fn old_path(base: &Path) -> PathBuf {
        base.join("seal").join("config.json")
    }

    fn write_at(path: &Path, content: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn book_with(label: &str) -> SealConfig {
        SealConfig {
            recipients: vec![RecipientEntry {
                label: label.to_string(),
                public_key: "age1example".to_string(),
            }],
            ..SealConfig::default()
        }
    }

    #[test]
    fn the_settings_file_is_in_seal_guis_folder_in_the_toolbox_home() {
        let home = tempfile::tempdir().unwrap();

        let path = config_path(Some(home.path().to_path_buf()));

        assert_eq!(path, Some(new_path(home.path())));
    }

    #[rstest]
    #[case::no_home(None)]
    #[case::relative_home(Some(PathBuf::from("home")))]
    fn with_no_usable_home_there_is_no_settings_file(#[case] home: Option<PathBuf>) {
        assert_eq!(config_path(home), None);
    }

    #[test]
    fn the_old_windows_place_uses_appdata_or_nothing() {
        let base = Path::new("roaming");

        let dir = legacy_dir_for(true, &env_with(&[("APPDATA", base)]));
        assert_eq!(dir, Some(Path::new("roaming").join("seal")));

        assert_eq!(legacy_dir_for(true, &env_with(&[])), None);
    }

    #[test]
    fn the_old_unix_place_prefers_xdg_then_falls_back_to_home() {
        let xdg = Path::new("xdg");
        let home = Path::new("home");

        let preferred = legacy_dir_for(
            false,
            &env_with(&[("XDG_CONFIG_HOME", xdg), ("HOME", home)]),
        );
        assert_eq!(preferred, Some(Path::new("xdg").join("seal")));

        let fallback = legacy_dir_for(false, &env_with(&[("HOME", home)]));
        assert_eq!(
            fallback,
            Some(Path::new("home").join(".config").join("seal"))
        );

        assert_eq!(legacy_dir_for(false, &env_with(&[])), None);
    }

    #[test]
    fn boot_moves_seals_old_settings_to_the_new_folder_and_loads_them() {
        let home = tempfile::tempdir().unwrap();
        let base = tempfile::tempdir().unwrap();
        let old = old_path(base.path());
        save(&old, &book_with("Alice")).unwrap();

        let boot = ConfigStore::boot(Some(home.path().to_path_buf()), env_all(base.path()));

        assert_eq!(boot.config, book_with("Alice"));
        assert_eq!(boot.status, ConfigStatus::Clean);
        assert_eq!(
            load(&new_path(home.path())),
            LoadOutcome::Loaded(book_with("Alice"))
        );
        assert!(!old.exists());
    }

    #[test]
    fn boot_leaves_another_tools_old_file_alone_and_starts_fresh() {
        let home = tempfile::tempdir().unwrap();
        let base = tempfile::tempdir().unwrap();
        let old = old_path(base.path());
        let foreign = r#"{"tool": "eh", "schema_version": 1}"#;
        write_at(&old, foreign);

        let boot = ConfigStore::boot(Some(home.path().to_path_buf()), env_all(base.path()));

        assert_eq!(boot.config, SealConfig::default());
        assert_eq!(boot.status, ConfigStatus::Clean);
        assert_eq!(fs::read_to_string(&old).unwrap(), foreign);
        assert!(!new_path(home.path()).exists());
    }

    #[test]
    fn boot_leaves_an_unparseable_old_file_alone_and_starts_fresh() {
        let home = tempfile::tempdir().unwrap();
        let base = tempfile::tempdir().unwrap();
        let old = old_path(base.path());
        write_at(&old, "garbage");

        let boot = ConfigStore::boot(Some(home.path().to_path_buf()), env_all(base.path()));

        assert_eq!(boot.config, SealConfig::default());
        assert_eq!(boot.status, ConfigStatus::Clean);
        assert_eq!(fs::read_to_string(&old).unwrap(), "garbage");
        assert!(!new_path(home.path()).exists());
    }

    #[test]
    fn boot_leaves_an_old_settings_path_that_cannot_be_read_alone() {
        let home = tempfile::tempdir().unwrap();
        let base = tempfile::tempdir().unwrap();
        let old = old_path(base.path());
        fs::create_dir_all(&old).unwrap();

        let boot = ConfigStore::boot(Some(home.path().to_path_buf()), env_all(base.path()));

        assert_eq!(boot.config, SealConfig::default());
        assert_eq!(boot.status, ConfigStatus::Clean);
        assert!(old.is_dir());
        assert!(!new_path(home.path()).exists());
    }

    #[test]
    fn boot_keeps_using_old_settings_that_cannot_move_and_saves_there() {
        let home = tempfile::tempdir().unwrap();
        let base = tempfile::tempdir().unwrap();
        let old = old_path(base.path());
        save(&old, &book_with("Alice")).unwrap();
        let toolbox = home.path().join(".rusted-toolbox");
        fs::create_dir_all(&toolbox).unwrap();
        fs::write(
            toolbox.join("seal-gui"),
            b"a file where seal-gui's folder goes",
        )
        .unwrap();

        let boot = ConfigStore::boot(Some(home.path().to_path_buf()), env_all(base.path()));
        let saved = boot.store.flush(&book_with("Bea"));

        assert_eq!(boot.config, book_with("Alice"));
        assert_eq!(saved, SaveOutcome::Saved);
        assert_eq!(load(&old), LoadOutcome::Loaded(book_with("Bea")));
    }

    #[cfg(windows)]
    #[test]
    fn boot_over_old_settings_held_open_uses_the_new_file_and_the_next_boot_loads_its_saves() {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_SHARE_READ_WRITE: u32 = 0x1 | 0x2;
        let home = tempfile::tempdir().unwrap();
        let base = tempfile::tempdir().unwrap();
        let old = old_path(base.path());
        save(&old, &book_with("Alice")).unwrap();
        let held = fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ_WRITE)
            .open(&old)
            .unwrap();

        let boot = ConfigStore::boot(Some(home.path().to_path_buf()), env_all(base.path()));
        drop(held);
        let saved = boot.store.flush(&book_with("Bea"));
        let next_boot = ConfigStore::boot(Some(home.path().to_path_buf()), env_all(base.path()));

        assert_eq!(boot.config, book_with("Alice"));
        assert_eq!(saved, SaveOutcome::Saved);
        assert_eq!(next_boot.config, book_with("Bea"));
        assert_eq!(
            load(&new_path(home.path())),
            LoadOutcome::Loaded(book_with("Bea"))
        );
        assert_eq!(load(&old), LoadOutcome::Loaded(book_with("Alice")));
    }

    #[test]
    fn boot_with_settings_in_both_places_loads_the_new_ones() {
        let home = tempfile::tempdir().unwrap();
        let base = tempfile::tempdir().unwrap();
        let old = old_path(base.path());
        save(&old, &book_with("Alice")).unwrap();
        save(&new_path(home.path()), &book_with("Bea")).unwrap();

        let boot = ConfigStore::boot(Some(home.path().to_path_buf()), env_all(base.path()));

        assert_eq!(boot.config, book_with("Bea"));
        assert_eq!(load(&old), LoadOutcome::Loaded(book_with("Alice")));
    }

    #[test]
    fn boot_with_no_home_moves_nothing_and_saves_nowhere() {
        let base = tempfile::tempdir().unwrap();
        let old = old_path(base.path());
        save(&old, &book_with("Alice")).unwrap();

        let boot = ConfigStore::boot(None, env_all(base.path()));

        assert_eq!(boot.status, ConfigStatus::NoLocation);
        assert_eq!(load(&old), LoadOutcome::Loaded(book_with("Alice")));
    }

    #[test]
    fn save_then_load_round_trips_at_the_resolved_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = config_path(Some(dir.path().to_path_buf())).unwrap();
        let config = SealConfig {
            recipients: vec![RecipientEntry {
                label: "Alice".to_string(),
                public_key: "age1example".to_string(),
            }],
            last_identity_file: Some(PathBuf::from("keys/id.txt")),
            last_output_folder: Some(PathBuf::from("safe")),
            overwrite_existing: true,
            ..SealConfig::default()
        };

        save(&path, &config).unwrap();

        assert_eq!(load(&path), LoadOutcome::Loaded(config));
    }

    #[test]
    fn a_missing_file_loads_as_missing() {
        let dir = tempfile::tempdir().unwrap();

        assert_eq!(load(&dir.path().join("config.json")), LoadOutcome::Missing);
    }

    #[rstest]
    #[case::another_tool(r#"{"tool": "eh", "schema_version": 1}"#)]
    #[case::missing_marker(r#"{"recipients": []}"#)]
    #[case::wrong_schema(r#"{"tool": "seal", "schema_version": 2}"#)]
    fn a_foreign_marker_loads_as_foreign(#[case] content: &str) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        fs::write(&path, content).unwrap();

        assert_eq!(load(&path), LoadOutcome::Foreign);
    }

    #[test]
    fn unparseable_json_loads_as_invalid() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        fs::write(&path, "not json at all").unwrap();

        assert_eq!(load(&path), LoadOutcome::Invalid);
    }

    #[test]
    fn boot_over_a_foreign_file_poisons_saves_and_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let path = new_path(dir.path());
        let foreign = r#"{"tool": "eh", "schema_version": 1}"#;
        write_at(&path, foreign);

        let boot = ConfigStore::boot(Some(dir.path().to_path_buf()), env_with(&[]));

        assert_eq!(boot.config, SealConfig::default());
        assert_eq!(boot.status, ConfigStatus::Refused(ConfigNote::Foreign));
        assert_eq!(
            boot.store.flush(&SealConfig::default()),
            SaveOutcome::Refused
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), foreign);
    }

    #[test]
    fn boot_over_an_unparseable_file_poisons_saves_with_its_own_note() {
        let dir = tempfile::tempdir().unwrap();
        let path = new_path(dir.path());
        write_at(&path, "garbage");

        let boot = ConfigStore::boot(Some(dir.path().to_path_buf()), env_with(&[]));

        assert_eq!(boot.status, ConfigStatus::Refused(ConfigNote::Unreadable));
        assert_eq!(
            boot.store.flush(&SealConfig::default()),
            SaveOutcome::Refused
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "garbage");
    }

    #[test]
    fn s3_boot_with_no_config_location_still_runs_and_saves_nowhere() {
        let boot = ConfigStore::boot(None, env_with(&[]));

        assert_eq!(boot.config, SealConfig::default());
        assert_eq!(boot.status, ConfigStatus::NoLocation);
        assert_eq!(
            boot.store.flush(&SealConfig::default()),
            SaveOutcome::NoLocation
        );
    }

    #[test]
    fn boot_on_a_fresh_profile_saves_cleanly_and_reloads() {
        let dir = tempfile::tempdir().unwrap();
        let home = Some(dir.path().to_path_buf());

        let boot = ConfigStore::boot(home.clone(), env_with(&[]));
        assert_eq!(boot.status, ConfigStatus::Clean);

        let mut config = SealConfig::default();
        config.recipients.push(RecipientEntry {
            label: "Bea".to_string(),
            public_key: "age1whatever".to_string(),
        });
        assert_eq!(boot.store.flush(&config), SaveOutcome::Saved);

        let reloaded = ConfigStore::boot(home, env_with(&[]));
        assert_eq!(reloaded.config, config);
    }

    /// A store whose config folder is a file, so every save fails.
    fn failing_store(dir: &Path) -> ConfigStore {
        let blocker = dir.join("seal");
        fs::write(&blocker, b"in the way").unwrap();
        ConfigStore {
            path: Some(blocker.join("config.json")),
            poisoned: false,
        }
    }

    #[test]
    fn s1_every_failed_save_answers_failed_with_the_error() {
        let dir = tempfile::tempdir().unwrap();
        let store = failing_store(dir.path());
        let config = SealConfig::default();

        let first = store.flush(&config);
        let second = store.flush(&config);

        assert!(matches!(&first, SaveOutcome::Failed(text) if text.contains("failed to create")));
        assert_eq!(second, first);
    }

    #[test]
    fn s2_a_poisoned_store_refuses_and_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let store = ConfigStore {
            path: Some(path.clone()),
            poisoned: true,
        };

        assert_eq!(store.flush(&SealConfig::default()), SaveOutcome::Refused);
        assert!(!path.exists());
    }

    #[test]
    fn the_serialized_shape_holds_exactly_the_documented_secret_free_fields() {
        let json = serde_json::to_value(SealConfig::default()).unwrap();

        let object = json.as_object().unwrap();
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "last_identity_file",
                "last_output_folder",
                "overwrite_existing",
                "recipients",
                "schema_version",
                "tool",
            ]
        );
    }

    #[test]
    fn the_canonical_fixture_round_trips() {
        let fixture = r#"{
            "tool": "seal",
            "schema_version": 1,
            "recipients": [{"label": "Alice", "public_key": "age1abc"}],
            "last_identity_file": "keys/id.txt",
            "last_output_folder": "safe",
            "overwrite_existing": true
        }"#;

        let parsed: SealConfig = serde_json::from_str(fixture).unwrap();

        assert_eq!(parsed.tool, TOOL_MARKER);
        assert_eq!(parsed.schema_version, SCHEMA_VERSION);
        assert_eq!(parsed.recipients.len(), 1);
        assert_eq!(parsed.recipients[0].label, "Alice");
        assert_eq!(
            parsed.last_identity_file,
            Some(PathBuf::from("keys/id.txt"))
        );
        assert_eq!(parsed.last_output_folder, Some(PathBuf::from("safe")));
        assert!(parsed.overwrite_existing);

        let reparsed: SealConfig =
            serde_json::from_str(&serde_json::to_string(&parsed).unwrap()).unwrap();
        assert_eq!(reparsed, parsed);
    }
}

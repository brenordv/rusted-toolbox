//! The seal-gui settings file: the recipient book and convenience paths,
//! persisted as `config.json` in the per-user OS config directory. The
//! struct is structurally secret-free (labels, public keys, paths, booleans;
//! no field could hold key material), and the file carries the house marker
//! (`tool` + `schema_version`) so a foreign file at the same path is ignored
//! on load and never overwritten on save.

use anyhow::Context as _;
use common_file_utils::atomic_write::write_via_temp;
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// The marker value naming this tool in its config file.
pub const TOOL_MARKER: &str = "seal";

/// The config layout this build reads and writes.
pub const SCHEMA_VERSION: u32 = 1;

/// Status-bar note shown when another tool's config occupies seal's path.
pub const NOTE_FOREIGN_CONFIG: &str = "another tool's config.json sits in the seal config \
     folder; changes will not be saved until it is moved";

/// Status-bar note shown when the config exists but cannot be read.
pub const NOTE_UNREADABLE_CONFIG: &str =
    "config.json could not be read; changes will not be saved until it is fixed or removed";

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

/// Resolves the per-user config directory from environment lookups alone, so
/// tests can inject them; `windows` picks the platform branch for the same
/// reason.
fn config_dir_for(windows: bool, get: &impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    if windows {
        get("APPDATA").map(|dir| PathBuf::from(dir).join(TOOL_MARKER))
    } else {
        get("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| get("HOME").map(|home| PathBuf::from(home).join(".config")))
            .map(|dir| dir.join(TOOL_MARKER))
    }
}

/// The full path of `config.json` on this platform, or None when the
/// environment names no config location.
pub fn config_path(get: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    config_dir_for(cfg!(windows), &get).map(|dir| dir.join("config.json"))
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
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == ErrorKind::NotFound => return LoadOutcome::Missing,
        Err(error) => {
            warn!(path = %path.display(), error = %error, "config file could not be read");
            return LoadOutcome::Invalid;
        }
    };

    match serde_json::from_str::<SealConfig>(&content) {
        Ok(config) if config.owns_the_marker() => LoadOutcome::Loaded(config),
        Ok(_) => {
            warn!(path = %path.display(), "config file carries another tool's marker; leaving it alone");
            LoadOutcome::Foreign
        }
        Err(error) => {
            warn!(path = %path.display(), error = %error, "config file did not parse; leaving it alone");
            LoadOutcome::Invalid
        }
    }
}

/// Writes the config through a temp file and rename, creating the config
/// directory on first save, so a crash never truncates the recipient book.
///
/// # Errors
/// Fails when the directory cannot be created or the write or rename fails.
pub fn save(path: &Path, config: &SealConfig) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(config).context("failed to serialize the config")?;
    write_via_temp(path, |temp| {
        fs::write(temp, format!("{json}\n"))
            .with_context(|| format!("failed to write {}", temp.display()))
    })
}

/// The session's relationship with the config file: where it lives, whether
/// saves are allowed, and whether a save failure was already reported.
pub struct ConfigStore {
    path: Option<PathBuf>,
    poisoned: bool,
    save_warned: bool,
}

/// Everything boot-time config resolution produced.
pub struct Boot {
    pub store: ConfigStore,
    pub config: SealConfig,
    /// A standing status-bar note (foreign or unreadable config file).
    pub note: Option<&'static str>,
}

/// How one flush attempt ended.
#[derive(Debug, PartialEq, Eq)]
pub enum FlushOutcome {
    Saved,
    /// The stored file is not ours; nothing was written.
    Refused,
    /// The environment names no config location.
    NoLocation,
    /// First save failure of the session; worth a toast.
    FailedFirst(String),
    /// A repeat failure, already reported once.
    FailedAgain,
}

impl ConfigStore {
    /// Resolves the path and loads the config once. A foreign or unreadable
    /// file poisons the path: saves are refused for the whole session rather
    /// than overwriting a file seal does not own.
    pub fn boot(get: impl Fn(&str) -> Option<OsString>) -> Boot {
        let Some(path) = config_path(get) else {
            warn!("no config location in the environment; settings will not persist");
            return Boot {
                store: ConfigStore {
                    path: None,
                    poisoned: false,
                    save_warned: false,
                },
                config: SealConfig::default(),
                note: None,
            };
        };

        let (config, poisoned, note) = match load(&path) {
            LoadOutcome::Loaded(config) => (config, false, None),
            LoadOutcome::Missing => (SealConfig::default(), false, None),
            LoadOutcome::Foreign => (SealConfig::default(), true, Some(NOTE_FOREIGN_CONFIG)),
            LoadOutcome::Invalid => (SealConfig::default(), true, Some(NOTE_UNREADABLE_CONFIG)),
        };

        Boot {
            store: ConfigStore {
                path: Some(path),
                poisoned,
                save_warned: false,
            },
            config,
            note,
        }
    }

    /// Writes the config unless the path is poisoned or absent. The first
    /// save failure of the session is reported loudly (warn here, a toast in
    /// the app); repeats stay quiet so a read-only profile cannot toast on
    /// every tab switch.
    pub fn flush(&mut self, config: &SealConfig) -> FlushOutcome {
        let Some(path) = self.path.as_ref() else {
            return FlushOutcome::NoLocation;
        };
        if self.poisoned {
            debug!("config save refused: the stored file is not ours");
            return FlushOutcome::Refused;
        }
        match save(path, config) {
            Ok(()) => FlushOutcome::Saved,
            Err(error) if self.save_warned => {
                debug!(error = format!("{error:#}"), "config save failed again");
                FlushOutcome::FailedAgain
            }
            Err(error) => {
                self.save_warned = true;
                let rendered = format!("{error:#}");
                warn!(error = %rendered, "config save failed");
                FlushOutcome::FailedFirst(rendered)
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

    #[test]
    fn windows_resolution_uses_appdata_or_nothing() {
        let base = Path::new("roaming");

        let dir = config_dir_for(true, &env_with(&[("APPDATA", base)]));
        assert_eq!(dir, Some(Path::new("roaming").join("seal")));

        assert_eq!(config_dir_for(true, &env_with(&[])), None);
    }

    #[test]
    fn unix_resolution_prefers_xdg_then_falls_back_to_home() {
        let xdg = Path::new("xdg");
        let home = Path::new("home");

        let preferred = config_dir_for(
            false,
            &env_with(&[("XDG_CONFIG_HOME", xdg), ("HOME", home)]),
        );
        assert_eq!(preferred, Some(Path::new("xdg").join("seal")));

        let fallback = config_dir_for(false, &env_with(&[("HOME", home)]));
        assert_eq!(
            fallback,
            Some(Path::new("home").join(".config").join("seal"))
        );

        assert_eq!(config_dir_for(false, &env_with(&[])), None);
    }

    #[test]
    fn save_then_load_round_trips_at_the_resolved_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = config_path(env_all(dir.path())).unwrap();
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
        let env = env_all(dir.path());
        let path = config_path(&env).unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let foreign = r#"{"tool": "eh", "schema_version": 1}"#;
        fs::write(&path, foreign).unwrap();

        let mut boot = ConfigStore::boot(&env);

        assert_eq!(boot.config, SealConfig::default());
        assert_eq!(boot.note, Some(NOTE_FOREIGN_CONFIG));
        assert_eq!(
            boot.store.flush(&SealConfig::default()),
            FlushOutcome::Refused
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), foreign);
    }

    #[test]
    fn boot_over_an_unparseable_file_poisons_saves_with_its_own_note() {
        let dir = tempfile::tempdir().unwrap();
        let env = env_all(dir.path());
        let path = config_path(&env).unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "garbage").unwrap();

        let mut boot = ConfigStore::boot(&env);

        assert_eq!(boot.note, Some(NOTE_UNREADABLE_CONFIG));
        assert_eq!(
            boot.store.flush(&SealConfig::default()),
            FlushOutcome::Refused
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "garbage");
    }

    #[test]
    fn boot_with_no_config_location_still_runs_and_refuses_nothing_loudly() {
        let mut boot = ConfigStore::boot(env_with(&[]));

        assert_eq!(boot.config, SealConfig::default());
        assert_eq!(boot.note, None);
        assert_eq!(
            boot.store.flush(&SealConfig::default()),
            FlushOutcome::NoLocation
        );
    }

    #[test]
    fn boot_on_a_fresh_profile_saves_cleanly_and_reloads() {
        let dir = tempfile::tempdir().unwrap();
        let env = env_all(dir.path());

        let mut boot = ConfigStore::boot(&env);
        assert_eq!(boot.note, None);

        let mut config = SealConfig::default();
        config.recipients.push(RecipientEntry {
            label: "Bea".to_string(),
            public_key: "age1whatever".to_string(),
        });
        assert_eq!(boot.store.flush(&config), FlushOutcome::Saved);

        let reloaded = ConfigStore::boot(&env);
        assert_eq!(reloaded.config, config);
    }

    #[test]
    fn save_failures_report_once_then_stay_quiet() {
        let dir = tempfile::tempdir().unwrap();
        // The config path's parent is a file, so create_dir_all must fail.
        let blocker = dir.path().join("seal");
        fs::write(&blocker, b"in the way").unwrap();
        let mut store = ConfigStore {
            path: Some(blocker.join("config.json")),
            poisoned: false,
            save_warned: false,
        };
        let config = SealConfig::default();

        let first = store.flush(&config);
        let second = store.flush(&config);

        assert!(matches!(first, FlushOutcome::FailedFirst(_)));
        assert_eq!(second, FlushOutcome::FailedAgain);
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

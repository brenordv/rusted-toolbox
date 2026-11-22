//! speak's settings: where the speech server is and what each request carries.

use anyhow::{Context, Result, anyhow, bail};
use common_serialization_utils::{load_json_file_to_object, save_object_to_json_file};
use common_utils::file_system::get_app_sub_folder;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use url::Url;

/// The folder under the user's home that holds each tool's own settings folder.
const SETTINGS_ROOT: &str = "rusted-toolbox";
/// speak's settings folder inside [`SETTINGS_ROOT`].
const SETTINGS_FOLDER: &str = "tool-speak";
const CONFIG_FILE: &str = "config.json";

/// The port the first run offers.
pub const DEFAULT_PORT: u16 = 10157;
/// The path of the speech endpoint on the server.
pub const SPEECH_PATH: &str = "/v1/audio/speech";
pub const DEFAULT_MODEL: &str = "chatterbox-turbo";
pub const DEFAULT_VOICE: &str = "sj-clip-001.wav";
/// The vocal reactions the default model understands inline in the text.
pub const DEFAULT_REACTIONS: [&str; 9] = [
    "[laugh]",
    "[chuckle]",
    "[sigh]",
    "[gasp]",
    "[cough]",
    "[clear throat]",
    "[sniff]",
    "[groan]",
    "[shush]",
];

const METHOD_POST: &str = "POST";
const METHOD_PUT: &str = "PUT";
const MODEL_KEY: &str = "model";
const VOICE_KEY: &str = "voice";
const INPUT_KEY: &str = "input";

/// speak's settings as `config.json` holds them. `payload` is sent as the request body with
/// `input` set to the text, so a field this tool does not name (`speed`, `seed`) is kept and
/// sent as written.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeakConfig {
    /// The speech endpoint; empty until a server is set.
    pub url: String,
    /// `POST` or `PUT`.
    pub request_method: String,
    /// The reaction tags the model understands inline in the text.
    pub supported_vocal_reactions: Vec<String>,
    /// The request body without `input`.
    pub payload: Map<String, Value>,
}

impl Default for SpeakConfig {
    fn default() -> Self {
        let mut payload = Map::new();
        payload.insert(MODEL_KEY.to_owned(), Value::from(DEFAULT_MODEL));
        payload.insert(VOICE_KEY.to_owned(), Value::from(DEFAULT_VOICE));
        Self {
            url: String::new(),
            request_method: METHOD_POST.to_owned(),
            supported_vocal_reactions: DEFAULT_REACTIONS.map(str::to_owned).to_vec(),
            payload,
        }
    }
}

/// Where speak keeps its settings: `~/.rusted-toolbox/tool-speak/config.json`.
pub fn default_config_path() -> PathBuf {
    get_app_sub_folder(SETTINGS_ROOT, SETTINGS_FOLDER).join(CONFIG_FILE)
}

/// Loads the settings at `path`; `Ok(None)` when the file does not exist.
///
/// # Errors
/// When the file exists and cannot be read or is not the settings JSON; the error names the
/// path.
pub async fn load(path: &Path) -> Result<Option<SpeakConfig>> {
    match load_json_file_to_object::<SpeakConfig>(path).await {
        Ok(config) => Ok(Some(config)),
        Err(error) if is_not_found(&error) => Ok(None),
        Err(error) => Err(error.context(format!(
            "speak's settings file {} could not be loaded",
            path.display()
        ))),
    }
}

fn is_not_found(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<std::io::Error>()
        .is_some_and(|io| io.kind() == ErrorKind::NotFound)
}

impl SpeakConfig {
    /// The default settings pointed at the server at `address`, listening on `port`.
    ///
    /// `address` is a host name or IP address, alone or after `http://` or `https://`
    /// (`http://` when there is none).
    ///
    /// # Errors
    /// When `address` is blank, holds a space or a control character, or carries anything
    /// besides the host: a path, a query, a port, or a user name.
    pub fn for_server(address: &str, port: u16) -> Result<Self> {
        let address = address.trim();
        if address.is_empty() {
            bail!("The server address is blank");
        }
        if address.chars().any(char::is_whitespace) {
            bail!("The server address cannot hold spaces");
        }
        if address.chars().any(char::is_control) {
            bail!("The server address cannot hold control characters");
        }
        let with_scheme = if address.contains("://") {
            address.to_owned()
        } else {
            format!("http://{address}")
        };
        let parsed = Url::parse(&with_scheme)
            .map_err(|error| anyhow!(error).context("The server address is not valid"))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            bail!("The server address must start with http:// or https://, or have no scheme");
        }
        if !parsed.username().is_empty() || parsed.password().is_some() {
            bail!("The server address cannot hold a user name or password");
        }
        if parsed.query().is_some() {
            bail!("The server address cannot hold a query; give only the host");
        }
        if parsed.path() != "/" || parsed.fragment().is_some() {
            bail!("The server address cannot hold a path; give only the host");
        }
        // An http or https URL that parsed has a host: the parser refuses an empty one.
        let host = parsed.host_str().unwrap_or_default();
        let given_host = with_scheme
            .split_once("://")
            .map_or("", |(_, rest)| rest)
            .trim_end_matches('/');
        if !given_host.eq_ignore_ascii_case(host) {
            bail!("The server address cannot hold a port; the port has its own question");
        }
        Ok(Self {
            url: format!("{}://{host}:{port}{SPEECH_PATH}", parsed.scheme()),
            ..Self::default()
        })
    }

    /// Saves the settings to `path`, creating its folder when it is missing.
    ///
    /// # Errors
    /// When the file cannot be written; the error names the path.
    pub fn save(&self, path: &Path) -> Result<()> {
        save_object_to_json_file(path, self)
            .with_context(|| format!("speak's settings could not be saved to {}", path.display()))
    }

    /// Checks that a request can be built from these settings.
    ///
    /// # Errors
    /// When the URL is not an `http` or `https` URL with a host and no user name or password,
    /// the method is not `POST` or `PUT`, or the payload's `model` or `voice` is not a
    /// non-blank text. No error text repeats the URL.
    pub fn validate(&self) -> Result<()> {
        self.endpoint()?;
        self.method()?;
        self.payload_text(MODEL_KEY)?;
        self.payload_text(VOICE_KEY)?;
        Ok(())
    }

    /// The speech endpoint as a URL.
    ///
    /// # Errors
    /// When the URL is blank or malformed (an `http` or `https` URL with no host is
    /// malformed), is not `http` or `https`, or holds a user name or password. No error text
    /// repeats the URL.
    pub fn endpoint(&self) -> Result<Url> {
        if self.url.trim().is_empty() {
            bail!("The speech server URL is not set");
        }
        // The parse error is dropped on purpose: only its kind is public, and the message
        // must not grow a copy of a URL that may hold a password.
        let Ok(url) = Url::parse(self.url.trim()) else {
            bail!("The speech server URL is not a valid URL");
        };
        if !matches!(url.scheme(), "http" | "https") {
            bail!("The speech server URL must start with http:// or https://");
        }
        if !url.username().is_empty() || url.password().is_some() {
            bail!("The speech server URL cannot hold a user name or password");
        }
        Ok(url)
    }

    /// The request method.
    ///
    /// # Errors
    /// When it is neither `POST` nor `PUT`.
    pub fn method(&self) -> Result<Method> {
        match self.request_method.as_str() {
            METHOD_POST => Ok(Method::POST),
            METHOD_PUT => Ok(Method::PUT),
            _ => bail!("The request method must be POST or PUT"),
        }
    }

    /// The payload's model, when it is a text.
    pub fn model(&self) -> Option<&str> {
        self.payload.get(MODEL_KEY).and_then(Value::as_str)
    }

    /// The payload's voice, when it is a text.
    pub fn voice(&self) -> Option<&str> {
        self.payload.get(VOICE_KEY).and_then(Value::as_str)
    }

    /// Replaces the payload's model.
    pub fn set_model(&mut self, model: &str) {
        self.payload
            .insert(MODEL_KEY.to_owned(), Value::from(model));
    }

    /// Replaces the payload's voice.
    pub fn set_voice(&mut self, voice: &str) {
        self.payload
            .insert(VOICE_KEY.to_owned(), Value::from(voice));
    }

    /// The request body: the payload with `input` set to `text`, replacing any `input` the
    /// payload carries.
    pub fn request_body(&self, text: &str) -> Map<String, Value> {
        let mut body = self.payload.clone();
        body.insert(INPUT_KEY.to_owned(), Value::from(text));
        body
    }

    fn payload_text(&self, key: &str) -> Result<&str> {
        match self.payload.get(key).and_then(Value::as_str) {
            Some(text) if !text.trim().is_empty() => Ok(text),
            _ => bail!("The payload's {key} must be a text that is not blank"),
        }
    }
}

#[cfg(test)]
mod tests;

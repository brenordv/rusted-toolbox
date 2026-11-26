//! speak's settings: where the speech server is and what each request carries.

use anyhow::{Context, Result, anyhow, bail};
use common_file_utils::adoption::{Adoption, Shape, adopt};
use common_file_utils::permissions::create_dir_all_owner_only;
use common_serialization_utils::{load_json_file_to_object, save_object_to_json_file};
use common_utils::file_system::toolbox_home;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use url::Url;

/// speak's folder in the toolbox home: its package name, like every tool's.
const SETTINGS_FOLDER: &str = env!("CARGO_PKG_NAME");
/// The folder in the toolbox home that held speak's settings before 2.0.0.
const LEGACY_SETTINGS_FOLDER: &str = "tool-speak";
const CONFIG_FILE: &str = "config.json";
/// What the settings file is called in the line that reports moving it.
const SETTINGS_DATA: &str = "speak's settings";

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

/// Where speak's settings file is, and what happened to one found in the old folder.
#[derive(Debug)]
pub struct ConfigLocation {
    /// The settings file's place in speak's folder.
    pub target: PathBuf,
    /// What became of a settings file in the folder speak used before 2.0.0.
    pub adoption: Adoption,
}

impl ConfigLocation {
    /// The settings file to read and write: [`Self::target`], or the old file when it could
    /// not be moved.
    pub fn path(&self) -> PathBuf {
        self.adoption.path_in_use(&self.target)
    }

    /// Logs a settings file moved from the old folder, or left there, at `warn`.
    pub fn report(&self) {
        self.adoption.report(SETTINGS_DATA, &self.target);
    }
}

/// speak's settings file in the toolbox home `root`: `<root>/speak/config.json`. A settings
/// file only in the folder speak used before 2.0.0, `<root>/tool-speak/config.json`, is moved
/// there first; when it cannot be, the old file stays in use.
pub fn locate_config(root: &Path) -> ConfigLocation {
    let target = root.join(SETTINGS_FOLDER).join(CONFIG_FILE);
    let legacy = root.join(LEGACY_SETTINGS_FOLDER).join(CONFIG_FILE);
    let adoption = adopt(Some(&legacy), &target, Shape::File);
    ConfigLocation { target, adoption }
}

/// [`locate_config`] in the current user's toolbox home, `~/.rusted-toolbox`.
///
/// # Errors
/// When no home folder is found: speak then has no place for its settings.
pub fn locate_default_config() -> Result<ConfigLocation> {
    let root = toolbox_home().context("No home folder was found to keep speak's settings in")?;
    Ok(locate_config(&root))
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

/// `text` as a URL a request can go to.
///
/// # Errors
/// When `text` is blank or malformed (an `http` or `https` URL with no host is malformed), is
/// not `http` or `https`, or holds a user name or password. No error text repeats `text`.
fn checked_url(text: &str) -> Result<Url> {
    let text = text.trim();
    if text.is_empty() {
        bail!("The speech server URL is not set");
    }
    // The parse error is dropped on purpose: only its kind is public, and the message
    // must not grow a copy of a URL that may hold a password.
    let Ok(url) = Url::parse(text) else {
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

/// What separates a URL's scheme from the rest.
const SCHEME_SEPARATOR: &str = "://";

/// A speech URL as two texts: the server, and the endpoint on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlParts {
    /// The scheme, the host, and the port when there is one: `http://my-host:10157`.
    pub server: String,
    /// The path on the server, with any query and fragment: `/v1/audio/speech`.
    pub endpoint: String,
}

/// Cuts `url` into its server and its endpoint. The cut is on the text alone, so
/// [`join_url`] of the two parts of a valid URL gives that URL back character for character.
///
/// A blank `url` gives an empty server and [`SPEECH_PATH`]. Text with a scheme is cut at the
/// first `/`, `?`, or `#` after the scheme separator, and the endpoint is empty when there is
/// none. Text with no scheme is all server.
pub fn split_url(url: &str) -> UrlParts {
    let url = url.trim();
    if url.is_empty() {
        return UrlParts {
            server: String::new(),
            endpoint: SPEECH_PATH.to_owned(),
        };
    }
    let cut = url.find(SCHEME_SEPARATOR).and_then(|scheme_end| {
        let authority = scheme_end + SCHEME_SEPARATOR.len();
        url[authority..]
            .find(['/', '?', '#'])
            .map(|offset| authority + offset)
    });
    let (server, endpoint) = url.split_at(cut.unwrap_or(url.len()));
    UrlParts {
        server: server.to_owned(),
        endpoint: endpoint.to_owned(),
    }
}

/// The speech URL for `server` and `endpoint`: the server followed by the endpoint.
///
/// A server with no scheme gets `http://`. A non-blank endpoint that starts with neither `/`
/// nor `?` gets a `/` in front, and the server then loses its trailing slashes. A blank
/// endpoint is allowed.
///
/// # Errors
/// When the server is blank, holds a `?` or a `#` after its scheme, or is not by itself an
/// `http` or `https` URL with a host and no user name or password. No error text repeats
/// `server` or `endpoint`.
pub fn join_url(server: &str, endpoint: &str) -> Result<String> {
    let server = server.trim();
    let endpoint = endpoint.trim();
    if server.is_empty() {
        bail!("The speech server is not set");
    }
    let server = if server.contains(SCHEME_SEPARATOR) {
        server.to_owned()
    } else {
        format!("http://{server}")
    };
    let after_scheme = server
        .split_once(SCHEME_SEPARATOR)
        .map_or("", |(_, rest)| rest);
    if after_scheme.contains(['?', '#']) {
        bail!("The speech server cannot hold a ? or a #; put the query in Endpoint");
    }
    let endpoint = if endpoint.is_empty() || endpoint.starts_with(['/', '?']) {
        endpoint.to_owned()
    } else {
        format!("/{endpoint}")
    };
    let server = if endpoint.is_empty() {
        server.as_str()
    } else {
        server.trim_end_matches('/')
    };
    // The server is checked before the endpoint follows it: the parser skips every slash
    // after `http:` before it reads a host, so a server with no host would take the
    // endpoint's first segment as its host. A server that parses alone has a host, and what
    // follows a host cannot change it or fail to parse.
    checked_url(server)?;
    Ok(format!("{server}{endpoint}"))
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

    /// Saves the settings to `path`, creating its folder, owner-only on Unix, when it is
    /// missing.
    ///
    /// # Errors
    /// When the folder cannot be created or the file cannot be written; the error names the
    /// path.
    pub fn save(&self, path: &Path) -> Result<()> {
        let saved = match path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            Some(folder) => create_dir_all_owner_only(folder)
                .with_context(|| format!("failed to create folder {}", folder.display())),
            None => Ok(()),
        }
        .and_then(|()| save_object_to_json_file(path, self));
        saved.with_context(|| format!("speak's settings could not be saved to {}", path.display()))
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
        self.check_model()?;
        self.check_voice()?;
        Ok(())
    }

    /// Checks the payload's model.
    ///
    /// # Errors
    /// When it is not a text, or is blank.
    pub fn check_model(&self) -> Result<()> {
        self.payload_text(MODEL_KEY).map(|_| ())
    }

    /// Checks the payload's voice.
    ///
    /// # Errors
    /// When it is not a text, or is blank.
    pub fn check_voice(&self) -> Result<()> {
        self.payload_text(VOICE_KEY).map(|_| ())
    }

    /// The speech endpoint as a URL.
    ///
    /// # Errors
    /// When the URL is blank or malformed (an `http` or `https` URL with no host is
    /// malformed), is not `http` or `https`, or holds a user name or password. No error text
    /// repeats the URL.
    pub fn endpoint(&self) -> Result<Url> {
        checked_url(&self.url)
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

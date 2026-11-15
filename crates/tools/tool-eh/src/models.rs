//! Resolved run settings, the secrets taken from the environment, and run
//! outcomes.

use common_utils::string_utils::SecretString;
use shared_eventhub_core::filters::CaptureFilters;
use shared_eventhub_core::producer::{OutgoingEvent, SendPlan};
use shared_eventhub_persistence::config::{ExportSettings, SensitiveValues, ToolConfig};
use std::env::VarError;
use std::path::PathBuf;

/// Environment variable holding the event hub connection string.
pub const CONNECTION_STRING_ENV: &str = "EH_CONNECTION_STRING";
/// Environment variable holding a service principal's client secret.
pub const CLIENT_SECRET_ENV: &str = "EH_CLIENT_SECRET";

/// Secrets supplied through the environment. A variable that is unset,
/// empty, or only whitespace counts as absent.
#[derive(Clone, Debug, Default)]
pub struct Environment {
    pub connection_string: Option<SecretString>,
    pub client_secret: Option<SecretString>,
}

impl Environment {
    /// Reads [`CONNECTION_STRING_ENV`] and [`CLIENT_SECRET_ENV`] from the
    /// process environment.
    ///
    /// # Errors
    /// Fails when either variable is set to a value that is not valid
    /// Unicode.
    pub fn from_process() -> anyhow::Result<Self> {
        Ok(Self {
            connection_string: secret_from(
                CONNECTION_STRING_ENV,
                std::env::var(CONNECTION_STRING_ENV),
            )?,
            client_secret: secret_from(CLIENT_SECRET_ENV, std::env::var(CLIENT_SECRET_ENV))?,
        })
    }
}

fn secret_from(
    name: &str,
    value: Result<String, VarError>,
) -> anyhow::Result<Option<SecretString>> {
    match value {
        Ok(value) if value.trim().is_empty() => Ok(None),
        Ok(value) => Ok(Some(value.into())),
        Err(VarError::NotPresent) => Ok(None),
        Err(VarError::NotUnicode(_)) => anyhow::bail!("{name} is set but is not valid Unicode"),
    }
}

/// What happened to `config.json` for this run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigSource {
    /// `--no-config` was given; the file was not read.
    Disabled,
    Missing,
    /// A `config.json` that belongs to another tool; it was ignored.
    Foreign,
    Loaded,
}

/// The subcommand to run, with its settings resolved.
#[derive(Debug)]
pub enum EhCommand {
    Read(Box<ReadJob>),
    Export(Box<ExportJob>),
    Send(Box<SendJob>),
}

/// Where the body of `eh send` came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BodySource {
    /// `--body`.
    Inline,
    /// `--body-file <path>`.
    File(PathBuf),
    /// `--body-file -`.
    Stdin,
}

/// A send run: the merged configuration, the secrets given for it, and what
/// to send. `Debug` never shows the body or a secret.
#[derive(Clone, Debug)]
pub struct SendJob {
    /// The target merged as for `read`; every other setting is the recalled
    /// config's (only the data folder can change).
    pub config: ToolConfig,
    pub secrets: SensitiveValues,
    pub save_config: bool,
    pub working_dir: PathBuf,
    pub config_source: ConfigSource,
    pub plan: SendPlan,
    pub event: OutgoingEvent,
    pub body_source: BodySource,
}

/// A capture run: the merged configuration (flags over `config.json` over
/// defaults) and the secrets given for it.
#[derive(Clone, Debug)]
pub struct ReadJob {
    pub config: ToolConfig,
    /// Secrets given by flag or environment; only the ones the auth method
    /// uses are present.
    pub secrets: SensitiveValues,
    pub save_config: bool,
    pub verbose: bool,
    /// Where `config.json` is read and saved.
    pub working_dir: PathBuf,
    pub config_source: ConfigSource,
}

/// An export run over a data folder's archive.
#[derive(Clone, Debug)]
pub struct ExportJob {
    pub base_data_folder: PathBuf,
    pub export: ExportSettings,
    pub filters: CaptureFilters,
    pub config_source: ConfigSource,
}

/// How a run ended when it did not fail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunOutcome {
    Completed,
    Interrupted,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    #[test]
    fn a_set_variable_becomes_a_secret() {
        let secret = secret_from("X", Ok("value".to_owned())).unwrap();

        assert_eq!(secret.unwrap().expose(), "value");
    }

    #[rstest::rstest]
    #[case::unset(Err(VarError::NotPresent))]
    #[case::empty(Ok(String::new()))]
    #[case::whitespace(Ok("  \t".to_owned()))]
    fn an_unset_or_blank_variable_is_absent(#[case] value: Result<String, VarError>) {
        assert!(secret_from("X", value).unwrap().is_none());
    }

    #[test]
    fn a_variable_that_is_not_unicode_fails_naming_it() {
        let error = secret_from(
            CONNECTION_STRING_ENV,
            Err(VarError::NotUnicode(OsString::from("x"))),
        )
        .unwrap_err();

        assert!(
            error.to_string().contains(CONNECTION_STRING_ENV),
            "{error:#}"
        );
    }

    #[test]
    fn debug_of_the_environment_redacts_both_secrets() {
        let environment = Environment {
            connection_string: Some("Endpoint=sb://x/;SharedAccessKey=abc".to_owned().into()),
            client_secret: Some("client-secret-value".to_owned().into()),
        };

        let rendered = format!("{environment:?}");

        assert!(!rendered.contains("SharedAccessKey"), "{rendered}");
        assert!(!rendered.contains("client-secret-value"), "{rendered}");
    }
}

//! Merging the command line with `config.json` (flags over the environment
//! over `config.json` over defaults), validating the result, and parsing
//! timestamp and filter values. Everything here is pure.

use crate::cli_utils::{AuthArg, ExportArgs, ReadArgs, TargetArgs, TextFilterArgs, TimeFilterArgs};
use crate::models::{ConfigSource, Environment, ExportJob, ReadJob};
use anyhow::{Result, bail};
use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use shared_eventhub_core::connection::DEFAULT_CONSUMER_GROUP;
use shared_eventhub_core::filters::{CaptureFilters, TextFilter, TimeFilter};
use shared_eventhub_persistence::config::{
    AuthMethodConfig, CaptureConfig, CredentialKindConfig, DiskWriteSettings, ExportSettings,
    SensitiveValues, StorageConfig, StoredSecret, TargetConfig, ToolConfig,
};
use std::path::PathBuf;

const NO_TARGET: &str = "No event hub to read: pass -s, --auth, or --keyvault-url, set \
                         EH_CONNECTION_STRING, or run in a folder with a saved config.json";
const TIME_SHAPES: &str = "an RFC3339 timestamp such as 2026-07-02T12:34:56Z, or a date such as 2026-07-02 (midnight UTC)";
const CLIENT_IDS_NEED_CLIENT_SECRET: &str =
    "--tenant-id and --client-id apply to --auth client-secret only";

/// An inclusive enqueue-time window from `--between START..END`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TimeWindow {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

/// Builds a capture run from the arguments, the recalled `config.json`, and
/// the environment.
///
/// # Errors
/// Fails when no target can be found, when the auth flags are inconsistent,
/// when credential auth lacks a namespace or event hub, or when the consumer
/// group is empty.
pub(crate) fn read_job(
    args: ReadArgs,
    recalled: Option<&ToolConfig>,
    environment: &Environment,
    verbose: bool,
    working_dir: PathBuf,
    config_source: ConfigSource,
) -> Result<ReadJob> {
    let target = merge_target(&args.target, recalled, environment)?;
    let mut config = ToolConfig::new(target);
    config.capture = merge_capture(&args, recalled.map(|recalled| &recalled.capture));
    config.storage = merge_read_storage(&args, recalled.map(|recalled| &recalled.storage));
    let secrets = given_secrets(&config, &args.target, environment);

    Ok(ReadJob {
        config,
        secrets,
        save_config: args.target.save_config,
        verbose,
        working_dir,
        config_source,
    })
}

/// Builds an export run. Filters come from the arguments only: the recalled
/// capture filters describe what a capture shows, not what to export.
pub(crate) fn export_job(
    args: ExportArgs,
    recalled: Option<&ToolConfig>,
    config_source: ConfigSource,
) -> ExportJob {
    let storage = recalled
        .map(|recalled| recalled.storage.clone())
        .unwrap_or_default();
    let defaults = storage.export;
    let export = ExportSettings {
        format: args.format.map_or(defaults.format, Into::into),
        include_metadata: args.include_metadata.unwrap_or(defaults.include_metadata),
        condense_output: args.condense_output || defaults.condense_output,
        use_local_time: args.use_local_time || defaults.use_local_time,
        export_folder: args.export_folder.unwrap_or(defaults.export_folder),
        skip_already_exported: !args.ignore_export_log && defaults.skip_already_exported,
    };

    ExportJob {
        base_data_folder: PathBuf::from(
            args.data
                .base_data_folder
                .unwrap_or(storage.base_data_folder),
        ),
        export,
        filters: CaptureFilters {
            time: time_filter(&args.time),
            text: text_filter(&args.text),
        },
        config_source,
    }
}

fn merge_target(
    args: &TargetArgs,
    recalled: Option<&ToolConfig>,
    environment: &Environment,
) -> Result<TargetConfig> {
    let recalled = recalled.map(|recalled| &recalled.target);
    let (auth, namespace, event_hub) = if args.has_auth_flags() {
        (
            auth_from_flags(args)?,
            args.namespace.clone(),
            args.event_hub.clone(),
        )
    } else if let Some(recalled) = recalled {
        (
            recalled.auth.clone(),
            args.namespace
                .clone()
                .or_else(|| recalled.namespace.clone()),
            args.event_hub
                .clone()
                .or_else(|| recalled.event_hub.clone()),
        )
    } else if environment.connection_string.is_some() {
        (
            AuthMethodConfig::ConnectionString {
                stored: StoredSecret::LocalDb,
            },
            args.namespace.clone(),
            args.event_hub.clone(),
        )
    } else {
        bail!(NO_TARGET);
    };

    let consumer_group = args
        .consumer_group
        .clone()
        .or_else(|| recalled.map(|recalled| recalled.consumer_group.clone()))
        .unwrap_or_else(|| DEFAULT_CONSUMER_GROUP.to_owned());
    if consumer_group.trim().is_empty() {
        bail!(
            "The consumer group is empty (an unquoted $Default expands to nothing in most \
             shells; quote it: -g '$Default')"
        );
    }
    if let AuthMethodConfig::Credential { .. } = auth {
        if namespace.is_none() {
            bail!("Credential auth needs --namespace (the event hub's fully qualified namespace)");
        }
        if event_hub.is_none() {
            bail!("Credential auth needs -e/--entity-path (the event hub name)");
        }
    }

    Ok(TargetConfig {
        namespace,
        event_hub,
        consumer_group,
        auth,
    })
}

fn auth_from_flags(args: &TargetArgs) -> Result<AuthMethodConfig> {
    let key_vault = match (&args.keyvault_url, &args.keyvault_secret) {
        (Some(vault_url), Some(secret_name)) => Some((vault_url.clone(), secret_name.clone())),
        (None, None) => None,
        (Some(_), None) | (None, Some(_)) => {
            bail!("--keyvault-url and --keyvault-secret go together")
        }
    };
    let method = match (args.auth, args.connection_string.is_some()) {
        (None | Some(AuthArg::ConnectionString), true) => AuthArg::ConnectionString,
        (Some(other), true) => bail!(
            "-s/--connection-string is connection-string auth; it cannot be combined with --auth {}",
            other.flag_value()
        ),
        (Some(method), false) => method,
        (None, false) if key_vault.is_some() => bail!(
            "--keyvault-url needs --auth naming the credential for the vault: developer, \
             managed-identity, or client-secret"
        ),
        (None, false) => bail!("--tenant-id and --client-id need --auth client-secret"),
    };
    let has_client_ids = args.tenant_id.is_some() || args.client_id.is_some();

    let credential = match method {
        AuthArg::ConnectionString => {
            if key_vault.is_some() {
                bail!(
                    "--keyvault-url fetches the connection string; it cannot be combined with \
                     connection-string auth (-s or --auth connection-string)"
                );
            }
            if has_client_ids {
                bail!(CLIENT_IDS_NEED_CLIENT_SECRET);
            }
            return Ok(AuthMethodConfig::ConnectionString {
                stored: StoredSecret::LocalDb,
            });
        }
        AuthArg::Developer | AuthArg::ManagedIdentity if has_client_ids => {
            bail!(CLIENT_IDS_NEED_CLIENT_SECRET)
        }
        AuthArg::Developer => CredentialKindConfig::DeveloperTools,
        AuthArg::ManagedIdentity => CredentialKindConfig::ManagedIdentity,
        AuthArg::ClientSecret => client_secret_credential(args)?,
    };

    Ok(match key_vault {
        Some((vault_url, secret_name)) => AuthMethodConfig::KeyVaultConnectionString {
            vault_url,
            secret_name,
            credential,
        },
        None => AuthMethodConfig::Credential { credential },
    })
}

fn client_secret_credential(args: &TargetArgs) -> Result<CredentialKindConfig> {
    let non_blank = |value: &Option<String>| {
        value
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    match (non_blank(&args.tenant_id), non_blank(&args.client_id)) {
        (Some(tenant_id), Some(client_id)) => Ok(CredentialKindConfig::ClientSecret {
            tenant_id,
            client_id,
        }),
        (None, _) | (_, None) => bail!("--auth client-secret needs --tenant-id and --client-id"),
    }
}

/// The secrets given for this run that its auth method uses: the connection
/// string from `-s` or the environment, and the client secret from the
/// environment.
fn given_secrets(
    config: &ToolConfig,
    args: &TargetArgs,
    environment: &Environment,
) -> SensitiveValues {
    let connection_string = match config.target.auth {
        AuthMethodConfig::ConnectionString { .. } => args
            .connection_string
            .clone()
            .or_else(|| environment.connection_string.clone()),
        AuthMethodConfig::Credential { .. } | AuthMethodConfig::KeyVaultConnectionString { .. } => {
            None
        }
    };
    let client_secret = config
        .client_secret_id()
        .and_then(|_| environment.client_secret.clone());

    SensitiveValues {
        connection_string,
        client_secret,
    }
}

fn merge_capture(args: &ReadArgs, recalled: Option<&CaptureConfig>) -> CaptureConfig {
    let recalled = recalled.cloned().unwrap_or_default();
    let partitions = if args.partitions.is_empty() {
        recalled.partitions
    } else {
        Some(args.partitions.clone())
    };

    CaptureConfig {
        partitions,
        ignore_checkpoint: args.ignore_checkpoint || recalled.ignore_checkpoint,
        prefetch: recalled.prefetch,
        feedback_interval_secs: args
            .feedback_interval
            .unwrap_or(recalled.feedback_interval_secs),
        filters: CaptureFilters {
            time: time_filter(&args.time).or(recalled.filters.time),
            text: text_filter(&args.text).or(recalled.filters.text),
        },
    }
}

fn merge_read_storage(args: &ReadArgs, recalled: Option<&StorageConfig>) -> StorageConfig {
    let recalled = recalled.cloned().unwrap_or_default();

    StorageConfig {
        base_data_folder: args
            .data
            .base_data_folder
            .clone()
            .unwrap_or(recalled.base_data_folder),
        disk_write: DiskWriteSettings {
            mode: args.disk_mode.map_or(recalled.disk_write.mode, Into::into),
            received_msg_path: args
                .received_msg_path
                .clone()
                .unwrap_or(recalled.disk_write.received_msg_path),
            content_only: args.dump_content_only || recalled.disk_write.content_only,
        },
        export: recalled.export,
    }
}

fn time_filter(args: &TimeFilterArgs) -> Option<TimeFilter> {
    let TimeFilterArgs {
        older_than,
        older_or_equal,
        newer_than,
        newer_or_equal,
        during,
        between,
    } = args;

    older_than
        .map(TimeFilter::OlderThan)
        .or(older_or_equal.map(TimeFilter::OlderOrEqual))
        .or(newer_than.map(TimeFilter::NewerThan))
        .or(newer_or_equal.map(TimeFilter::NewerOrEqual))
        .or(during.map(TimeFilter::During))
        .or(between.map(|window| TimeFilter::Between {
            start: window.start,
            end: window.end,
        }))
}

fn text_filter(args: &TextFilterArgs) -> Option<TextFilter> {
    args.filter_regex
        .clone()
        .or_else(|| args.dump_filter.clone().map(TextFilter::Contains))
}

/// Parses an RFC3339 timestamp, or a `YYYY-MM-DD` date as midnight UTC.
pub(crate) fn parse_time(value: &str) -> Result<DateTime<Utc>, String> {
    match DateTime::parse_from_rfc3339(value) {
        Ok(time) => Ok(time.with_timezone(&Utc)),
        Err(_) => NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .map(|date| date.and_time(NaiveTime::MIN).and_utc())
            .map_err(|_| format!("expected {TIME_SHAPES}")),
    }
}

/// Parses the full RFC3339 timestamp `--during` needs. A date alone is
/// refused: as midnight it would select one hour, not the day.
pub(crate) fn parse_time_with_granularity(value: &str) -> Result<DateTime<Utc>, String> {
    match DateTime::parse_from_rfc3339(value) {
        Ok(time) => Ok(time.with_timezone(&Utc)),
        Err(_) if NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok() => Err(format!(
            "a date alone would select its first hour only; for the whole day use \
             --between {value}T00:00:00Z..{value}T23:59:59.999Z"
        )),
        Err(_) => Err(
            "expected an RFC3339 timestamp such as 2026-07-02T12:34:00Z (that minute)".to_owned(),
        ),
    }
}

/// Parses `START..END`, two [`parse_time`] values with `START` not after
/// `END`.
pub(crate) fn parse_window(value: &str) -> Result<TimeWindow, String> {
    let Some((start, end)) = value.split_once("..") else {
        return Err(format!("expected START..END, each {TIME_SHAPES}"));
    };
    let start = parse_time(start).map_err(|error| format!("START: {error}"))?;
    let end = parse_time(end).map_err(|error| format!("END: {error}"))?;
    if start > end {
        return Err("START is after END".to_owned());
    }
    Ok(TimeWindow { start, end })
}

/// Compiles a `--filter-regex` pattern.
pub(crate) fn parse_regex(value: &str) -> Result<TextFilter, String> {
    TextFilter::regex(value).map_err(|error| format!("{error:#}"))
}

#[cfg(test)]
mod tests;

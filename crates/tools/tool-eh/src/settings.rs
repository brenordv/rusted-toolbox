//! Merging the command line with `config.json` (flags over the environment
//! over `config.json` over defaults), validating the result, and parsing
//! timestamp and filter values. Everything here is pure.

use crate::cli_utils::{AuthArg, ExportArgs, ReadArgs, TargetArgs, TextFilterArgs, TimeFilterArgs};
use crate::models::{
    BodySource, CONNECTION_STRING_ENV, ConfigSource, Environment, ExportJob, ReadJob, SendJob,
};
use crate::progress::{display, target_label};
use crate::send_cli::{ArgText, SendArgs};
use anyhow::{Context, Result, bail};
use common_utils::string_utils::{SecretString, non_blank};
use mock_data_utils::template::Template;
use shared_azure::auth::AuthMethod;
use shared_eventhub_core::connection::{DEFAULT_CONSUMER_GROUP, EventHubTarget, target_names};
use shared_eventhub_core::filters::{CaptureFilters, TextFilter, TimeFilter};
use shared_eventhub_core::producer::{
    OutgoingEvent, Pacing, Routing, SendPlan, insert_property, parse_property,
};
use shared_eventhub_persistence::config::{
    AuthMethodConfig, CaptureConfig, CredentialKindConfig, DiskWriteSettings, ExportSettings,
    SensitiveValues, StorageConfig, StoredSecret, TargetConfig, ToolConfig,
};
use shared_eventhub_persistence::workspace::ConfigHome;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

const NO_TARGET: &str = "No event hub given: pass -s, --auth, or --keyvault-url, set \
                         EH_CONNECTION_STRING, or run in a folder with a saved config.json";
const CLIENT_IDS_NEED_CLIENT_SECRET: &str =
    "--tenant-id and --client-id apply to --auth client-secret only";
const NO_KEYWORD: &str =
    "--dump-filter needs at least one keyword, as in --dump-filter error,timeout";

/// A folder setting of `config.json`: what messages call it, and the flag
/// that sets it for one run.
struct FolderSetting {
    name: &'static str,
    flag: &'static str,
}

const DATA_FOLDER: FolderSetting = FolderSetting {
    name: "a data folder",
    flag: "-b",
};
const EXPORT_FOLDER: FolderSetting = FolderSetting {
    name: "an export folder",
    flag: "--export-folder",
};
const RECEIVED_MSG_PATH: FolderSetting = FolderSetting {
    name: "a received message path",
    flag: "--received-msg-path",
};

/// Checks one folder setting from its text alone, so no folder is touched.
/// A folder `typed` on this run is used as given. Without one, the `recalled`
/// folder must be one `home` uses
/// ([`ConfigHome::uses_recalled_folder`]): a `config.json` in the current
/// folder can come with a cloned repository, and may only name folders
/// inside that folder.
///
/// # Errors
/// Fails for a recalled folder `home` does not use, and for a typed one that
/// `save_config` would write into a `config.json` whose home would not use
/// it on the next run.
fn check_folder(
    setting: &FolderSetting,
    typed: Option<&str>,
    recalled: Option<&str>,
    home: &ConfigHome,
    save_config: bool,
) -> Result<()> {
    match (typed, recalled) {
        (Some(folder), _) if save_config && !home.uses_recalled_folder(folder) => bail!(
            "--save-config cannot save {} outside this folder ('{}'): a config.json here is \
             recalled only with folders inside it. Keep the setup in a profile (--profile NAME).",
            setting.name,
            display(folder)
        ),
        (None, Some(folder)) if !home.uses_recalled_folder(folder) => bail!(
            "config.json names {} outside this folder ('{}'). Pass {} FOLDER to choose one for \
             this run, or keep the setup in a profile (--profile NAME).",
            setting.name,
            display(folder),
            setting.flag
        ),
        (Some(_), _) | (None, Some(_) | None) => Ok(()),
    }
}

/// Builds a capture run from the arguments, the recalled `config.json`, and
/// the environment.
///
/// # Errors
/// Fails when the current folder's `config.json` names a data folder or a
/// received message path outside that folder and no flag replaces it, when
/// `--save-config` would save such a folder there, when no target can be
/// found, when the auth flags are inconsistent, when credential auth lacks a
/// namespace or event hub, when the consumer group is empty, or when
/// `--dump-filter` holds no keyword.
pub(crate) fn read_job(
    args: ReadArgs,
    recalled: Option<&ToolConfig>,
    environment: &Environment,
    verbose: bool,
    home: ConfigHome,
    config_source: ConfigSource,
) -> Result<ReadJob> {
    let storage = recalled.map(|recalled| &recalled.storage);
    check_folder(
        &DATA_FOLDER,
        args.data.base_data_folder.as_deref(),
        storage.map(|storage| storage.base_data_folder.as_str()),
        &home,
        args.target.save_config,
    )?;
    check_folder(
        &RECEIVED_MSG_PATH,
        args.received_msg_path.as_deref(),
        storage.map(|storage| storage.disk_write.received_msg_path.as_str()),
        &home,
        args.target.save_config,
    )?;
    let target = merge_target(
        &args.target,
        args.consumer_group.as_deref(),
        recalled,
        environment,
    )?;
    let mut config = ToolConfig::new(target);
    config.capture = merge_capture(&args, recalled.map(|recalled| &recalled.capture))?;
    config.storage = merge_read_storage(&args, recalled.map(|recalled| &recalled.storage));
    let secrets = given_secrets(&config, &args.target, environment);
    let paths = home.data_paths(&config.storage.base_data_folder);

    Ok(ReadJob {
        config,
        secrets,
        save_config: args.target.save_config,
        verbose,
        home,
        paths,
        config_source,
    })
}

/// Builds an export run. Filters come from the arguments only: the recalled
/// capture filters describe what a capture shows, not what to export.
///
/// # Errors
/// Fails when the current folder's `config.json` names a data folder or an
/// export folder outside that folder and no flag replaces it, or when
/// `--dump-filter` holds no keyword.
pub(crate) fn export_job(
    args: ExportArgs,
    recalled: Option<&ToolConfig>,
    home: ConfigHome,
    config_source: ConfigSource,
) -> Result<ExportJob> {
    let recalled_storage = recalled.map(|recalled| &recalled.storage);
    check_folder(
        &DATA_FOLDER,
        args.data.base_data_folder.as_deref(),
        recalled_storage.map(|storage| storage.base_data_folder.as_str()),
        &home,
        false,
    )?;
    check_folder(
        &EXPORT_FOLDER,
        args.export_folder.as_deref(),
        recalled_storage.map(|storage| storage.export.export_folder.as_str()),
        &home,
        false,
    )?;
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

    let base_data_folder = args
        .data
        .base_data_folder
        .unwrap_or(storage.base_data_folder);

    Ok(ExportJob {
        paths: home.data_paths(&base_data_folder),
        home,
        export,
        filters: CaptureFilters {
            time: time_filter(&args.time),
            text: text_filter(&args.text)?,
        },
        config_source,
    })
}

/// Builds a send run. The target merges exactly as for `read` (a recalled
/// consumer group is kept, so saving does not lose it); every other setting
/// is the recalled config's except the data folder. A target recalled from
/// `config.json` must be named on the command line or confirmed with
/// `--yes`, unless it comes from a profile that names its namespace and
/// event hub.
///
/// # Errors
/// Fails when a `--property` is malformed or repeats a key (checked before
/// anything else), when the current folder's `config.json` names a data
/// folder outside that folder and `-b` does not replace it, when
/// `--save-config` would save such a folder there, as [`read_job`] does for
/// the target, when the recalled target is neither named nor confirmed, and
/// when the partition key is blank.
pub(crate) fn send_job(
    args: SendArgs,
    (body, body_source): (Vec<u8>, BodySource),
    template: Option<Arc<Template>>,
    recalled: Option<&ToolConfig>,
    environment: &Environment,
    home: ConfigHome,
    config_source: ConfigSource,
) -> Result<SendJob> {
    let properties = send_properties(&args.properties)?;
    check_folder(
        &DATA_FOLDER,
        args.data.base_data_folder.as_deref(),
        recalled.map(|recalled| recalled.storage.base_data_folder.as_str()),
        &home,
        args.target.save_config,
    )?;
    let target = merge_target(&args.target, None, recalled, environment)?;
    check_recalled_destination(&args, &target, recalled, environment, &home)?;
    let mut config = match recalled {
        Some(recalled) => ToolConfig {
            target,
            ..recalled.clone()
        },
        None => ToolConfig::new(target),
    };
    if let Some(base_data_folder) = args.data.base_data_folder.clone() {
        config.storage.base_data_folder = base_data_folder;
    }
    let routing = match &args.partition_key {
        Some(key) => Routing::partition_key(key.as_str())?,
        None => Routing::automatic(),
    };
    let pacing = match args.interval_ms {
        Some(interval_ms) => Pacing::interval(Duration::from_millis(interval_ms))?,
        None => Pacing::batched(),
    };
    let secrets = given_secrets(&config, &args.target, environment);
    let paths = home.data_paths(&config.storage.base_data_folder);

    Ok(SendJob {
        config,
        secrets,
        save_config: args.target.save_config,
        home,
        paths,
        config_source,
        plan: SendPlan::new(args.count, routing, pacing)?,
        event: OutgoingEvent {
            body,
            content_type: args.content_type.as_deref().and_then(non_blank),
            correlation_id: args.correlation_id.as_ref().and_then(|id| non_blank(&id.0)),
            properties,
        },
        template,
        body_source,
    })
}

/// The `--property` values as one map. An error names the flag by its
/// position on the command line ("--property 2: ...").
fn send_properties(values: &[ArgText]) -> Result<BTreeMap<String, String>> {
    let mut properties = BTreeMap::new();
    for (index, text) in values.iter().enumerate() {
        let flag = || format!("--property {}", index + 1);
        let (key, value) = parse_property(&text.0).with_context(flag)?;
        insert_property(&mut properties, key, value).with_context(flag)?;
    }
    Ok(properties)
}

/// Refuses a send in a folder with a `config.json` unless the run's
/// destination comes from this command line or the environment, or `--yes`
/// confirms it. A cloned repository can ship a `config.json` that points at
/// someone else's event hub, so the merged `target` counts as named only
/// when `--namespace` and `-e` are both on the command line (a saved
/// connection string must then match both: a string without `EntityPath`
/// would otherwise keep its own namespace under any `-e`), when
/// connection-string auth uses a string given by `-s`, or when Key Vault
/// auth names its vault with `--keyvault-url`. A string from
/// `EH_CONNECTION_STRING` names the destination with an auth flag (which
/// drops the recalled names), with `-e`, or when it resolves an event hub by
/// itself; otherwise the event hub would be the recalled one, in whatever
/// namespace the environment's string opens. A profile's
/// `config.json` that names both the namespace and the event hub counts as
/// named while the merged `target` still carries both names: the profile
/// lives in the user's own workspace, and those names pin any stored
/// connection string. An auth flag replaces the recalled names, so with one
/// the profile earns no trust.
fn check_recalled_destination(
    args: &SendArgs,
    target: &TargetConfig,
    recalled: Option<&ToolConfig>,
    environment: &Environment,
    home: &ConfigHome,
) -> Result<()> {
    let Some(recalled) = recalled else {
        return Ok(());
    };
    let profile = home.profile_name();
    let profile_pins_destination = profile.is_some()
        && recalled.target.namespace.is_some()
        && recalled.target.event_hub.is_some()
        && target.namespace.is_some()
        && target.event_hub.is_some();
    if args.yes || profile_pins_destination {
        return Ok(());
    }
    let flags = &args.target;
    // The environment's string, while it is the run's string and the recalled names are still
    // in the merged target.
    let environment_string = match target.auth {
        AuthMethodConfig::ConnectionString { .. } if !flags.has_auth_flags() => {
            environment.connection_string.as_ref()
        }
        AuthMethodConfig::ConnectionString { .. }
        | AuthMethodConfig::KeyVaultConnectionString { .. }
        | AuthMethodConfig::Credential { .. } => None,
    };
    let given = match (&target.auth, environment_string) {
        (AuthMethodConfig::ConnectionString { .. }, Some(connection_string)) => {
            flags.event_hub.is_some() || names_an_event_hub(connection_string)
        }
        (AuthMethodConfig::ConnectionString { .. }, None) => {
            flags.connection_string.is_some() || environment.connection_string.is_some()
        }
        (AuthMethodConfig::KeyVaultConnectionString { .. }, _) => flags.keyvault_url.is_some(),
        (AuthMethodConfig::Credential { .. }, _) => false,
    };
    if given || (flags.namespace.is_some() && flags.event_hub.is_some()) {
        return Ok(());
    }
    let source = match (flags.has_auth_flags(), profile) {
        (true, _) => "the connection string saved in the data folder".to_owned(),
        (false, Some(name)) => format!("profile {}", display(name)),
        (false, None) => "this folder's config.json".to_owned(),
    };
    match environment_string {
        Some(_) => bail!(
            "This run would send to {}, taken from {source} (the connection string in \
             {CONNECTION_STRING_ENV} does not name an event hub by itself). Name the event hub \
             with -e, or pass --yes.",
            target_label(target)
        ),
        None => bail!(
            "This run would send to {}, taken from {source}. Name it with --namespace and -e, \
             or pass --yes.",
            target_label(target)
        ),
    }
}

/// Whether `connection_string` resolves an event hub with no namespace or event hub given
/// beside it: it parses, its endpoint is an Event Hubs host, and it carries an `EntityPath`.
/// Nothing is fetched or opened.
fn names_an_event_hub(connection_string: &SecretString) -> bool {
    let target = EventHubTarget {
        auth: AuthMethod::ConnectionString {
            connection_string: connection_string.clone(),
        },
        client_secret: None,
        fully_qualified_namespace: None,
        event_hub_name: None,
        consumer_group: DEFAULT_CONSUMER_GROUP.to_owned(),
    };
    matches!(target_names(&target), Some(Ok(_)))
}

fn merge_target(
    args: &TargetArgs,
    consumer_group: Option<&str>,
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

    let consumer_group = consumer_group
        .map(str::to_owned)
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
    let tenant_id = args.tenant_id.as_deref().and_then(non_blank);
    let client_id = args.client_id.as_deref().and_then(non_blank);
    match (tenant_id, client_id) {
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

fn merge_capture(args: &ReadArgs, recalled: Option<&CaptureConfig>) -> Result<CaptureConfig> {
    let recalled = recalled.cloned().unwrap_or_default();
    let partitions = if args.partitions.is_empty() {
        recalled.partitions
    } else {
        Some(args.partitions.clone())
    };

    Ok(CaptureConfig {
        partitions,
        ignore_checkpoint: args.ignore_checkpoint || recalled.ignore_checkpoint,
        prefetch: recalled.prefetch,
        feedback_interval_secs: args
            .feedback_interval
            .unwrap_or(recalled.feedback_interval_secs),
        filters: CaptureFilters {
            time: time_filter(&args.time).or(recalled.filters.time),
            text: text_filter(&args.text)?.or(recalled.filters.text),
        },
    })
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
        .or(between.map(TimeFilter::from))
}

/// The text filter the flags name: `--filter-regex` as compiled, or
/// `--dump-filter` through [`TextFilter::keywords`].
fn text_filter(args: &TextFilterArgs) -> Result<Option<TextFilter>> {
    if let Some(regex) = &args.filter_regex {
        return Ok(Some(regex.clone()));
    }
    args.dump_filter
        .as_ref()
        .map(|values| TextFilter::keywords(values.iter().map(String::as_str)).context(NO_KEYWORD))
        .transpose()
}

/// Compiles a `--filter-regex` pattern.
pub(crate) fn parse_regex(value: &str) -> Result<TextFilter, String> {
    TextFilter::regex(value).map_err(|error| format!("{error:#}"))
}

#[cfg(test)]
mod tests;

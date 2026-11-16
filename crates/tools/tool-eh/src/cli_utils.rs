use crate::models::{ConfigSource, DryRunJob, EhCommand, Environment, ExportJob, ReadJob};
use crate::progress::{
    describe_auth, describe_text_filter, describe_time_filter, display, target_label,
};
use crate::send_cli::{
    SendArgs, compile_template, dry_run_header, load_body, send_header, template_help,
};
use crate::settings::{export_job, parse_regex, read_job, send_job};
use anyhow::Result;
use chrono::{DateTime, Utc};
use clap::{Args, Parser, Subcommand, ValueEnum};
use common_cli::common_tool_args::CommonToolArgs;
use common_cli::header_format::format_config_item;
use common_utils::file_system::get_current_dir;
use common_utils::string_utils::{SecretString, escape_for_terminal_display};
use shared_eventhub_core::filters::{
    TextFilter, TimeWindow, parse_time, parse_time_with_granularity, parse_window,
};
use shared_eventhub_persistence::config::{ConfigLoad, ToolConfig, load_config};
use shared_eventhub_persistence::export::{DiskWriteMode, ExportFormat};
use shared_eventhub_persistence::paths::DataPaths;
use std::fmt::Display;
use std::path::PathBuf;
use std::sync::Arc;

/// Azure Event Hubs CLI: captures events into a local archive, exports them,
/// and sends test events.
///
/// `read` captures events from an event hub into the data folder's archive
/// (optionally writing message files as it goes), `export` writes archived
/// events to txt, csv, or json files, and `send` publishes one body a given
/// number of times. The shared flags
/// (`--app-header`, `--verbose`, `--log-level`, ...) go before the
/// subcommand: `eh --verbose read ...`.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about, arg_required_else_help = true)]
struct CliArgs {
    #[command(subcommand)]
    command: Commands,

    #[command(flatten)]
    common: CommonToolArgs,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Capture events from an event hub into the local archive
    #[command(after_help = "Examples:\n  \
        eh read -e orders                (connection string in EH_CONNECTION_STRING)\n  \
        eh read --auth developer --namespace my-ns.servicebus.windows.net -e orders -g 'my-group'\n  \
        eh read --auth developer --keyvault-url https://my-vault.vault.azure.net --keyvault-secret eh-orders\n  \
        eh --verbose read --newer-than 2026-07-02T12:00:00Z --dump-filter error,timeout\n\n\
        Timestamps are RFC3339 (2026-07-02T12:34:56Z) or a date (2026-07-02, midnight UTC).")]
    Read(Box<ReadArgs>),

    /// Export archived events to txt, csv, or json files
    #[command(after_help = "Examples:\n  \
        eh export\n  \
        eh export -f csv --include-metadata false --export-folder out\n  \
        eh export --between 2026-07-02T00:00:00Z..2026-07-02T23:59:59Z --filter-regex 'order-\\d+'")]
    Export(Box<ExportArgs>),

    /// Send test events to an event hub
    #[command(after_help = "Examples:\n  \
        eh send -e orders --body '{\"orderId\": 1}' --content-type application/json\n  \
        eh send --body-file order.json -n 500\n  \
        eh send --body-file - --partition-key customer-17 < event.json\n  \
        eh send -e orders --body ping -n 600 --interval-ms 1000\n  \
        eh send --template --dry-run -n 3 --body '{\"id\": {{random.integer min=1 max=999}}}'\n\n\
        A target recalled from config.json must be named again with --namespace and -e, or \
        confirmed with --yes.", after_long_help = template_help())]
    Send(Box<SendArgs>),
}

/// Where the event hub is and how to authenticate to it.
#[derive(Args, Debug, Default)]
pub(crate) struct TargetArgs {
    /// Event hub connection string. Visible in the process list and shell
    /// history: prefer the EH_CONNECTION_STRING environment variable, or pass
    /// it once with --save-config
    #[arg(short = 's', long = "connection-string", value_name = "STRING")]
    pub connection_string: Option<SecretString>,

    /// Fully qualified namespace, e.g. my-ns.servicebus.windows.net
    #[arg(long = "namespace", value_name = "FQNS")]
    pub namespace: Option<String>,

    /// Event hub name
    #[arg(short = 'e', long = "entity-path", value_name = "NAME")]
    pub event_hub: Option<String>,

    /// Authentication method; with --keyvault-url it is the credential used
    /// for the vault
    #[arg(long = "auth", value_enum, value_name = "METHOD")]
    pub auth: Option<AuthArg>,

    /// Tenant of the service principal (--auth client-secret); the secret
    /// comes from EH_CLIENT_SECRET or the saved secrets
    #[arg(long = "tenant-id", value_name = "ID")]
    pub tenant_id: Option<String>,

    /// Client id of the service principal (--auth client-secret)
    #[arg(long = "client-id", value_name = "ID")]
    pub client_id: Option<String>,

    /// Key Vault holding the connection string
    #[arg(long = "keyvault-url", value_name = "URL")]
    pub keyvault_url: Option<String>,

    /// Name of the Key Vault secret holding the connection string
    #[arg(long = "keyvault-secret", value_name = "NAME")]
    pub keyvault_secret: Option<String>,

    /// Save this run's settings to config.json (secrets go to the data
    /// folder's secrets file, not encrypted), then continue
    #[arg(long = "save-config")]
    pub save_config: bool,
}

impl TargetArgs {
    /// Whether any flag that selects the auth method was given.
    pub(crate) fn has_auth_flags(&self) -> bool {
        self.connection_string.is_some()
            || self.auth.is_some()
            || self.keyvault_url.is_some()
            || self.keyvault_secret.is_some()
            || self.tenant_id.is_some()
            || self.client_id.is_some()
    }
}

/// The authentication method named by `--auth`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum AuthArg {
    ConnectionString,
    /// The local developer tools (Azure CLI, Azure Developer CLI)
    Developer,
    /// The system-assigned managed identity
    ManagedIdentity,
    /// A service principal: --tenant-id, --client-id, and EH_CLIENT_SECRET
    ClientSecret,
}

impl AuthArg {
    pub(crate) fn flag_value(self) -> &'static str {
        match self {
            Self::ConnectionString => "connection-string",
            Self::Developer => "developer",
            Self::ManagedIdentity => "managed-identity",
            Self::ClientSecret => "client-secret",
        }
    }
}

/// The data folder and whether to read `config.json`.
#[derive(Args, Debug, Default)]
pub(crate) struct DataArgs {
    /// Data folder holding the archive and the secrets file [default: .eh-data]
    #[arg(short = 'b', long = "base-data-folder", value_name = "PATH")]
    pub base_data_folder: Option<String>,

    /// Ignore config.json in the current folder
    #[arg(long = "no-config")]
    pub no_config: bool,
}

/// Body text conditions.
#[derive(Args, Debug, Default)]
pub(crate) struct TextFilterArgs {
    /// Comma-separated keywords; a message matches when its body contains any
    /// of them, ignoring case. Spaces around a keyword and empty items are
    /// ignored
    #[arg(long = "dump-filter", value_name = "KEYWORDS", value_delimiter = ',')]
    pub dump_filter: Option<Vec<String>>,

    /// Regular expression matched against the body, ignoring case unless the
    /// pattern starts with (?-i)
    #[arg(
        long = "filter-regex",
        value_name = "PATTERN",
        value_parser = parse_regex,
        conflicts_with = "dump_filter"
    )]
    pub filter_regex: Option<TextFilter>,
}

/// Enqueue-time conditions; at most one per run.
#[derive(Args, Debug, Default)]
#[group(id = "time_filter", multiple = false)]
pub(crate) struct TimeFilterArgs {
    /// Only events enqueued before TIME
    #[arg(long = "older-than", value_name = "TIME", value_parser = parse_time)]
    pub older_than: Option<DateTime<Utc>>,

    /// Only events enqueued at or before TIME
    #[arg(long = "older-or-equal", value_name = "TIME", value_parser = parse_time)]
    pub older_or_equal: Option<DateTime<Utc>>,

    /// Only events enqueued after TIME
    #[arg(long = "newer-than", value_name = "TIME", value_parser = parse_time)]
    pub newer_than: Option<DateTime<Utc>>,

    /// Only events enqueued at or after TIME
    #[arg(long = "newer-or-equal", value_name = "TIME", value_parser = parse_time)]
    pub newer_or_equal: Option<DateTime<Utc>>,

    /// Only events enqueued during the hour, minute, or second TIME names
    /// (12:00:00 is the hour, 12:34:00 the minute, 12:34:56 the second); a
    /// full timestamp is required
    #[arg(long = "during", value_name = "TIME", value_parser = parse_time_with_granularity)]
    pub during: Option<DateTime<Utc>>,

    /// Only events enqueued from START to END, both included
    #[arg(long = "between", value_name = "START..END", value_parser = parse_window)]
    pub between: Option<TimeWindow>,
}

/// `eh read` arguments.
#[derive(Args, Debug, Default)]
pub(crate) struct ReadArgs {
    #[command(flatten)]
    pub target: TargetArgs,

    #[command(flatten)]
    pub data: DataArgs,

    /// Consumer group [default: $Default]
    #[arg(short = 'g', long = "consumer-group", value_name = "NAME")]
    pub consumer_group: Option<String>,

    /// Partition to read; repeat for several [default: all]
    #[arg(short = 'p', long = "partition", value_name = "ID")]
    pub partitions: Vec<String>,

    /// Start from the time filter or the earliest event instead of the stored
    /// checkpoint, and store every event again
    #[arg(short = 'i', long = "ignore-checkpoint")]
    pub ignore_checkpoint: bool,

    /// Seconds between progress updates [default: 1]
    #[arg(
        long = "feedback-interval",
        value_name = "SECONDS",
        value_parser = clap::value_parser!(u64).range(1..)
    )]
    pub feedback_interval: Option<u64>,

    #[command(flatten)]
    pub text: TextFilterArgs,

    #[command(flatten)]
    pub time: TimeFilterArgs,

    /// Also write matching messages to files: as they arrive, or when the run
    /// ends [default: off]
    #[arg(long = "disk-mode", value_enum, value_name = "MODE")]
    pub disk_mode: Option<DiskModeArg>,

    /// Folder for message files, relative to the data folder unless absolute
    /// [default: inbound]
    #[arg(short = 'r', long = "received-msg-path", value_name = "PATH")]
    pub received_msg_path: Option<String>,

    /// Write only the message body to message files
    #[arg(long = "dump-content-only")]
    pub dump_content_only: bool,
}

/// `eh export` arguments.
#[derive(Args, Debug, Default)]
pub(crate) struct ExportArgs {
    #[command(flatten)]
    pub data: DataArgs,

    /// File format [default: txt]
    #[arg(short = 'f', long = "export-format", value_enum, value_name = "FORMAT")]
    pub format: Option<FormatArg>,

    /// Folder for exported files, relative to the data folder unless absolute
    /// [default: exports]
    #[arg(long = "export-folder", value_name = "PATH")]
    pub export_folder: Option<String>,

    /// One file per month instead of one file per message
    #[arg(long = "condense-output")]
    pub condense_output: bool,

    /// Write the details (txt) or metadata columns (csv, json) with the body
    /// [default: true]
    #[arg(long = "include-metadata", value_name = "BOOL")]
    pub include_metadata: Option<bool>,

    /// Date folders and file names in local time instead of UTC
    #[arg(long = "use-local-time")]
    pub use_local_time: bool,

    /// Export messages again even when the export log says they were exported
    #[arg(short = 'i', long = "ignore-export-log")]
    pub ignore_export_log: bool,

    #[command(flatten)]
    pub text: TextFilterArgs,

    #[command(flatten)]
    pub time: TimeFilterArgs,
}

/// `--disk-mode` values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum DiskModeArg {
    Off,
    Immediate,
    AtEnd,
}

impl From<DiskModeArg> for DiskWriteMode {
    fn from(mode: DiskModeArg) -> Self {
        match mode {
            DiskModeArg::Off => Self::Off,
            DiskModeArg::Immediate => Self::Immediate,
            DiskModeArg::AtEnd => Self::AtEnd,
        }
    }
}

/// `--export-format` values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum FormatArg {
    Txt,
    Csv,
    Json,
}

impl From<FormatArg> for ExportFormat {
    fn from(format: FormatArg) -> Self {
        match format {
            FormatArg::Txt => Self::Txt,
            FormatArg::Csv => Self::Csv,
            FormatArg::Json => Self::Json,
        }
    }
}

/// Parses the command line, loads `config.json`, merges and validates the
/// settings, and boots logging (printing the header from the merged
/// settings when `--app-header` is set). The config notice prints to stderr
/// whether or not the merge succeeds.
///
/// # Errors
/// Fails when an environment variable is not Unicode, when `config.json`
/// cannot be read or is an invalid eh config, or when the merged settings
/// are invalid.
pub async fn initialize() -> Result<EhCommand> {
    let args = CliArgs::parse();
    let (notice, resolved) = resolve(args.command, args.common.verbose).await;

    match resolved {
        Ok(command) => {
            args.common.app_boot_up(
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_VERSION"),
                true,
                false,
                Some(|| print_header(&command)),
            );
            print_notice(notice.as_deref());
            Ok(command)
        }
        Err(error) => {
            args.common.app_boot_up(
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_VERSION"),
                true,
                false,
                None::<fn()>,
            );
            print_notice(notice.as_deref());
            Err(error)
        }
    }
}

/// Loads `config.json` and merges it with the arguments. Returns the config
/// notice (when there is one to show) alongside the result.
async fn resolve(command: Commands, verbose: bool) -> (Option<String>, Result<EhCommand>) {
    let environment = match Environment::from_process() {
        Ok(environment) => environment,
        Err(error) => return (None, Err(error)),
    };
    let working_dir = get_current_dir();
    // A dry run resolves no target, so it never reads config.json.
    let no_config = match &command {
        Commands::Read(args) => args.data.no_config,
        Commands::Export(args) => args.data.no_config,
        Commands::Send(args) => args.data.no_config || args.dry_run,
    };
    let (recalled, source) = if no_config {
        (None, ConfigSource::Disabled)
    } else {
        match load_config(&working_dir).await {
            Ok(ConfigLoad::Missing) => (None, ConfigSource::Missing),
            Ok(ConfigLoad::Foreign) => (None, ConfigSource::Foreign),
            Ok(ConfigLoad::Loaded(config)) => (Some(*config), ConfigSource::Loaded),
            Err(error) => return (None, Err(error)),
        }
    };

    let notice = config_notice(source, recalled.as_ref());
    let command = match command {
        Commands::Read(args) => read_job(
            *args,
            recalled.as_ref(),
            &environment,
            verbose,
            working_dir,
            source,
        )
        .map(|job| EhCommand::Read(Box::new(job))),
        Commands::Export(args) => {
            export_job(*args, recalled.as_ref(), source).map(|job| EhCommand::Export(Box::new(job)))
        }
        Commands::Send(args) => {
            send_command(*args, recalled.as_ref(), &environment, working_dir, source)
        }
    };
    (notice, command)
}

/// Loads the body, compiles it under `--template`, and builds the send or
/// the dry run. A body or template error returns before any target is
/// resolved.
fn send_command(
    args: SendArgs,
    recalled: Option<&ToolConfig>,
    environment: &Environment,
    working_dir: PathBuf,
    source: ConfigSource,
) -> Result<EhCommand> {
    let body = load_body(&args)?;
    let template = if args.template {
        Some(Arc::new(compile_template(&body.0)?))
    } else {
        None
    };
    match (args.dry_run, template) {
        (true, Some(template)) => Ok(EhCommand::DryRun(Box::new(DryRunJob {
            template,
            count: args.count,
            body_len: body.0.len(),
            body_source: body.1,
        }))),
        (true, None) => anyhow::bail!("--dry-run needs --template"),
        (false, template) => send_job(
            args,
            body,
            template,
            recalled,
            environment,
            working_dir,
            source,
        )
        .map(|job| EhCommand::Send(Box::new(job))),
    }
}

fn print_notice(notice: Option<&str>) {
    if let Some(notice) = notice {
        eprintln!("{notice}");
    }
}

/// The stderr line saying what `config.json` contributed. A recalled config
/// is shown in full (target and folders) because a cloned repository can
/// ship one.
pub(crate) fn config_notice(source: ConfigSource, recalled: Option<&ToolConfig>) -> Option<String> {
    match (source, recalled) {
        (ConfigSource::Foreign, _) => {
            Some("config.json here is not an eh config; ignored".to_owned())
        }
        (ConfigSource::Loaded, Some(config)) => Some(format!(
            "Using config.json: {}, auth {}, consumer group {}, data folder {}, dump folder {} \
             (flags override these)",
            target_label(&config.target),
            describe_auth(&config.target.auth),
            display(&config.target.consumer_group),
            display(&config.storage.base_data_folder),
            display(&config.storage.disk_write.received_msg_path),
        )),
        (ConfigSource::Loaded, None) | (ConfigSource::Disabled | ConfigSource::Missing, _) => None,
    }
}

/// Prints the tool's runtime configuration under `--app-header`. Secrets
/// never appear: the auth line names the method only.
fn print_header(command: &EhCommand) {
    let lines = match command {
        EhCommand::Read(job) => read_header(job),
        EhCommand::Export(job) => export_header(job),
        EhCommand::Send(job) => send_header(job),
        EhCommand::DryRun(job) => dry_run_header(job),
    };
    for line in lines {
        println!("{line}");
    }
}

pub(crate) fn item(label: &str, value: impl Display) -> String {
    format_config_item(label, escape_for_terminal_display(&value.to_string()))
}

pub(crate) fn read_header(job: &ReadJob) -> Vec<String> {
    let config = &job.config;
    let capture = &config.capture;
    let disk_write = &config.storage.disk_write;
    let partitions = match &capture.partitions {
        Some(partitions) => partitions.join(", "),
        None => "all".to_owned(),
    };
    vec![
        item("Command", "read"),
        format_config_item("Target", target_label(&config.target)),
        format_config_item("Auth", describe_auth(&config.target.auth)),
        item("Consumer group", &config.target.consumer_group),
        item("Partitions", partitions),
        item("Ignore checkpoint", capture.ignore_checkpoint),
        item(
            "Feedback interval",
            format!("{} s", capture.feedback_interval_secs),
        ),
        format_config_item(
            "Time filter",
            describe_time_filter(capture.filters.time.as_ref()),
        ),
        format_config_item(
            "Text filter",
            describe_text_filter(capture.filters.text.as_ref()),
        ),
        item("Data folder", &config.storage.base_data_folder),
        item("Disk mode", disk_mode_name(disk_write.mode)),
        item(
            "Dump folder",
            DataPaths::new(&config.storage.base_data_folder)
                .inbound_dir(&disk_write.received_msg_path)
                .display(),
        ),
        item("Dump content only", disk_write.content_only),
        item("Save config", job.save_config),
        item("config.json", config_source_name(job.config_source)),
    ]
}

pub(crate) fn export_header(job: &ExportJob) -> Vec<String> {
    let export = &job.export;
    vec![
        item("Command", "export"),
        item("Data folder", job.base_data_folder.display()),
        item(
            "Export folder",
            DataPaths::new(&job.base_data_folder)
                .export_dir(&export.export_folder)
                .display(),
        ),
        item("Format", format_name(export.format)),
        item("Include metadata", export.include_metadata),
        item("Condense output", export.condense_output),
        item("Use local time", export.use_local_time),
        item("Skip already exported", export.skip_already_exported),
        format_config_item(
            "Time filter",
            describe_time_filter(job.filters.time.as_ref()),
        ),
        format_config_item(
            "Text filter",
            describe_text_filter(job.filters.text.as_ref()),
        ),
        item("config.json", config_source_name(job.config_source)),
    ]
}

fn disk_mode_name(mode: DiskWriteMode) -> &'static str {
    match mode {
        DiskWriteMode::Off => "off",
        DiskWriteMode::Immediate => "immediate",
        DiskWriteMode::AtEnd => "at-end",
    }
}

fn format_name(format: ExportFormat) -> &'static str {
    match format {
        ExportFormat::Txt => "txt",
        ExportFormat::Csv => "csv",
        ExportFormat::Json => "json",
    }
}

pub(crate) fn config_source_name(source: ConfigSource) -> &'static str {
    match source {
        ConfigSource::Disabled => "not read (--no-config)",
        ConfigSource::Missing => "none",
        ConfigSource::Foreign => "ignored (not an eh config)",
        ConfigSource::Loaded => "used",
    }
}

#[cfg(test)]
mod tests;

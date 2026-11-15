//! `eh send` arguments, loading the body they name, and the run header.

use crate::cli_utils::{DataArgs, TargetArgs, config_source_name, item};
use crate::models::{BodySource, SendJob};
use crate::progress::{describe_auth, display, target_label};
use anyhow::Context;
use clap::{ArgGroup, Args};
use common_cli::header_format::format_config_item;
use shared_eventhub_core::producer::{MAX_SEND_COUNT, MAX_SEND_INTERVAL_MS};
use std::convert::Infallible;
use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};

/// The largest body `eh send` reads, in bytes: 20 MiB, above the largest
/// publication any Event Hubs tier accepts (20 MB on Dedicated). It guards
/// memory; the SDK refuses events far smaller than this.
pub(crate) const MAX_BODY_BYTES: u64 = 20 * 1024 * 1024;

/// `eh send` arguments.
#[derive(Args, Debug)]
#[command(group = ArgGroup::new("body_source").required(true).multiple(false))]
pub(crate) struct SendArgs {
    #[command(flatten)]
    pub target: TargetArgs,

    #[command(flatten)]
    pub data: DataArgs,

    /// The event body, sent as UTF-8
    #[arg(
        long = "body",
        value_name = "TEXT",
        group = "body_source",
        value_parser = parse_body_text
    )]
    pub body: Option<BodyText>,

    /// Read the event body from a file (sent byte for byte); `-` reads stdin
    #[arg(long = "body-file", value_name = "PATH", group = "body_source")]
    pub body_file: Option<PathBuf>,

    /// How many times to send the body
    #[arg(
        short = 'n',
        long = "count",
        value_name = "N",
        default_value_t = 1,
        value_parser = clap::value_parser!(u64).range(1..=MAX_SEND_COUNT)
    )]
    pub count: u64,

    /// Partition key for every event; events with one key land in one
    /// partition, in order [default: the service picks]
    #[arg(long = "partition-key", value_name = "KEY")]
    pub partition_key: Option<String>,

    /// Content type of every event, e.g. application/json
    #[arg(long = "content-type", value_name = "TYPE")]
    pub content_type: Option<String>,

    /// Send one event every MS milliseconds (start to start) instead of in
    /// full batches
    #[arg(
        long = "interval-ms",
        value_name = "MS",
        value_parser = clap::value_parser!(u64).range(1..=MAX_SEND_INTERVAL_MS)
    )]
    pub interval_ms: Option<u64>,

    /// Send to the event hub recalled from config.json without naming it
    /// again
    #[arg(long = "yes")]
    pub yes: bool,
}

/// Reads the body `args` names: `--body` as UTF-8 bytes, `--body-file` from
/// the file or (for `-`) stdin, refused past [`MAX_BODY_BYTES`].
///
/// # Errors
/// Fails when no body source is given, when the file cannot be read, or when
/// the file or stdin is over the cap; the error names the path and the cap.
pub(crate) fn load_body(args: &SendArgs) -> anyhow::Result<(Vec<u8>, BodySource)> {
    match (&args.body, &args.body_file) {
        // A command-line argument cannot come near the cap (the OS limits
        // one to a few hundred KB at most), so only files and stdin are
        // checked.
        (Some(body), _) => Ok((body.0.as_bytes().to_vec(), BodySource::Inline)),
        (None, Some(path)) if path.as_os_str() == "-" => {
            let body = read_capped(std::io::stdin().lock(), MAX_BODY_BYTES, "stdin")?;
            Ok((body, BodySource::Stdin))
        }
        (None, Some(path)) => Ok((read_file(path)?, BodySource::File(path.clone()))),
        (None, None) => anyhow::bail!("Give the body with --body or --body-file"),
    }
}

fn read_file(path: &Path) -> anyhow::Result<Vec<u8>> {
    let what = format!("the body file '{}'", display(path.display().to_string()));
    let metadata = std::fs::metadata(path).with_context(|| format!("Failed to read {what}"))?;
    if metadata.len() > MAX_BODY_BYTES {
        anyhow::bail!("Refusing {what}: it is over the {MAX_BODY_BYTES}-byte limit");
    }
    let file = std::fs::File::open(path).with_context(|| format!("Failed to read {what}"))?;
    read_capped(file, MAX_BODY_BYTES, &what)
}

/// Reads `reader` to its end, failing once it yields more than `cap` bytes
/// (a growing file or a pipe passes any size check made before reading).
/// `what` names the source in errors.
fn read_capped(reader: impl Read, cap: u64, what: &str) -> anyhow::Result<Vec<u8>> {
    let mut body = Vec::new();
    reader
        .take(cap + 1)
        .read_to_end(&mut body)
        .with_context(|| format!("Failed to read {what}"))?;
    if body.len() as u64 > cap {
        anyhow::bail!("Refusing {what}: it is over the {cap}-byte limit");
    }
    Ok(body)
}

/// The `--body` text. `Debug` shows its length only, so a parsed command
/// line never prints the body.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct BodyText(pub String);

impl fmt::Debug for BodyText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "BodyText({} bytes)", self.0.len())
    }
}

fn parse_body_text(value: &str) -> Result<BodyText, Infallible> {
    Ok(BodyText(value.to_owned()))
}

/// The `--app-header` lines for a send run. The body is described by its
/// source and size, never its content.
pub(crate) fn send_header(job: &SendJob) -> Vec<String> {
    let config = &job.config;
    let routing = match job.plan.routing().key() {
        Some(key) => format!("partition key {}", display(key)),
        None => "automatic".to_owned(),
    };
    let pacing = match job.plan.pacing().get() {
        Some(interval) => format!("one event every {} ms", interval.as_millis()),
        None => "batched".to_owned(),
    };
    let source = match &job.body_source {
        BodySource::Inline => "--body".to_owned(),
        BodySource::File(path) => format!("file {}", display(path.display().to_string())),
        BodySource::Stdin => "stdin".to_owned(),
    };
    vec![
        item("Command", "send"),
        format_config_item("Target", target_label(&config.target)),
        format_config_item("Auth", describe_auth(&config.target.auth)),
        item("Data folder", &config.storage.base_data_folder),
        item("Count", job.plan.count()),
        format_config_item("Routing", routing),
        item("Pacing", pacing),
        format_config_item("Body", format!("{source}, {} bytes", job.event.body.len())),
        item(
            "Content type",
            job.event.content_type.as_deref().unwrap_or("none"),
        ),
        item("Save config", job.save_config),
        item("config.json", config_source_name(job.config_source)),
    ]
}

#[cfg(test)]
mod tests;

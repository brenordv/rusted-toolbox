//! `eh send` arguments, loading the body they name, templates, and the run
//! header.

use crate::cli_utils::{DataArgs, TargetArgs, command_lines, config_source_name, item};
use crate::models::{BodySource, DryRunJob, SendJob};
use crate::progress::{describe_auth, display, target_label};
use anyhow::Context;
use clap::{ArgGroup, Args};
use common_cli::header_format::format_config_item;
use mock_data_utils::template::{OPTIONS_HELP, Template, placeholder_groups};
use shared_eventhub_core::producer::{MAX_SEND_COUNT, MAX_SEND_INTERVAL_MS};
use std::collections::BTreeMap;
use std::convert::Infallible;
use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};

/// The largest body `eh send` reads, in bytes: 20 MiB, above the largest
/// publication any Event Hubs tier accepts (20 MB on Dedicated). It guards
/// memory; the SDK refuses events far smaller than this.
pub(crate) const MAX_BODY_BYTES: u64 = 20 * 1024 * 1024;

/// The largest body one template render may produce, the same 20 MiB as a
/// body read from a file.
pub(crate) const RENDER_CAP: usize = MAX_BODY_BYTES as usize;

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
        value_parser = parse_arg_text
    )]
    pub body: Option<ArgText>,

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

    /// Content type of every event, e.g. application/json; trimmed, and a
    /// blank value sets none
    #[arg(long = "content-type", value_name = "TYPE")]
    pub content_type: Option<String>,

    /// An application property on every event, as KEY=VALUE (repeatable;
    /// split at the first =, key and value trimmed, each key once). Sent as
    /// plain text: never put a secret here
    // The value parser cannot fail: clap's own validation error would print
    // the raw value unescaped, so `send_job` parses these and reports through
    // main's escaped error path.
    #[arg(long = "property", value_name = "KEY=VALUE", value_parser = parse_arg_text)]
    pub properties: Vec<ArgText>,

    /// Correlation id of every event; trimmed, and a blank value sets none
    #[arg(long = "correlation-id", value_name = "ID", value_parser = parse_arg_text)]
    pub correlation_id: Option<ArgText>,

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

    /// Treat the body as a template: every {{type option=value}} placeholder
    /// gets a fresh mock value in each event, {{now}} and {{utc-now}} the time
    /// the event is built (see the list below)
    #[arg(long = "template")]
    pub template: bool,

    /// Print the rendered events instead of sending them; needs no event hub
    /// and reads no config.json
    #[arg(
        long = "dry-run",
        requires = "template",
        conflicts_with_all = [
            "save_config",
            "partition_key",
            "interval_ms",
            "yes",
            "properties",
            "correlation_id",
            "profile"
        ]
    )]
    pub dry_run: bool,
}

/// The `after_long_help` text of `eh send`: the placeholder names by group
/// and the option words.
pub(crate) fn template_help() -> String {
    let mut help =
        String::from("Template placeholders (with --template; \\{{ writes a literal {{):\n");
    for (prefix, commands) in placeholder_groups() {
        help.push_str(&format!("  {prefix}: {}\n", commands.join(", ")));
    }
    help.push_str("\nPlaceholder options:\n");
    for line in OPTIONS_HELP.lines() {
        help.push_str(&format!("  {line}\n"));
    }
    help
}

/// Compiles `body` as a template.
///
/// # Errors
/// Fails when `body` is not UTF-8, naming the offset of the first bad byte,
/// and when the template does not compile, naming the line and column.
pub(crate) fn compile_template(body: &[u8]) -> anyhow::Result<Template> {
    let source = std::str::from_utf8(body).map_err(|error| {
        anyhow::anyhow!(
            "The --template body is not UTF-8: the byte at offset {} is not valid",
            error.valid_up_to()
        )
    })?;
    Template::compile(source).context("The --template body does not compile")
}

/// The stderr notices a template run starts with, escaped: a template with
/// no placeholders, and each locale that has no effect.
pub(crate) fn template_notices(template: &Template) -> Vec<String> {
    let mut notices = Vec::new();
    if template.placeholder_count() == 0 {
        notices.push("The template has no placeholders; every event is the same.".to_owned());
    }
    notices.extend(template.inert_locale_notes().iter().map(display));
    notices
}

fn template_line(template: Option<&Template>) -> String {
    match template {
        Some(template) => match template.placeholder_count() {
            1 => "yes, 1 placeholder".to_owned(),
            count => format!("yes, {count} placeholders"),
        },
        None => "no".to_owned(),
    }
}

/// The most property keys the header names.
const HEADER_KEYS: usize = 5;

/// "none", or the count and the first [`HEADER_KEYS`] keys: "7: a, b, c, d,
/// e and 2 more". Values never show.
fn properties_line(properties: &BTreeMap<String, String>) -> String {
    if properties.is_empty() {
        return "none".to_owned();
    }
    let keys: Vec<&str> = properties
        .keys()
        .take(HEADER_KEYS)
        .map(String::as_str)
        .collect();
    let mut line = format!("{}: {}", properties.len(), keys.join(", "));
    if properties.len() > HEADER_KEYS {
        line.push_str(&format!(" and {} more", properties.len() - HEADER_KEYS));
    }
    line
}

fn body_line(source: &BodySource, bytes: usize) -> String {
    let source = match source {
        BodySource::Inline => "--body".to_owned(),
        BodySource::File(path) => format!("file {}", display(path.display().to_string())),
        BodySource::Stdin => "stdin".to_owned(),
    };
    format!("{source}, {bytes} bytes")
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

/// A flag's text that `Debug` shows by length only (the body, a property, a
/// correlation id), so a parsed command line never prints it.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ArgText(pub String);

impl fmt::Debug for ArgText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "ArgText({} bytes)", self.0.len())
    }
}

fn parse_arg_text(value: &str) -> Result<ArgText, Infallible> {
    Ok(ArgText(value.to_owned()))
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
    let mut lines = command_lines("send", &job.home);
    lines.extend([
        format_config_item("Target", target_label(&config.target)),
        format_config_item("Auth", describe_auth(&config.target.auth)),
        item("Data folder", job.paths.base().display()),
        item("Count", job.plan.count()),
        format_config_item("Routing", routing),
        item("Pacing", pacing),
        format_config_item("Body", body_line(&job.body_source, job.event.body.len())),
        item("Template", template_line(job.template.as_deref())),
        item(
            "Content type",
            job.event.content_type.as_deref().unwrap_or("none"),
        ),
        item(
            "Correlation id",
            job.event.correlation_id.as_deref().unwrap_or("none"),
        ),
        item("Properties", properties_line(&job.event.properties)),
        item("Save config", job.save_config),
        item("config.json", config_source_name(job.config_source)),
    ]);
    lines
}

/// The `--app-header` lines for a dry run.
pub(crate) fn dry_run_header(job: &DryRunJob) -> Vec<String> {
    vec![
        item("Command", "send --dry-run"),
        item("Count", job.count),
        format_config_item("Body", body_line(&job.body_source, job.body_len)),
        item("Template", template_line(Some(&job.template))),
    ]
}

#[cfg(test)]
mod tests;

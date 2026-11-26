//! Terminal rendering: the live progress lines, message previews, notices,
//! and the end-of-run blocks, plus the one-line descriptions the header and
//! notices share. Every value that came from outside the program (event
//! data, `config.json`, error text) is escaped before it reaches the
//! terminal.

use chrono::{DateTime, Local, Utc};
use common_utils::string_utils::{escape_for_terminal_display, format_duration_to_string};
use crossterm::cursor::MoveToColumn;
use crossterm::queue;
use crossterm::style::{Print, Stylize};
use crossterm::terminal::{Clear, ClearType};
use eh_core::engine::{EngineEvent, PartitionEnd, RunEnd, RunSummary};
use eh_core::filters::{TextFilter, TimeFilter};
use eh_core::models::CapturedMessage;
use eh_core::stats::StatsSnapshot;
use eh_persistence::config::{AuthMethodConfig, CredentialKindConfig, TargetConfig};
use eh_persistence::export::ExportOutcome;
use std::io::{self, IsTerminal, Stderr, Stdout, Write};
use std::time::{Duration, Instant};
use tracing::warn;

/// Width used when the output is not a terminal.
const DEFAULT_WIDTH: usize = 120;
/// Shortest pause between two redraws of the export progress line.
const EXPORT_REDRAW_INTERVAL: Duration = Duration::from_millis(250);
const FROM_CONNECTION_STRING: &str = "(from the connection string)";

/// Escapes `value` for the terminal.
pub(crate) fn display(value: impl AsRef<str>) -> String {
    escape_for_terminal_display(value.as_ref())
}

/// `namespace/hub`, with "(from the connection string)" or "(not set)" for a
/// part that is not known before connecting.
pub(crate) fn target_label(target: &TargetConfig) -> String {
    let fallback = match target.auth {
        AuthMethodConfig::ConnectionString { .. }
        | AuthMethodConfig::KeyVaultConnectionString { .. } => FROM_CONNECTION_STRING,
        AuthMethodConfig::Credential { .. } => "(not set)",
    };
    let part = |value: &Option<String>| {
        value
            .as_deref()
            .map_or_else(|| fallback.to_owned(), display)
    };

    format!(
        "namespace {}, event hub {}",
        part(&target.namespace),
        part(&target.event_hub)
    )
}

/// The auth method by name, never a secret.
pub(crate) fn describe_auth(auth: &AuthMethodConfig) -> String {
    match auth {
        AuthMethodConfig::ConnectionString { .. } => "connection string".to_owned(),
        AuthMethodConfig::Credential { credential } => describe_credential(credential),
        AuthMethodConfig::KeyVaultConnectionString {
            vault_url,
            secret_name,
            credential,
        } => format!(
            "Key Vault secret {} in {} via {}",
            display(secret_name),
            display(vault_url),
            describe_credential(credential)
        ),
    }
}

fn describe_credential(credential: &CredentialKindConfig) -> String {
    match credential {
        CredentialKindConfig::DeveloperTools => "developer tools credential".to_owned(),
        CredentialKindConfig::ManagedIdentity => "managed identity".to_owned(),
        CredentialKindConfig::ClientSecret {
            tenant_id,
            client_id,
        } => format!(
            "client secret (tenant {}, client {})",
            display(tenant_id),
            display(client_id)
        ),
    }
}

/// The time filter in words, or "none".
pub(crate) fn describe_time_filter(filter: Option<&TimeFilter>) -> String {
    let at = |time: &DateTime<Utc>| time.to_rfc3339();
    match filter {
        None => "none".to_owned(),
        Some(TimeFilter::OlderThan(time)) => format!("older than {}", at(time)),
        Some(TimeFilter::OlderOrEqual(time)) => format!("at or before {}", at(time)),
        Some(TimeFilter::NewerThan(time)) => format!("newer than {}", at(time)),
        Some(TimeFilter::NewerOrEqual(time)) => format!("at or after {}", at(time)),
        Some(TimeFilter::During(time)) => format!("during {}", at(time)),
        Some(TimeFilter::Between { start, end }) => {
            format!("from {} to {}", at(start), at(end))
        }
    }
}

/// The text filter in words, or "none".
pub(crate) fn describe_text_filter(filter: Option<&TextFilter>) -> String {
    match filter {
        None => "none".to_owned(),
        Some(TextFilter::Contains(keywords)) => {
            format!("contains any of {}", display(keywords.join(", ")))
        }
        Some(TextFilter::Regex(regex)) => format!("matches {}", display(regex.as_str())),
    }
}

/// The stderr line shown while the run connects, naming `consumer_group`
/// when there is one.
pub(crate) fn connecting_notice(target: &TargetConfig, consumer_group: Option<&str>) -> String {
    match (&target.auth, consumer_group) {
        (AuthMethodConfig::KeyVaultConnectionString { .. }, Some(group)) => format!(
            "Connecting (connection string from Key Vault, consumer group {})...",
            display(group)
        ),
        (AuthMethodConfig::KeyVaultConnectionString { .. }, None) => {
            "Connecting (connection string from Key Vault)...".to_owned()
        }
        (
            AuthMethodConfig::ConnectionString { .. } | AuthMethodConfig::Credential { .. },
            Some(group),
        ) => format!(
            "Connecting to {} (consumer group {})...",
            target_label(target),
            display(group)
        ),
        (AuthMethodConfig::ConnectionString { .. } | AuthMethodConfig::Credential { .. }, None) => {
            format!("Connecting to {}...", target_label(target))
        }
    }
}

/// The live read progress line.
pub(crate) fn progress_line(snapshot: &StatsSnapshot) -> String {
    let last = snapshot.last_message_at.map_or_else(
        || "never".to_owned(),
        |time| {
            time.with_timezone(&Local)
                .format("%H:%M:%S%.3f")
                .to_string()
        },
    );
    format!(
        "Received: {} | Stored: {} | Duplicated: {} | Filtered: {} | Rate: {:.2} msg/s | \
         Runtime: {} | Last: {last}",
        snapshot.received,
        snapshot.stored,
        snapshot.duplicated,
        snapshot.filtered_out,
        snapshot.current_rate,
        format_elapsed(snapshot.elapsed),
    )
}

/// The export (and at-end message file) progress line.
pub(crate) fn export_line(outcome: &ExportOutcome, elapsed: Duration) -> String {
    let seconds = elapsed.as_secs_f64();
    let rate = if seconds > 0.0 {
        outcome.exported as f64 / seconds
    } else {
        0.0
    };
    format!(
        "Exported: {} | Skipped: {} | Failed: {} | Rate: {rate:.2} msg/s | Runtime: {}",
        outcome.exported,
        outcome.skipped,
        outcome.failed,
        format_elapsed(elapsed)
    )
}

/// `HH:MM:SS.mmm`.
pub(crate) fn format_elapsed(elapsed: Duration) -> String {
    // A run would need to last about 292 million years to overflow chrono's
    // millisecond range; saturate rather than fail a display string.
    let elapsed = chrono::Duration::from_std(elapsed).unwrap_or(chrono::Duration::MAX);
    format_duration_to_string(elapsed)
}

/// One line describing a captured message: position, size, and the start of
/// its body, escaped and cut to `width` characters.
pub(crate) fn preview_line(message: &CapturedMessage, width: usize) -> String {
    let body: String = message.body.chars().take(width).collect();
    let line = format!(
        "Partition/Sequence: {}/{} | Bytes: {} | Preview: {}",
        display(&message.key.partition_id),
        message.key.sequence_number,
        message.body.len(),
        display(body)
    );
    truncate(&line, width)
}

/// Cuts `line` to `width` characters, ending in "..." when it was longer.
pub(crate) fn truncate(line: &str, width: usize) -> String {
    if line.chars().count() <= width {
        return line.to_owned();
    }
    let kept: String = line.chars().take(width.saturating_sub(3)).collect();
    format!("{kept}...")
}

/// The block printed when a capture run ends.
pub(crate) fn read_summary_block(summary: &RunSummary, at_end: Option<&ExportOutcome>) -> String {
    let stats = &summary.stats;
    let ended = match &summary.end {
        RunEnd::Completed => "completed".to_owned(),
        RunEnd::Stopped => "stopped on request".to_owned(),
        RunEnd::Failed(reason) => format!("failed: {}", display(reason)),
    };
    let mut block = format!(
        "Run ended: {ended}\n\
         Received: {} | Stored: {} | Duplicated: {} | Filtered out: {} | Unkeyable: {} | \
         UI dropped: {}\n\
         Average rate: {:.2} msg/s | Peak rate: {:.2} msg/s | Elapsed: {}\n",
        stats.received,
        stats.stored,
        stats.duplicated,
        stats.filtered_out,
        stats.unkeyable,
        stats.ui_dropped,
        stats.average_rate,
        stats.peak_rate,
        format_elapsed(stats.elapsed),
    );
    if !summary.partitions.is_empty() {
        block.push_str("Partitions:\n");
        for (partition_id, end) in &summary.partitions {
            block.push_str(&format!(
                "  - {}: {}\n",
                display(partition_id),
                display(end.to_string())
            ));
        }
    }
    if let Some(outcome) = at_end {
        block.push_str(&format!(
            "Message files written at the end: {}, failed: {}\n",
            outcome.exported, outcome.failed
        ));
        push_first_error(&mut block, outcome);
    }
    block
}

/// The block printed when an export ends.
pub(crate) fn export_summary_block(outcome: &ExportOutcome) -> String {
    let mut block = format!(
        "Exported: {} | Skipped (already exported): {} | Failed: {}\n",
        outcome.exported, outcome.skipped, outcome.failed
    );
    push_first_error(&mut block, outcome);
    if outcome.cancelled {
        block.push_str("Cancelled; every written file is complete.\n");
    }
    block
}

fn push_first_error(block: &mut String, outcome: &ExportOutcome) {
    if let Some(first_error) = &outcome.first_error {
        block.push_str(&format!("First failure: {}\n", display(first_error)));
    }
}

fn terminal_width() -> usize {
    // No terminal size (output redirected to a file or a pipe) is normal:
    // the lines are still written, cut at a fixed width.
    crossterm::terminal::size().map_or(DEFAULT_WIDTH, |(columns, _)| usize::from(columns))
}

/// Writes the read run's terminal output: the progress line and previews to
/// `out`, notices to `err`. Writing is best-effort: a failed write never
/// stops a capture (the archive is the record), so the first failure is
/// logged once and later ones are dropped.
pub(crate) struct Renderer<O: Write, E: Write> {
    out: O,
    err: E,
    verbose: bool,
    width: usize,
    line_open: bool,
    write_failed: bool,
}

impl Renderer<Stdout, Stderr> {
    pub(crate) fn terminal(verbose: bool) -> Self {
        Self::new(io::stdout(), io::stderr(), verbose, terminal_width())
    }
}

impl<O: Write, E: Write> Renderer<O, E> {
    pub(crate) fn new(out: O, err: E, verbose: bool, width: usize) -> Self {
        Self {
            out,
            err,
            verbose,
            width,
            line_open: false,
            write_failed: false,
        }
    }

    /// Prints `text` (already escaped) on its own stderr line, above the
    /// progress line.
    pub(crate) fn notice(&mut self, text: &str) {
        self.close_line();
        let result = writeln!(self.err, "{}", text.yellow()).and_then(|()| self.err.flush());
        self.record(result);
    }

    /// Redraws the progress line.
    pub(crate) fn progress(&mut self, snapshot: &StatsSnapshot) {
        let line = truncate(&progress_line(snapshot), self.width.saturating_sub(1));
        let result = queue!(
            self.out,
            MoveToColumn(0),
            Clear(ClearType::CurrentLine),
            Print(line.green())
        )
        .and_then(|()| self.out.flush());
        self.line_open = true;
        self.record(result);
    }

    /// Shows one engine event: the partition list, a preview with
    /// `--verbose`, and how a partition ended unless it was stopped. Errors
    /// are not shown here: a partition's failure arrives again as its end,
    /// and a run that cannot start returns its error to the caller.
    pub(crate) fn engine_event(&mut self, event: &EngineEvent) {
        match event {
            EngineEvent::Started { partitions } => {
                self.notice(&format!(
                    "Reading partitions: {}",
                    display(partitions.join(", "))
                ));
            }
            EngineEvent::Message(message) => self.preview(message),
            EngineEvent::Skipped { .. } | EngineEvent::Error { .. } => {}
            EngineEvent::PartitionEnded { partition_id, end } => match end {
                PartitionEnd::Stopped => {}
                PartitionEnd::UpperBoundPassed
                | PartitionEnd::NothingLeftInWindow
                | PartitionEnd::Failed(_) => self.notice(&format!(
                    "Partition {} {}",
                    display(partition_id),
                    display(end.to_string())
                )),
            },
        }
    }

    /// Ends an open progress line so the next output starts on a new line.
    pub(crate) fn finish_line(&mut self) {
        if self.line_open {
            self.line_open = false;
            let result = writeln!(self.out).and_then(|()| self.out.flush());
            self.record(result);
        }
    }

    /// Prints a multi-line block (already escaped) to `out`.
    pub(crate) fn block(&mut self, text: &str) {
        self.finish_line();
        let result = write!(self.out, "{text}").and_then(|()| self.out.flush());
        self.record(result);
    }

    fn preview(&mut self, message: &CapturedMessage) {
        if !self.verbose {
            return;
        }
        self.close_line();
        let line = preview_line(message, self.width.saturating_sub(1));
        let result = writeln!(self.out, "{}", line.blue()).and_then(|()| self.out.flush());
        self.record(result);
    }

    /// Clears an open progress line so other output takes its place.
    fn close_line(&mut self) {
        if self.line_open {
            self.line_open = false;
            let result = queue!(self.out, MoveToColumn(0), Clear(ClearType::CurrentLine))
                .and_then(|()| self.out.flush());
            self.record(result);
        }
    }

    fn record(&mut self, result: io::Result<()>) {
        if let Err(error) = result
            && !self.write_failed
        {
            self.write_failed = true;
            warn!(error = %error, "Terminal output failed; later output errors are not reported");
        }
    }

    #[cfg(test)]
    pub(crate) fn into_parts(self) -> (O, E) {
        (self.out, self.err)
    }
}

/// A single progress line (export, at-end message files, send), redrawn at
/// most every 250 ms. Writing is best-effort, as for [`Renderer`].
pub(crate) struct ExportProgressLine<W: Write> {
    out: W,
    /// Whether `out` is a terminal, where a finished line can be erased; a
    /// file or a pipe keeps every byte, so there the line is ended instead.
    terminal: bool,
    started: Instant,
    last_draw: Option<Instant>,
    drawn: bool,
    write_failed: bool,
}

impl ExportProgressLine<Stdout> {
    pub(crate) fn stdout() -> Self {
        let out = io::stdout();
        let terminal = out.is_terminal();
        Self::new(out, terminal)
    }
}

impl<W: Write> ExportProgressLine<W> {
    pub(crate) fn new(out: W, terminal: bool) -> Self {
        Self {
            out,
            terminal,
            started: Instant::now(),
            last_draw: None,
            drawn: false,
            write_failed: false,
        }
    }

    /// Redraws the export counts unless the line was drawn less than 250 ms
    /// ago.
    pub(crate) fn draw(&mut self, outcome: &ExportOutcome) {
        self.draw_with(|elapsed| export_line(outcome, elapsed));
    }

    /// Redraws the line with the text `line` builds from the time since the
    /// line was created, unless it was drawn less than 250 ms ago.
    pub(crate) fn draw_with(&mut self, line: impl FnOnce(Duration) -> String) {
        let now = Instant::now();
        if self
            .last_draw
            .is_some_and(|last| now.duration_since(last) < EXPORT_REDRAW_INTERVAL)
        {
            return;
        }
        self.last_draw = Some(now);
        self.render(&line(self.started.elapsed()));
    }

    /// Ends the line. On a terminal, when a summary with the final counts is
    /// printed next (`summary_follows`), the line is erased so the counts show
    /// once; otherwise the last counts drawn stay on their own line.
    pub(crate) fn finish(&mut self, summary_follows: bool) {
        if !self.drawn {
            return;
        }
        let result = if summary_follows && self.terminal {
            queue!(self.out, MoveToColumn(0), Clear(ClearType::CurrentLine))
        } else {
            writeln!(self.out)
        }
        .and_then(|()| self.out.flush());
        self.record(result);
    }

    fn render(&mut self, line: &str) {
        let line = truncate(line, terminal_width().saturating_sub(1));
        let result = queue!(
            self.out,
            MoveToColumn(0),
            Clear(ClearType::CurrentLine),
            Print(line.cyan())
        )
        .and_then(|()| self.out.flush());
        self.drawn = true;
        self.record(result);
    }

    fn record(&mut self, result: io::Result<()>) {
        if let Err(error) = result
            && !self.write_failed
        {
            self.write_failed = true;
            warn!(error = %error, "Terminal output failed; later output errors are not reported");
        }
    }

    #[cfg(test)]
    pub(crate) fn into_inner(self) -> W {
        self.out
    }
}

#[cfg(test)]
mod tests;

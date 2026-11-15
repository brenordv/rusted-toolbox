//! `eh send --dry-run`: renders the template the given number of times and
//! prints the events instead of sending them.

use crate::models::{DryRunJob, RunOutcome};
use crate::progress::Renderer;
use crate::send_cli::{RENDER_CAP, template_notices};
use anyhow::Context;
use chrono::Utc;
use common_cli::broken_pipe::{BrokenPipe, flush_out, write_out};
use common_utils::string_utils::escape_for_terminal_display;
use mock_data_utils::template::Template;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};

/// Prints `job`'s renders to stdout, one event at a time, and a summary to
/// stderr.
///
/// # Errors
/// Fails when a render fails (naming the event's index and the placeholder)
/// and when stdout fails for any reason but a closed reader.
pub fn run(job: DryRunJob, shutdown: &AtomicBool) -> anyhow::Result<RunOutcome> {
    let mut renderer = Renderer::terminal(false);
    for notice in template_notices(&job.template) {
        renderer.notice(&notice);
    }
    let out = io::stdout().lock();
    let printed = print_renders(&job.template, job.count, RENDER_CAP, out, shutdown)?;
    let (summary, outcome) = ending(&printed, job.count);
    renderer.notice(&summary);
    Ok(outcome)
}

/// How printing ended.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Printed {
    /// Every event was printed.
    All,
    /// The shutdown flag stopped it after this many events.
    Interrupted(u64),
    /// The reader of `out` went away after this many events (`| head`).
    ReaderClosed(u64),
}

/// The summary line and the run's outcome for how printing ended. A reader
/// that stopped reading (`| head`) got what it wanted, so it ends completed.
pub(crate) fn ending(printed: &Printed, count: u64) -> (String, RunOutcome) {
    match printed {
        Printed::All => (
            format!("Dry run: {count} event(s) rendered, nothing sent."),
            RunOutcome::Completed,
        ),
        Printed::Interrupted(done) => (
            format!("Dry run interrupted: {done} of {count} event(s) rendered, nothing sent."),
            RunOutcome::Interrupted,
        ),
        Printed::ReaderClosed(done) => (
            format!("Dry run: output closed after {done} event(s), nothing sent."),
            RunOutcome::Completed,
        ),
    }
}

/// Renders `template` `count` times into `out`, each line of a render escaped
/// on its own and a blank line between events. Checks `shutdown` before each
/// event.
///
/// # Errors
/// Fails when a render fails, naming the event's zero-based index, and when
/// a write fails with anything but a closed pipe.
pub(crate) fn print_renders(
    template: &Template,
    count: u64,
    cap: usize,
    mut out: impl Write,
    shutdown: &AtomicBool,
) -> anyhow::Result<Printed> {
    for index in 0..count {
        if shutdown.load(Ordering::Relaxed) {
            return Ok(Printed::Interrupted(index));
        }
        let body = template
            .render(cap, Utc::now())
            .with_context(|| format!("event {index}"))?;
        match write_event(&mut out, &body, index > 0) {
            Ok(()) => {}
            Err(error) if error.is::<BrokenPipe>() => return Ok(Printed::ReaderClosed(index)),
            Err(error) => return Err(error).context("Failed to write the dry run to stdout"),
        }
    }
    Ok(Printed::All)
}

fn write_event(out: &mut impl Write, body: &str, separated: bool) -> anyhow::Result<()> {
    let mut text = String::with_capacity(body.len() + 1);
    if separated {
        text.push('\n');
    }
    for line in body.lines() {
        text.push_str(&escape_for_terminal_display(line));
        text.push('\n');
    }
    write_out(out, text.as_bytes())?;
    flush_out(out)
}

#[cfg(test)]
mod tests;

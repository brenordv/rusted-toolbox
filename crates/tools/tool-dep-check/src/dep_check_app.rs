//! One run of dep-check: read the packages, check them, write the report.

use crate::models::{DepCheckArgs, ExitCode, Format};
use anyhow::{Context, Result, bail};
use common_cli::broken_pipe::{BrokenPipe, flush_out, write_out};
use dep_check_core::ecosystem::Ecosystem;
use dep_check_core::engine::{CheckRequest, Checker, Clock};
use dep_check_core::offline::OfflineUse;
use dep_check_core::package::{PackageSpec, parse_specs};
use dep_check_core::render::{source_phrase, text_report};
use dep_check_core::report::Outcome;
use std::io::{Read, Write};

/// The most stdin dep-check reads.
pub const MAX_STDIN_BYTES: usize = 1024 * 1024;
/// The line shown when there are no packages to check.
pub const NO_PACKAGES: &str = "no packages: pass them as arguments, or pipe them in, one per line";

/// What one run reads from and writes to.
pub struct Deps<R: Read, W: Write> {
    /// Whether stdin is a terminal; a terminal is never read.
    pub stdin_terminal: bool,
    pub stdin: R,
    pub stdout: W,
    pub checker: Checker,
    pub clock: Clock,
}

/// How a run that did not fail ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunEnd {
    /// The report was written; the code follows its outcome.
    Reported(ExitCode),
    /// The packages could not be read (exit 2); the one line to show.
    Refused(String),
}

/// Reads the packages, checks them, and writes the report to stdout, which then holds the
/// report and nothing else.
///
/// # Errors
/// A failure of the run itself (exit 4): stdin could not be read, stdout could not be written,
/// or no source answered for any package. A reader that closed the pipe is not a failure.
pub async fn run<R: Read, W: Write>(args: DepCheckArgs, mut deps: Deps<R, W>) -> Result<RunEnd> {
    let packages = match read_packages(&args, &mut deps)? {
        Ok(packages) => packages,
        Err(line) => return Ok(RunEnd::Refused(line)),
    };
    let request = CheckRequest {
        ecosystem: args.ecosystem,
        packages,
        offline: OfflineUse::Off,
    };
    let report = deps.checker.check(request, deps.clock.clone()).await;
    if report.outcome == Outcome::Failed {
        let sources: Vec<String> = report.sources.iter().map(source_phrase).collect();
        bail!("no source answered: {}", sources.join("; "));
    }
    let bytes = match args.format {
        Format::Text => text_report(&report).into_bytes(),
        Format::Json => {
            let mut bytes =
                serde_json::to_vec_pretty(&report).context("Failed to write the report as JSON")?;
            bytes.push(b'\n');
            bytes
        }
    };
    let code = ExitCode::of(report.outcome);
    match write_out(&mut deps.stdout, &bytes).and_then(|()| flush_out(&mut deps.stdout)) {
        Ok(()) => Ok(RunEnd::Reported(code)),
        // The reader took what it wanted and closed the pipe; the verdict stands.
        Err(error) if error.is::<BrokenPipe>() => Ok(RunEnd::Reported(code)),
        Err(error) => Err(error.context("Failed to write the report")),
    }
}

/// The code a run ends with: the report's, 2 for packages that could not be read, 4 for a run
/// that failed.
pub fn exit_code_of(result: &Result<RunEnd>) -> ExitCode {
    match result {
        Ok(RunEnd::Reported(code)) => *code,
        Ok(RunEnd::Refused(_)) => ExitCode::Usage,
        Err(_) => ExitCode::Failed,
    }
}

/// The packages from the arguments, or else from stdin. The inner `Err` is the line to show
/// when they cannot be used.
fn read_packages<R: Read, W: Write>(
    args: &DepCheckArgs,
    deps: &mut Deps<R, W>,
) -> Result<Result<Vec<PackageSpec>, String>> {
    if !args.packages.is_empty() {
        return Ok(parse(
            args.ecosystem,
            args.packages.iter().map(String::as_str),
            "argument",
        ));
    }
    if deps.stdin_terminal {
        return Ok(Err(NO_PACKAGES.to_owned()));
    }
    let mut bytes = Vec::new();
    (&mut deps.stdin)
        .take(MAX_STDIN_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .context("Failed to read stdin")?;
    if bytes.len() > MAX_STDIN_BYTES {
        return Ok(Err("stdin holds more than 1 MiB".to_owned()));
    }
    let Ok(text) = String::from_utf8(bytes) else {
        return Ok(Err("stdin is not UTF-8 text".to_owned()));
    };
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let lines = text
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line));
    Ok(parse(args.ecosystem, lines, "line"))
}

/// `lines` as packages, or the line to show, naming the `unit` ("line" or "argument") that
/// was refused.
fn parse<'a>(
    ecosystem: Ecosystem,
    lines: impl IntoIterator<Item = &'a str>,
    unit: &str,
) -> Result<Vec<PackageSpec>, String> {
    parse_specs(ecosystem, lines).map_err(|issue| match issue.line {
        0 => NO_PACKAGES.to_owned(),
        line => format!("{unit} {line}: {}", issue.message),
    })
}

#[cfg(test)]
mod tests;

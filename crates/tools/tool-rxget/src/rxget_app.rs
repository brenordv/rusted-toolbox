use crate::models::{EngineResult, RunMode, RunOutcome, RunStats, RxgetConfig, Target};
use anyhow::{Context, Result};
use common_cli::broken_pipe::{BrokenPipe, flush_out, write_out};
use common_utils::constants::SIZE_128KB;
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{debug, error, warn};

/// A single line longer than this with no newline is a per-file error: the
/// rest of that file is skipped and the run ends with the failure exit.
const LINE_CAP: usize = 8 * 1024 * 1024;

/// Runs the extraction over every target, one sequential pass per file.
pub fn run<W: Write>(
    config: &RxgetConfig,
    shutdown: &AtomicBool,
    out: &mut W,
) -> Result<EngineResult> {
    let mut stats = RunStats::default();
    let inner = run_inner(config, shutdown, out, &mut stats);
    let result = match inner {
        Ok((outcome, all_ok)) => match flush_out(out) {
            Ok(()) => Ok(EngineResult {
                outcome,
                all_ok,
                stats,
            }),
            Err(error) if error.is::<BrokenPipe>() => Ok(EngineResult {
                outcome: RunOutcome::Completed,
                all_ok: true,
                stats,
            }),
            Err(error) => Err(error),
        },
        Err(error) if error.is::<BrokenPipe>() => Ok(EngineResult {
            outcome: RunOutcome::Completed,
            all_ok: true,
            stats,
        }),
        Err(error) => Err(error),
    };

    if let Ok(engine) = &result {
        debug!(
            targets_processed = engine.stats.targets_processed,
            values_emitted = engine.stats.values_emitted,
            duplicates_suppressed = engine.stats.duplicates_suppressed,
            "run summary"
        );
    }
    result
}

fn run_inner<W: Write>(
    config: &RxgetConfig,
    shutdown: &AtomicBool,
    out: &mut W,
    stats: &mut RunStats,
) -> Result<(RunOutcome, bool)> {
    let has_group = config.pattern.captures_len() > 1;
    let mut seen: HashSet<Vec<u8>> = HashSet::new();
    let mut all_ok = true;

    for target in &config.targets {
        if shutdown.load(Ordering::Relaxed) {
            warn!("interrupted; output is partial");
            return Ok((RunOutcome::Interrupted, all_ok));
        }
        if config.mode == RunMode::UniquePerFile {
            seen.clear();
        }

        let label = target.label();
        let scan = match target {
            Target::Stdin => scan_file(
                &mut std::io::stdin().lock(),
                &label,
                config,
                has_group,
                &mut seen,
                shutdown,
                out,
                stats,
            ),
            Target::File(path) => {
                let file = match File::open(path) {
                    Ok(file) => file,
                    Err(open_error) => {
                        error!(path = %label, "cannot open: {open_error}");
                        all_ok = false;
                        continue;
                    }
                };
                scan_file(
                    &mut BufReader::with_capacity(SIZE_128KB, file),
                    &label,
                    config,
                    has_group,
                    &mut seen,
                    shutdown,
                    out,
                    stats,
                )
            }
        };

        match scan {
            Ok(ScanEnd::Eof) => stats.targets_processed += 1,
            Ok(ScanEnd::OverCap) => {
                stats.targets_processed += 1;
                all_ok = false;
            }
            Ok(ScanEnd::Interrupted) => return Ok((RunOutcome::Interrupted, all_ok)),
            Err(error) if error.is::<BrokenPipe>() => return Err(error),
            Err(error) => {
                error!("rxget: {:#}", error);
                all_ok = false;
            }
        }
    }

    Ok((RunOutcome::Completed, all_ok))
}

/// How one file's scan ended.
enum ScanEnd {
    Eof,
    OverCap,
    Interrupted,
}

#[allow(clippy::too_many_arguments)] // two call sites on one match; splitting into a context struct would only rename the same eight values
fn scan_file<R: BufRead, W: Write>(
    reader: &mut R,
    label: &str,
    config: &RxgetConfig,
    has_group: bool,
    seen: &mut HashSet<Vec<u8>>,
    shutdown: &AtomicBool,
    out: &mut W,
    stats: &mut RunStats,
) -> Result<ScanEnd> {
    let prefix: Option<Vec<u8>> = config.with_filename.then(|| label.as_bytes().to_vec());
    let mut line: Vec<u8> = Vec::new();
    let mut line_number: u64 = 0;

    loop {
        if shutdown.load(Ordering::Relaxed) {
            warn!(path = %label, "interrupted; output is partial");
            return Ok(ScanEnd::Interrupted);
        }
        line_number += 1;

        let read = read_line_capped(reader, &mut line)
            .with_context(|| format!("error reading '{label}'"))?;
        match read {
            ReadLine::Eof => return Ok(ScanEnd::Eof),
            ReadLine::OverCap => {
                error!(
                    path = %label,
                    line = line_number,
                    cap_bytes = LINE_CAP,
                    "line exceeds the cap without a delimiter; the rest of this input is skipped"
                );
                return Ok(ScanEnd::OverCap);
            }
            ReadLine::Line => {
                let content = trim_eol(&line);
                process_line(
                    content,
                    config,
                    has_group,
                    seen,
                    prefix.as_deref(),
                    out,
                    stats,
                )?;
            }
        }
    }
}

/// Emits every non-overlapping match on one line, honoring the run mode.
fn process_line<W: Write>(
    line: &[u8],
    config: &RxgetConfig,
    has_group: bool,
    seen: &mut HashSet<Vec<u8>>,
    prefix: Option<&[u8]>,
    out: &mut W,
    stats: &mut RunStats,
) -> Result<()> {
    for captures in config.pattern.captures_iter(line) {
        // Group 1 when the pattern has capture groups; a match whose group 1
        // did not participate falls back to the whole match. Group 0 always
        // participates, so the else branch cannot skip a match in practice.
        let matched = if has_group {
            captures.get(1).or_else(|| captures.get(0))
        } else {
            captures.get(0)
        };
        let Some(matched) = matched else {
            continue;
        };
        let value = matched.as_bytes();

        let emit = match config.mode {
            RunMode::All => true,
            RunMode::UniquePerFile | RunMode::UniquePerRun => {
                if seen.contains(value) {
                    stats.duplicates_suppressed += 1;
                    false
                } else {
                    seen.insert(value.to_vec());
                    true
                }
            }
        };
        if emit {
            if let Some(prefix) = prefix {
                write_out(out, prefix)?;
                write_out(out, b": ")?;
            }
            write_out(out, value)?;
            write_out(out, b"\n")?;
            stats.values_emitted += 1;
        }
    }
    Ok(())
}

/// What one capped line read produced.
enum ReadLine {
    Line,
    Eof,
    OverCap,
}

/// Reads one `\n`-terminated line into `line` (delimiter included), capping
/// the read at `LINE_CAP` bytes for a line with no delimiter.
fn read_line_capped<R: BufRead>(reader: &mut R, line: &mut Vec<u8>) -> std::io::Result<ReadLine> {
    line.clear();
    loop {
        let (consumed, state) = {
            let available = match reader.fill_buf() {
                Ok(buffer) => buffer,
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            };
            if available.is_empty() {
                (
                    0,
                    Some(if line.is_empty() {
                        ReadLine::Eof
                    } else {
                        ReadLine::Line
                    }),
                )
            } else if let Some(pos) = available.iter().position(|&b| b == b'\n') {
                line.extend_from_slice(&available[..=pos]);
                (pos + 1, Some(ReadLine::Line))
            } else {
                line.extend_from_slice(available);
                let n = available.len();
                let over = line.len() > LINE_CAP;
                (n, over.then_some(ReadLine::OverCap))
            }
        };
        reader.consume(consumed);
        if let Some(state) = state {
            return Ok(state);
        }
    }
}

/// Strips the trailing `\n` and one `\r` before it, so CRLF and LF inputs
/// yield the same values.
fn trim_eol(line: &[u8]) -> &[u8] {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    line.strip_suffix(b"\r").unwrap_or(line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use common_cli::test_writers::ClosedPipe;
    use regex::bytes::Regex;
    use rstest::rstest;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn config(pattern: &str, mode: RunMode, targets: Vec<PathBuf>) -> RxgetConfig {
        RxgetConfig {
            pattern: Regex::new(pattern).unwrap(),
            mode,
            with_filename: false,
            targets: targets.into_iter().map(Target::File).collect(),
        }
    }

    #[test]
    fn stdin_label_prefixes_values_with_standard_input() {
        let cfg = RxgetConfig {
            pattern: Regex::new(r"x=(\d)").unwrap(),
            mode: RunMode::All,
            with_filename: true,
            targets: vec![Target::Stdin],
        };
        let flag = AtomicBool::new(false);
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        let mut stats = RunStats::default();

        let mut reader = BufReader::new(&b"x=4\n"[..]);
        let end = scan_file(
            &mut reader,
            &Target::Stdin.label(),
            &cfg,
            true,
            &mut seen,
            &flag,
            &mut out,
            &mut stats,
        )
        .unwrap();

        assert!(matches!(end, ScanEnd::Eof));
        assert_eq!(out, b"standard input: 4\n");
    }

    fn run_engine(config: &RxgetConfig) -> (Vec<u8>, EngineResult) {
        let flag = AtomicBool::new(false);
        let mut out = Vec::new();
        let result = run(config, &flag, &mut out).unwrap();
        (out, result)
    }

    fn write_file(dir: &tempfile::TempDir, name: &str, content: &[u8]) -> PathBuf {
        let path = dir.path().join(name);
        std::fs::write(&path, content).unwrap();
        path
    }

    #[rstest]
    #[case(r"id=(\d+)", b"id=42 id=7\n".as_slice(), b"42\n7\n".as_slice())]
    #[case(r"\d+", b"a 12 b 34\n".as_slice(), b"12\n34\n".as_slice())]
    #[case(r"(?<v>\d+)-x", b"12-x 9-y\n".as_slice(), b"12\n".as_slice())]
    #[case(r"aa", b"aaaa\n".as_slice(), b"aa\naa\n".as_slice())]
    fn extraction_uses_group_one_or_whole_match(
        #[case] pattern: &str,
        #[case] content: &[u8],
        #[case] expected: &[u8],
    ) {
        let dir = tempdir().unwrap();
        let file = write_file(&dir, "in.txt", content);
        let cfg = config(pattern, RunMode::All, vec![file]);
        let (out, result) = run_engine(&cfg);
        assert!(result.all_ok);
        assert_eq!(out, expected);
    }

    #[test]
    fn unparticipating_group_one_falls_back_to_whole_match() {
        let dir = tempdir().unwrap();
        let file = write_file(&dir, "in.txt", b"key=1 raw\n");
        // Group 1 sits on the branch that does not match "raw".
        let cfg = config(r"key=(\d+)|raw", RunMode::All, vec![file]);
        let (out, _) = run_engine(&cfg);
        assert_eq!(out, b"1\nraw\n");
    }

    #[test]
    fn invalid_utf8_values_pass_through_byte_exact() {
        let dir = tempdir().unwrap();
        let mut content = b"v=".to_vec();
        content.extend_from_slice(&[0xFF, 0xFE]);
        content.extend_from_slice(b" end\n");
        let file = write_file(&dir, "in.bin", &content);

        let cfg = config(r"(?-u)v=([\x80-\xFF]+)", RunMode::All, vec![file]);
        let (out, result) = run_engine(&cfg);
        assert!(result.all_ok);
        assert_eq!(out, [0xFF, 0xFE, b'\n']);
    }

    #[test]
    fn crlf_and_lf_inputs_produce_identical_values() {
        let dir = tempdir().unwrap();
        let lf = write_file(&dir, "lf.txt", b"id=1\nid=2\nid=3");
        let crlf = write_file(&dir, "crlf.txt", b"id=1\r\nid=2\r\nid=3");

        let (lf_out, _) = run_engine(&config(r"id=(\d)", RunMode::All, vec![lf]));
        let (crlf_out, _) = run_engine(&config(r"id=(\d)", RunMode::All, vec![crlf]));
        assert_eq!(lf_out, crlf_out);
        assert_eq!(lf_out, b"1\n2\n3\n");
    }

    #[test]
    fn mode_all_prints_duplicates_across_files() {
        let dir = tempdir().unwrap();
        let a = write_file(&dir, "a.txt", b"x=1 x=1\n");
        let b = write_file(&dir, "b.txt", b"x=1\n");

        let cfg = config(r"x=(\d)", RunMode::All, vec![a, b]);
        let (out, result) = run_engine(&cfg);
        assert_eq!(out, b"1\n1\n1\n");
        assert_eq!(result.stats.values_emitted, 3);
        assert_eq!(result.stats.duplicates_suppressed, 0);
    }

    #[test]
    fn unique_per_file_resets_between_files() {
        let dir = tempdir().unwrap();
        let a = write_file(&dir, "a.txt", b"x=1 x=1 x=2\n");
        let b = write_file(&dir, "b.txt", b"x=1\n");

        let cfg = config(r"x=(\d)", RunMode::UniquePerFile, vec![a, b]);
        let (out, result) = run_engine(&cfg);
        assert_eq!(out, b"1\n2\n1\n");
        assert_eq!(result.stats.duplicates_suppressed, 1);
    }

    #[test]
    fn unique_per_run_suppresses_across_files_in_first_seen_order() {
        let dir = tempdir().unwrap();
        let a = write_file(&dir, "a.txt", b"x=2 x=1\n");
        let b = write_file(&dir, "b.txt", b"x=1 x=3\n");

        let cfg = config(r"x=(\d)", RunMode::UniquePerRun, vec![a, b]);
        let (out, result) = run_engine(&cfg);
        assert_eq!(out, b"2\n1\n3\n");
        assert_eq!(result.stats.duplicates_suppressed, 1);
        assert_eq!(result.stats.targets_processed, 2);
        assert_eq!(result.stats.values_emitted, 3);
    }

    #[test]
    fn with_filename_prefixes_and_dedup_ignores_the_prefix() {
        let dir = tempdir().unwrap();
        let a = write_file(&dir, "a.txt", b"x=1\n");
        let b = write_file(&dir, "b.txt", b"x=1\n");

        let mut cfg = config(r"x=(\d)", RunMode::UniquePerRun, vec![a.clone(), b]);
        cfg.with_filename = true;
        let (out, result) = run_engine(&cfg);

        let expected = format!("{}: 1\n", a.display());
        assert_eq!(out, expected.as_bytes());
        assert_eq!(result.stats.duplicates_suppressed, 1);
    }

    #[test]
    fn empty_file_and_no_match_file_emit_nothing() {
        let dir = tempdir().unwrap();
        let empty = write_file(&dir, "empty.txt", b"");
        let nomatch = write_file(&dir, "nomatch.txt", b"nothing here\n");

        let cfg = config(r"x=(\d)", RunMode::All, vec![empty, nomatch]);
        let (out, result) = run_engine(&cfg);
        assert!(out.is_empty());
        assert!(result.all_ok);
        assert_eq!(result.stats.targets_processed, 2);
    }

    #[test]
    fn missing_file_among_good_ones_fails_but_continues() {
        let dir = tempdir().unwrap();
        let good = write_file(&dir, "good.txt", b"x=5\n");
        let missing = dir.path().join("missing.txt");

        let cfg = config(r"x=(\d)", RunMode::All, vec![missing, good]);
        let (out, result) = run_engine(&cfg);
        assert_eq!(out, b"5\n");
        assert!(!result.all_ok);
        assert_eq!(result.outcome, RunOutcome::Completed);
    }

    #[test]
    fn shutdown_flag_yields_interrupted() {
        let dir = tempdir().unwrap();
        let file = write_file(&dir, "in.txt", b"x=1\n");

        let cfg = config(r"x=(\d)", RunMode::All, vec![file]);
        let flag = AtomicBool::new(true);
        let mut out = Vec::new();
        let result = run(&cfg, &flag, &mut out).unwrap();
        assert_eq!(result.outcome, RunOutcome::Interrupted);
    }

    #[test]
    fn broken_pipe_yields_quiet_success() {
        let dir = tempdir().unwrap();
        let file = write_file(&dir, "in.txt", b"x=1\n");

        let cfg = config(r"x=(\d)", RunMode::All, vec![file]);
        let flag = AtomicBool::new(false);
        let result = run(&cfg, &flag, &mut ClosedPipe).unwrap();
        assert!(result.all_ok);
        assert_eq!(result.outcome, RunOutcome::Completed);
    }

    #[test]
    fn read_line_capped_reports_over_cap_lines() {
        // The cap fires once the accumulated delimiterless bytes exceed it,
        // so the line must overshoot by more than one reader chunk to trip
        // deterministically regardless of chunk boundaries.
        let dir = tempdir().unwrap();
        let mut content = vec![b'a'; LINE_CAP + 200_000];
        content.push(b'\n');
        content.extend_from_slice(b"x=9\n");
        let big = write_file(&dir, "big.txt", &content);
        let good = write_file(&dir, "good.txt", b"x=1\n");

        let cfg = config(r"x=(\d)", RunMode::All, vec![big, good]);
        let (out, result) = run_engine(&cfg);
        // The over-cap file is abandoned (its x=9 is never scanned), the run
        // continues with the next file and ends as a failure.
        assert_eq!(out, b"1\n");
        assert!(!result.all_ok);
        assert_eq!(result.outcome, RunOutcome::Completed);
    }

    #[test]
    fn final_line_without_newline_is_processed() {
        let dir = tempdir().unwrap();
        let file = write_file(&dir, "in.txt", b"x=1\nx=2");
        let cfg = config(r"x=(\d)", RunMode::All, vec![file]);
        let (out, _) = run_engine(&cfg);
        assert_eq!(out, b"1\n2\n");
    }
}

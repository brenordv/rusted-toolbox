use crate::models::HeadConfig;
use anyhow::{Context, Result};
use common_utils::constants::SIZE_128KB;
use shared_head_tail::io_shared::{
    backward_scan_start, check_shutdown, copy_limited, finish_run, process_input_source,
    read_chunk, run_inputs, split_items, write_out, HeaderState, Input,
};
use shared_head_tail::models::{CountUnit, RunResult};
use std::collections::VecDeque;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::atomic::AtomicBool;

/// Runs the head engine over every input, honoring the broken-pipe and
/// interrupt policies. The result maps to the exit-code contract in `main`.
///
/// # Errors
/// Returns only non-marker writer failures; per-input errors are logged and
/// reflected in the result's `all_ok`.
pub fn run<W: Write>(config: &HeadConfig, shutdown: &AtomicBool, out: &mut W) -> Result<RunResult> {
    let mut buf = vec![0u8; SIZE_128KB];
    let inner = run_inputs(
        &config.files,
        config.headers,
        "head",
        shutdown,
        out,
        &mut buf,
        |input, show_headers, header_state, out, buf| {
            head_one_input(
                input,
                config,
                show_headers,
                header_state,
                shutdown,
                out,
                buf,
            )
        },
    );
    finish_run(inner, out)
}

fn head_one_input<W: Write>(
    input: &Input<'_>,
    config: &HeadConfig,
    show_headers: bool,
    header_state: &mut HeaderState,
    shutdown: &AtomicBool,
    out: &mut W,
    buf: &mut [u8],
) -> Result<()> {
    process_input_source(
        input,
        show_headers,
        header_state,
        out,
        buf,
        |file, len, out, buf| head_seekable(file, len, config, shutdown, out, buf),
        |mut reader, out, buf| head_stream(&mut reader, config, shutdown, out, buf),
    )
}

/// Head over a seekable regular file: the negative-count forms run in
/// constant memory by finding the cutoff with the shared backward scan.
fn head_seekable<W: Write>(
    file: &mut File,
    len: u64,
    config: &HeadConfig,
    shutdown: &AtomicBool,
    out: &mut W,
    buf: &mut [u8],
) -> Result<()> {
    if !config.elide {
        return head_stream(file, config, shutdown, out, buf);
    }

    let cutoff = match config.unit {
        CountUnit::Lines => {
            let cutoff =
                backward_scan_start(file, len, config.count, config.delimiter(), buf.len())?;
            file.seek(SeekFrom::Start(0))
                .context("failed to seek back to the start")?;
            cutoff
        }
        CountUnit::Bytes => len.saturating_sub(config.count),
    };
    copy_limited(file, out, cutoff, shutdown, buf)?;
    Ok(())
}

/// Head over a non-seekable stream (and the plain forms of seekable files).
fn head_stream<R: Read, W: Write>(
    reader: &mut R,
    config: &HeadConfig,
    shutdown: &AtomicBool,
    out: &mut W,
    buf: &mut [u8],
) -> Result<()> {
    match (config.unit, config.elide) {
        (CountUnit::Lines, false) => {
            first_items(reader, config.count, config.delimiter(), shutdown, out, buf)
        }
        (CountUnit::Bytes, false) => {
            copy_limited(reader, out, config.count, shutdown, buf).map(|_| ())
        }
        (CountUnit::Lines, true) => {
            elide_last_items(reader, config.count, config.delimiter(), shutdown, out, buf)
        }
        (CountUnit::Bytes, true) => elide_last_bytes(reader, config.count, shutdown, out, buf),
    }
}

/// Streams the first `count` delimiter-terminated items, stopping early.
fn first_items<R: Read, W: Write>(
    reader: &mut R,
    count: u64,
    delimiter: u8,
    shutdown: &AtomicBool,
    out: &mut W,
    buf: &mut [u8],
) -> Result<()> {
    if count == 0 {
        return Ok(());
    }
    let mut remaining = count;
    loop {
        check_shutdown(shutdown)?;
        let n = read_chunk(reader, buf).context("failed to read input")?;
        if n == 0 {
            return Ok(());
        }
        let chunk = &buf[..n];
        for (i, &byte) in chunk.iter().enumerate() {
            if byte == delimiter {
                remaining -= 1;
                if remaining == 0 {
                    write_out(out, &chunk[..=i])?;
                    return Ok(());
                }
            }
        }
        write_out(out, chunk)?;
    }
}

/// The head.c pipe algorithm for `-n -NUM`: hold up to NUM complete items in
/// a deque, writing overflow through; at EOF the deque holds the elided tail.
fn elide_last_items<R: Read, W: Write>(
    reader: &mut R,
    count: u64,
    delimiter: u8,
    shutdown: &AtomicBool,
    out: &mut W,
    buf: &mut [u8],
) -> Result<()> {
    let cap = usize::try_from(count).unwrap_or(usize::MAX);
    let mut items: VecDeque<Vec<u8>> = VecDeque::new();
    split_items(reader, delimiter, shutdown, buf, |item| {
        items.push_back(item);
        write_deque_overflow(&mut items, cap, out)
    })
}

fn write_deque_overflow<W: Write>(
    items: &mut VecDeque<Vec<u8>>,
    cap: usize,
    out: &mut W,
) -> Result<()> {
    while items.len() > cap {
        if let Some(item) = items.pop_front() {
            write_out(out, &item)?;
        }
    }
    Ok(())
}

/// Rolling delay line for `-c -NUM` on a stream: keeps the last NUM bytes
/// buffered and writes everything older through.
fn elide_last_bytes<R: Read, W: Write>(
    reader: &mut R,
    count: u64,
    shutdown: &AtomicBool,
    out: &mut W,
    buf: &mut [u8],
) -> Result<()> {
    let cap = usize::try_from(count).unwrap_or(usize::MAX);
    let mut delay: VecDeque<u8> = VecDeque::new();

    loop {
        check_shutdown(shutdown)?;
        let n = read_chunk(reader, buf).context("failed to read input")?;
        if n == 0 {
            return Ok(());
        }
        delay.extend(buf[..n].iter().copied());
        if delay.len() > cap {
            let excess = delay.len() - cap;
            let drained: Vec<u8> = delay.drain(..excess).collect();
            write_out(out, &drained)?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common_cli::test_writers::FailAfter;
    use rstest::rstest;
    use shared_head_tail::models::{HeaderPolicy, RunOutcome};
    use std::sync::atomic::AtomicBool;
    use tempfile::tempdir;

    fn config(unit: CountUnit, count: u64, elide: bool) -> HeadConfig {
        HeadConfig {
            unit,
            count,
            elide,
            headers: HeaderPolicy::Auto,
            zero_terminated: false,
            files: Vec::new(),
        }
    }

    fn run_stream(cfg: &HeadConfig, input: &[u8], chunk: usize) -> Vec<u8> {
        let flag = AtomicBool::new(false);
        let mut buf = vec![0u8; chunk];
        let mut out = Vec::new();
        let mut reader = input;
        head_stream(&mut reader, cfg, &flag, &mut out, &mut buf).unwrap();
        out
    }

    fn run_over_files(cfg: &HeadConfig) -> (Vec<u8>, RunResult) {
        let flag = AtomicBool::new(false);
        let mut out = Vec::new();
        let result = run(cfg, &flag, &mut out).unwrap();
        (out, result)
    }

    #[rstest]
    #[case(10, b"a\nb\nc\n".as_slice(), b"a\nb\nc\n".as_slice())]
    #[case(3, b"1\n2\n3\n4\n5\n".as_slice(), b"1\n2\n3\n".as_slice())]
    #[case(0, b"a\nb\n".as_slice(), b"".as_slice())]
    #[case(2, b"only\n".as_slice(), b"only\n".as_slice())]
    #[case(2, b"a\nno-newline".as_slice(), b"a\nno-newline".as_slice())]
    fn first_lines_cases(#[case] count: u64, #[case] input: &[u8], #[case] expected: &[u8]) {
        let cfg = config(CountUnit::Lines, count, false);
        assert_eq!(run_stream(&cfg, input, 4), expected);
        assert_eq!(run_stream(&cfg, input, 64 * 1024), expected);
    }

    #[test]
    fn crlf_bytes_pass_through_untouched() {
        let cfg = config(CountUnit::Lines, 1, false);
        assert_eq!(run_stream(&cfg, b"a\r\nb\r\n", 64), b"a\r\n");
    }

    #[test]
    fn zero_delimiter_counts_nul_items() {
        let mut cfg = config(CountUnit::Lines, 2, false);
        cfg.zero_terminated = true;
        assert_eq!(run_stream(&cfg, b"a\0b\0c\0", 64), b"a\0b\0");
    }

    #[rstest]
    #[case(5, b"hello world".as_slice(), b"hello".as_slice())]
    #[case(0, b"abc".as_slice(), b"".as_slice())]
    #[case(10, b"ab".as_slice(), b"ab".as_slice())]
    fn first_bytes_cases(#[case] count: u64, #[case] input: &[u8], #[case] expected: &[u8]) {
        let cfg = config(CountUnit::Bytes, count, false);
        assert_eq!(run_stream(&cfg, input, 3), expected);
    }

    #[rstest]
    #[case(2, b"1\n2\n3\n4\n".as_slice(), b"1\n2\n".as_slice())]
    #[case(2, b"1\n2\n".as_slice(), b"".as_slice())]
    #[case(2, b"only\n".as_slice(), b"".as_slice())]
    #[case(1, b"a\nfragment".as_slice(), b"a\n".as_slice())]
    #[case(0, b"a\nb\n".as_slice(), b"a\nb\n".as_slice())]
    fn elide_lines_stream_cases(#[case] count: u64, #[case] input: &[u8], #[case] expected: &[u8]) {
        let cfg = config(CountUnit::Lines, count, true);
        assert_eq!(run_stream(&cfg, input, 3), expected);
        assert_eq!(run_stream(&cfg, input, 64 * 1024), expected);
    }

    #[rstest]
    #[case(4, b"abcdefgh".as_slice(), b"abcd".as_slice())]
    #[case(4, b"abc".as_slice(), b"".as_slice())]
    #[case(0, b"abc".as_slice(), b"abc".as_slice())]
    fn elide_bytes_stream_cases(#[case] count: u64, #[case] input: &[u8], #[case] expected: &[u8]) {
        let cfg = config(CountUnit::Bytes, count, true);
        assert_eq!(run_stream(&cfg, input, 3), expected);
    }

    #[test]
    fn elide_paths_agree_between_seekable_and_stream() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("in.txt");
        let content = b"alpha\nbeta\ngamma\ndelta\n";
        std::fs::write(&path, content).unwrap();
        let flag = AtomicBool::new(false);

        for (unit, count) in [(CountUnit::Lines, 2), (CountUnit::Bytes, 4)] {
            let mut cfg = config(unit, count, true);
            cfg.files = vec![path.to_string_lossy().to_string()];

            let mut seekable_out = Vec::new();
            let result = run(&cfg, &flag, &mut seekable_out).unwrap();
            assert!(result.all_ok);

            let stream_out = run_stream(&cfg, content, 5);
            assert_eq!(seekable_out, stream_out);
        }
    }

    #[test]
    fn headers_appear_for_two_files_with_exact_bytes() {
        let dir = tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        std::fs::write(&a, b"1\n").unwrap();
        std::fs::write(&b, b"2\n").unwrap();

        let mut cfg = config(CountUnit::Lines, 10, false);
        cfg.files = vec![
            a.to_string_lossy().to_string(),
            b.to_string_lossy().to_string(),
        ];
        let (out, result) = run_over_files(&cfg);
        assert!(result.all_ok);

        let expected = format!(
            "==> {} <==\n1\n\n==> {} <==\n2\n",
            a.to_string_lossy(),
            b.to_string_lossy()
        );
        assert_eq!(out, expected.as_bytes());
    }

    #[test]
    fn single_file_has_no_header_unless_forced() {
        let dir = tempdir().unwrap();
        let a = dir.path().join("a");
        std::fs::write(&a, b"1\n").unwrap();

        let mut cfg = config(CountUnit::Lines, 10, false);
        cfg.files = vec![a.to_string_lossy().to_string()];
        let (out, _) = run_over_files(&cfg);
        assert_eq!(out, b"1\n");

        cfg.headers = HeaderPolicy::Always;
        let (out, _) = run_over_files(&cfg);
        let expected = format!("==> {} <==\n1\n", a.to_string_lossy());
        assert_eq!(out, expected.as_bytes());
    }

    #[test]
    fn quiet_suppresses_headers_for_two_files() {
        let dir = tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        std::fs::write(&a, b"1\n").unwrap();
        std::fs::write(&b, b"2\n").unwrap();

        let mut cfg = config(CountUnit::Lines, 10, false);
        cfg.headers = HeaderPolicy::Never;
        cfg.files = vec![
            a.to_string_lossy().to_string(),
            b.to_string_lossy().to_string(),
        ];
        let (out, _) = run_over_files(&cfg);
        assert_eq!(out, b"1\n2\n");
    }

    #[test]
    fn missing_file_between_two_real_ones_reports_failure_but_continues() {
        let dir = tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        std::fs::write(&a, b"first\n").unwrap();
        std::fs::write(&b, b"second\n").unwrap();

        let mut cfg = config(CountUnit::Lines, 10, false);
        cfg.headers = HeaderPolicy::Never;
        cfg.files = vec![
            a.to_string_lossy().to_string(),
            dir.path().join("missing").to_string_lossy().to_string(),
            b.to_string_lossy().to_string(),
        ];
        let (out, result) = run_over_files(&cfg);
        assert!(!result.all_ok);
        assert_eq!(result.outcome, RunOutcome::Completed);
        assert_eq!(out, b"first\nsecond\n");
    }

    #[test]
    fn broken_pipe_ends_the_run_with_quiet_success() {
        let dir = tempdir().unwrap();
        let a = dir.path().join("a");
        std::fs::write(&a, b"a longer line than four bytes\n").unwrap();

        let mut cfg = config(CountUnit::Lines, 10, false);
        cfg.files = vec![a.to_string_lossy().to_string()];
        let flag = AtomicBool::new(false);
        let mut out = FailAfter::broken_pipe(4);
        let result = run(&cfg, &flag, &mut out).unwrap();
        assert!(result.all_ok);
        assert_eq!(result.outcome, RunOutcome::Completed);
    }

    #[test]
    fn shutdown_before_processing_yields_interrupted() {
        let dir = tempdir().unwrap();
        let a = dir.path().join("a");
        std::fs::write(&a, b"data\n").unwrap();

        let mut cfg = config(CountUnit::Lines, 10, false);
        cfg.files = vec![a.to_string_lossy().to_string()];
        let flag = AtomicBool::new(true);
        let mut out = Vec::new();
        let result = run(&cfg, &flag, &mut out).unwrap();
        assert_eq!(result.outcome, RunOutcome::Interrupted);
    }
}

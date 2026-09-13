use crate::models::{FollowMode, TailConfig};
use anyhow::{Context, Result};
use common_utils::constants::SIZE_128KB;
use shared_head_tail::io_shared::{
    backward_scan_start, check_shutdown, copy_limited, copy_to_end, finish_run, flush_out,
    probe_seekable, process_input_source, read_chunk, resolve_inputs, run_inputs, split_items,
    write_out, BrokenPipe, HeaderState, Input, Interrupted,
};
use shared_head_tail::models::{CountUnit, RunResult};
use std::collections::VecDeque;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Duration;
use tracing::{debug, error, warn};

/// Runs the tail engine over every input, honoring the broken-pipe and
/// interrupt policies. The result maps to the exit-code contract in `main`.
///
/// # Errors
/// Returns only non-marker writer failures; per-input errors are logged and
/// reflected in the result's `all_ok`.
pub fn run<W: Write>(config: &TailConfig, shutdown: &AtomicBool, out: &mut W) -> Result<RunResult> {
    let mut buf = vec![0u8; SIZE_128KB];
    if let Some(value) = config.max_unchanged_stats {
        debug!(
            value,
            "--max-unchanged-stats is accepted but has no effect in this port"
        );
    }
    if config.follow.is_some() {
        finish_run(run_follow_inner(config, shutdown, out, &mut buf), out)
    } else {
        finish_run(run_static_inner(config, shutdown, out, &mut buf), out)
    }
}

fn run_static_inner<W: Write>(
    config: &TailConfig,
    shutdown: &AtomicBool,
    out: &mut W,
    buf: &mut [u8],
) -> Result<bool> {
    run_inputs(
        &config.files,
        config.headers,
        "tail",
        shutdown,
        out,
        buf,
        |input, show_headers, header_state, out, buf| {
            tail_one_input(
                input,
                config,
                show_headers,
                header_state,
                shutdown,
                out,
                buf,
            )
        },
    )
}

fn tail_one_input<W: Write>(
    input: &Input<'_>,
    config: &TailConfig,
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
        |file, len, out, buf| tail_seekable(file, len, config, shutdown, out, buf).map(|_| ()),
        |mut reader, out, buf| tail_stream(&mut reader, config, shutdown, out, buf),
    )
}

/// Tail over a seekable regular file, in constant memory: the from-the-end
/// forms seek to the computed start offset and stream forward.
fn tail_seekable<W: Write>(
    file: &mut File,
    len: u64,
    config: &TailConfig,
    shutdown: &AtomicBool,
    out: &mut W,
    buf: &mut [u8],
) -> Result<u64> {
    if config.from_start {
        match config.unit {
            // `+NUM` bytes on a seekable file needs no skip-reading: seek
            // straight to the start offset (clamped to EOF) and stream.
            CountUnit::Bytes => {
                let start = config.count.saturating_sub(1).min(len);
                file.seek(SeekFrom::Start(start))
                    .context("failed to seek to the output start")?;
                copy_to_end(file, out, shutdown, buf)?;
            }
            CountUnit::Lines => {
                from_start_stream(file, config, shutdown, out, buf)?;
            }
        }
    } else {
        let start = match config.unit {
            CountUnit::Lines => {
                backward_scan_start(file, len, config.count, config.delimiter(), buf.len())?
            }
            CountUnit::Bytes => len.saturating_sub(config.count),
        };
        file.seek(SeekFrom::Start(start))
            .context("failed to seek to the output start")?;
        copy_to_end(file, out, shutdown, buf)?;
    }
    file.stream_position()
        .context("failed to record the file position")
}

/// Tail over a non-seekable stream: the from-the-end forms buffer at most
/// NUM items (the pipe_lines/pipe_bytes approach).
fn tail_stream<R: Read, W: Write>(
    reader: &mut R,
    config: &TailConfig,
    shutdown: &AtomicBool,
    out: &mut W,
    buf: &mut [u8],
) -> Result<()> {
    if config.from_start {
        return from_start_stream(reader, config, shutdown, out, buf);
    }
    match config.unit {
        CountUnit::Lines => {
            last_items_stream(reader, config.count, config.delimiter(), shutdown, out, buf)
        }
        CountUnit::Bytes => last_bytes_stream(reader, config.count, shutdown, out, buf),
    }
}

/// `+NUM`: skips NUM-1 items or bytes, then copies the rest through. Works
/// identically for seekable and non-seekable input; `+0` behaves as `+1`.
fn from_start_stream<R: Read, W: Write>(
    reader: &mut R,
    config: &TailConfig,
    shutdown: &AtomicBool,
    out: &mut W,
    buf: &mut [u8],
) -> Result<()> {
    let mut to_skip = config.count.saturating_sub(1);
    let delimiter = config.delimiter();

    loop {
        check_shutdown(shutdown)?;
        if to_skip == 0 {
            copy_to_end(reader, out, shutdown, buf)?;
            return Ok(());
        }
        let n = read_chunk(reader, buf).context("failed to read input")?;
        if n == 0 {
            return Ok(());
        }
        let chunk = &buf[..n];
        match config.unit {
            CountUnit::Bytes => {
                let skipped = to_skip.min(n as u64);
                to_skip -= skipped;
                let start = usize::try_from(skipped).unwrap_or(n);
                if start < n {
                    write_out(out, &chunk[start..])?;
                }
            }
            CountUnit::Lines => {
                let mut start = 0usize;
                for (i, &byte) in chunk.iter().enumerate() {
                    if byte == delimiter {
                        to_skip -= 1;
                        if to_skip == 0 {
                            start = i + 1;
                            break;
                        }
                    }
                }
                if to_skip == 0 && start < n {
                    write_out(out, &chunk[start..])?;
                }
            }
        }
    }
}

/// The pipe_lines approach: keep at most the last NUM items in a deque, then
/// write what remains at EOF. A trailing fragment counts as an item.
fn last_items_stream<R: Read, W: Write>(
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
        push_capped(&mut items, item, cap);
        Ok(())
    })?;

    for item in &items {
        write_out(out, item)?;
    }
    Ok(())
}

fn push_capped(items: &mut VecDeque<Vec<u8>>, item: Vec<u8>, cap: usize) {
    if cap == 0 {
        return;
    }
    if items.len() == cap {
        items.pop_front();
    }
    items.push_back(item);
}

/// Byte delay line keeping only the last NUM bytes, written at EOF.
fn last_bytes_stream<R: Read, W: Write>(
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
            break;
        }
        if cap == 0 {
            continue;
        }
        delay.extend(buf[..n].iter().copied());
        if delay.len() > cap {
            delay.drain(..delay.len() - cap);
        }
    }

    let (front, back) = delay.as_slices();
    write_out(out, front)?;
    write_out(out, back)?;
    Ok(())
}

/// Global follow-mode settings, fixed for the whole run.
#[derive(Debug, Clone, Copy)]
struct FollowSettings {
    mode: FollowMode,
    retry: bool,
    headers: bool,
}

/// One followed file and its polling state.
struct FollowSource {
    id: usize,
    path: PathBuf,
    /// Descriptor mode holds the handle for the source's lifetime; name mode
    /// reopens per cycle and keeps this empty between cycles.
    file: Option<File>,
    position: u64,
    /// Drives once-per-transition logging for open and read failures.
    readable: bool,
    #[cfg(unix)]
    identity: Option<(u64, u64)>,
}

impl FollowSource {
    fn opened(id: usize, path: PathBuf, file: File, position: u64) -> Self {
        #[cfg(unix)]
        let identity = file_identity(&file);
        FollowSource {
            id,
            path,
            file: Some(file),
            position,
            readable: true,
            #[cfg(unix)]
            identity,
        }
    }

    fn pending(id: usize, path: PathBuf) -> Self {
        FollowSource {
            id,
            path,
            file: None,
            position: 0,
            readable: false,
            #[cfg(unix)]
            identity: None,
        }
    }
}

#[cfg(unix)]
fn file_identity(file: &File) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    file.metadata()
        .ok()
        .map(|metadata| (metadata.dev(), metadata.ino()))
}

/// What one polling cycle did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CycleReport {
    wrote_any: bool,
    dropped_any: bool,
}

/// Runs one polling pass over every source: reopen (name mode), detect
/// truncation and rotation, stream new bytes, and flush when anything was
/// written. Per-source I/O problems are logged on their transition and never
/// abort the cycle; only output-side and shutdown errors propagate.
fn follow_cycle<W: Write>(
    sources: &mut Vec<FollowSource>,
    settings: &FollowSettings,
    last_printed: &mut Option<usize>,
    header_state: &mut HeaderState,
    shutdown: &AtomicBool,
    out: &mut W,
    buf: &mut [u8],
) -> Result<CycleReport> {
    let mut wrote_any = false;
    let mut dropped: Vec<usize> = Vec::new();

    for source in sources.iter_mut() {
        check_shutdown(shutdown)?;
        let poll = poll_source(
            source,
            settings,
            last_printed,
            header_state,
            shutdown,
            out,
            buf,
        )?;
        wrote_any |= poll.wrote;
        if poll.drop_source {
            dropped.push(source.id);
        }
    }

    if !dropped.is_empty() {
        sources.retain(|source| !dropped.contains(&source.id));
    }
    if wrote_any {
        flush_out(out)?;
    }
    Ok(CycleReport {
        wrote_any,
        dropped_any: !dropped.is_empty(),
    })
}

struct PollOutcome {
    wrote: bool,
    drop_source: bool,
}

impl PollOutcome {
    fn idle() -> Self {
        PollOutcome {
            wrote: false,
            drop_source: false,
        }
    }
}

fn poll_source<W: Write>(
    source: &mut FollowSource,
    settings: &FollowSettings,
    last_printed: &mut Option<usize>,
    header_state: &mut HeaderState,
    shutdown: &AtomicBool,
    out: &mut W,
    buf: &mut [u8],
) -> Result<PollOutcome> {
    // Name mode reopens per cycle, so no handle is held between cycles (the
    // contract on `FollowSource::file`); this also releases the handle kept
    // from the initial open.
    if settings.mode == FollowMode::Name {
        source.file = None;
    }

    let mut cycle_handle: Option<File> = None;

    match settings.mode {
        FollowMode::Name => match File::open(&source.path) {
            Err(error) => {
                if source.readable {
                    warn!(path = %source.path.display(), "has become inaccessible: {error}");
                    source.readable = false;
                }
                return Ok(PollOutcome {
                    wrote: false,
                    drop_source: !settings.retry,
                });
            }
            Ok(file) => {
                if source.readable {
                    detect_rotation(source, &file);
                } else {
                    warn!(path = %source.path.display(), "has appeared, following new file");
                    source.position = 0;
                    source.readable = true;
                    #[cfg(unix)]
                    {
                        source.identity = file_identity(&file);
                    }
                }
                cycle_handle = Some(file);
            }
        },
        FollowMode::Descriptor => {
            if source.file.is_none() {
                match File::open(&source.path) {
                    Ok(file) => {
                        warn!(path = %source.path.display(), "has appeared, following new file");
                        source.position = 0;
                        source.readable = true;
                        source.file = Some(file);
                        #[cfg(unix)]
                        if let Some(held) = source.file.as_ref() {
                            source.identity = file_identity(held);
                        }
                    }
                    Err(_) => return Ok(PollOutcome::idle()),
                }
            }
        }
    }

    let file = match settings.mode {
        FollowMode::Name => cycle_handle.as_mut(),
        FollowMode::Descriptor => source.file.as_mut(),
    };
    let Some(file) = file else {
        return Ok(PollOutcome::idle());
    };

    let len = match file.metadata() {
        Ok(metadata) => metadata.len(),
        Err(error) => {
            if source.readable {
                warn!(
                    path = %source.path.display(),
                    "cannot stat: {error}{}",
                    abandon_suffix(settings.retry)
                );
                source.readable = false;
            }
            return Ok(failed_source_outcome(settings.retry));
        }
    };

    if len < source.position {
        warn!(path = %source.path.display(), "file truncated");
        source.position = 0;
    }
    if len == source.position {
        return Ok(PollOutcome::idle());
    }

    if settings.headers && *last_printed != Some(source.id) {
        header_state.write(out, &source.path.display().to_string())?;
        *last_printed = Some(source.id);
    }

    let streamed = file
        .seek(SeekFrom::Start(source.position))
        .map_err(anyhow::Error::from)
        .and_then(|_| {
            copy_limited(
                file,
                out,
                len.saturating_sub(source.position),
                shutdown,
                buf,
            )
        });
    match streamed {
        Ok(copied) => {
            source.position = source.position.saturating_add(copied);
            source.readable = true;
            Ok(PollOutcome {
                wrote: copied > 0,
                drop_source: false,
            })
        }
        Err(error) if error.is::<BrokenPipe>() || error.is::<Interrupted>() => Err(error),
        Err(error) => {
            if source.readable {
                warn!(
                    path = %source.path.display(),
                    "read failed: {error:#}{}",
                    abandon_suffix(settings.retry)
                );
                source.readable = false;
            }
            Ok(failed_source_outcome(settings.retry))
        }
    }
}

/// Suffix for per-source failure warns, naming the consequence when the source
/// is about to be abandoned because `--retry` is off.
fn abandon_suffix(retry: bool) -> &'static str {
    if retry {
        ""
    } else {
        "; giving up on this file (use --retry to keep trying)"
    }
}

/// Outcome for a source whose stat or read failed: the source is abandoned
/// unless `--retry` asked to keep trying, mirroring the name-mode open-failure
/// policy.
fn failed_source_outcome(retry: bool) -> PollOutcome {
    PollOutcome {
        wrote: false,
        drop_source: !retry,
    }
}

/// Name mode, Unix: a changed device/inode pair on reopen means the path now
/// names a different file; restart from offset 0. On other platforms rotation
/// falls back to the truncation length heuristic.
#[cfg(unix)]
fn detect_rotation(source: &mut FollowSource, file: &File) {
    let identity = file_identity(file);
    if identity != source.identity {
        warn!(path = %source.path.display(), "has appeared, following new file");
        source.position = 0;
        source.identity = identity;
    }
}

#[cfg(not(unix))]
fn detect_rotation(_source: &mut FollowSource, _file: &File) {}

fn run_follow_inner<W: Write>(
    config: &TailConfig,
    shutdown: &AtomicBool,
    out: &mut W,
    buf: &mut [u8],
) -> Result<bool> {
    let inputs = resolve_inputs(&config.files);
    let show_headers = config.headers.show(inputs.len());
    let settings = FollowSettings {
        mode: config.follow.unwrap_or(FollowMode::Descriptor),
        retry: config.retry,
        headers: show_headers,
    };
    let mut header_state = HeaderState::new();
    let mut last_printed: Option<usize> = None;
    let mut sources: Vec<FollowSource> = Vec::new();
    let mut all_ok = true;
    let mut dropped_or_failed = false;

    for (id, input) in inputs.iter().enumerate() {
        check_shutdown(shutdown)?;
        match input {
            Input::Stdin => {
                if show_headers {
                    header_state.write(out, input.display_name())?;
                    last_printed = Some(id);
                }
                let stdin = io::stdin();
                let mut lock = stdin.lock();
                if let Err(error) = tail_stream(&mut lock, config, shutdown, out, buf)
                    .context("error reading standard input")
                {
                    if error.is::<BrokenPipe>() || error.is::<Interrupted>() {
                        return Err(error);
                    }
                    error!("tail: {:#}", error);
                    all_ok = false;
                }
            }
            Input::File(name) => match File::open(name) {
                Ok(mut file) => {
                    if show_headers {
                        header_state.write(out, name)?;
                        last_printed = Some(id);
                    }
                    match initial_file_output(&mut file, config, shutdown, out, buf) {
                        Ok(Some(position)) => {
                            sources.push(FollowSource::opened(
                                id,
                                PathBuf::from(name),
                                file,
                                position,
                            ));
                        }
                        Ok(None) => {
                            warn!(path = %name, "not a regular file; read once, not followed");
                        }
                        Err(error) if error.is::<BrokenPipe>() || error.is::<Interrupted>() => {
                            return Err(error);
                        }
                        Err(error) => {
                            error!("tail: error reading '{name}': {error:#}");
                            all_ok = false;
                            dropped_or_failed = true;
                        }
                    }
                }
                Err(error) => {
                    if settings.retry {
                        warn!(path = %name, "cannot open: {error}; will keep trying");
                        sources.push(FollowSource::pending(id, PathBuf::from(name)));
                    } else {
                        error!("tail: cannot open '{name}' for reading: {error}");
                        all_ok = false;
                        dropped_or_failed = true;
                    }
                }
            },
        }
    }

    flush_out(out)?;

    loop {
        check_shutdown(shutdown)?;
        if sources.is_empty() {
            if dropped_or_failed {
                error!("tail: no files remaining");
                return Ok(false);
            }
            warn!(
                "nothing left to follow; standard input and non-regular files are read only once"
            );
            return Ok(all_ok);
        }

        let report = follow_cycle(
            &mut sources,
            &settings,
            &mut last_printed,
            &mut header_state,
            shutdown,
            out,
            buf,
        )?;
        if report.dropped_any {
            all_ok = false;
            dropped_or_failed = true;
        }
        if sources.is_empty() {
            continue;
        }
        sleep_with_shutdown(config.sleep_interval, shutdown);
    }
}

/// Emits the initial (static) output for a followed regular file and returns
/// the position where polling resumes; `None` when the operand is not a
/// seekable regular file and cannot be followed.
fn initial_file_output<W: Write>(
    file: &mut File,
    config: &TailConfig,
    shutdown: &AtomicBool,
    out: &mut W,
    buf: &mut [u8],
) -> Result<Option<u64>> {
    match probe_seekable(file) {
        Some(len) => tail_seekable(file, len, config, shutdown, out, buf).map(Some),
        None => {
            tail_stream(file, config, shutdown, out, buf)?;
            Ok(None)
        }
    }
}

/// Sleeps the follow interval in slices of at most 100 ms, re-checking the
/// shutdown flag between slices so Ctrl+C stays responsive.
fn sleep_with_shutdown(total: Duration, shutdown: &AtomicBool) {
    let slice = Duration::from_millis(100);
    let mut remaining = total;
    while !remaining.is_zero() && !shutdown.load(std::sync::atomic::Ordering::Relaxed) {
        let step = remaining.min(slice);
        std::thread::sleep(step);
        remaining = remaining.saturating_sub(step);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use shared_head_tail::models::{HeaderPolicy, RunOutcome};
    use std::fs::OpenOptions;
    use std::io::ErrorKind;
    use std::sync::atomic::AtomicBool;
    use tempfile::tempdir;

    fn config(unit: CountUnit, count: u64) -> TailConfig {
        TailConfig {
            unit,
            count,
            from_start: false,
            follow: None,
            retry: false,
            sleep_interval: Duration::from_millis(1),
            max_unchanged_stats: None,
            headers: HeaderPolicy::Auto,
            zero_terminated: false,
            files: Vec::new(),
        }
    }

    fn run_stream(cfg: &TailConfig, input: &[u8], chunk: usize) -> Vec<u8> {
        let flag = AtomicBool::new(false);
        let mut buf = vec![0u8; chunk];
        let mut out = Vec::new();
        let mut reader = input;
        tail_stream(&mut reader, cfg, &flag, &mut out, &mut buf).unwrap();
        out
    }

    fn run_seekable(cfg: &TailConfig, content: &[u8], chunk: usize) -> Vec<u8> {
        let dir = tempdir().unwrap();
        let path = dir.path().join("in.txt");
        std::fs::write(&path, content).unwrap();
        let mut file = File::open(&path).unwrap();
        let flag = AtomicBool::new(false);
        let mut buf = vec![0u8; chunk];
        let mut out = Vec::new();
        tail_seekable(
            &mut file,
            content.len() as u64,
            cfg,
            &flag,
            &mut out,
            &mut buf,
        )
        .unwrap();
        out
    }

    #[rstest]
    #[case(10, b"a\nb\nc\n".as_slice(), b"a\nb\nc\n".as_slice())]
    #[case(2, b"1\n2\n3\n4\n".as_slice(), b"3\n4\n".as_slice())]
    #[case(2, b"1\n2\n3\nfragment".as_slice(), b"3\nfragment".as_slice())]
    #[case(0, b"a\nb\n".as_slice(), b"".as_slice())]
    #[case(5, b"only\n".as_slice(), b"only\n".as_slice())]
    fn last_lines_agree_between_seekable_and_pipe(
        #[case] count: u64,
        #[case] input: &[u8],
        #[case] expected: &[u8],
    ) {
        let cfg = config(CountUnit::Lines, count);
        for chunk in [3usize, 64 * 1024] {
            assert_eq!(
                run_stream(&cfg, input, chunk),
                expected,
                "pipe chunk={chunk}"
            );
            assert_eq!(
                run_seekable(&cfg, input, chunk),
                expected,
                "seekable chunk={chunk}"
            );
        }
    }

    #[test]
    fn last_lines_cutoff_exactly_at_chunk_boundary() {
        // With chunk size 3, the delimiter of interest sits exactly on a
        // boundary of the backward scan.
        let cfg = config(CountUnit::Lines, 2);
        let input = b"ab\ncd\nef\n";
        assert_eq!(run_seekable(&cfg, input, 3), b"cd\nef\n");
    }

    #[rstest]
    #[case(4, b"abcdefgh".as_slice(), b"efgh".as_slice())]
    #[case(100, b"abc".as_slice(), b"abc".as_slice())]
    #[case(0, b"abc".as_slice(), b"".as_slice())]
    fn last_bytes_agree_between_seekable_and_pipe(
        #[case] count: u64,
        #[case] input: &[u8],
        #[case] expected: &[u8],
    ) {
        let cfg = config(CountUnit::Bytes, count);
        assert_eq!(run_stream(&cfg, input, 3), expected);
        assert_eq!(run_seekable(&cfg, input, 3), expected);
    }

    #[test]
    fn zero_terminated_items_count_by_nul() {
        let mut cfg = config(CountUnit::Lines, 2);
        cfg.zero_terminated = true;
        assert_eq!(run_stream(&cfg, b"a\0b\0c\0", 64), b"b\0c\0");
    }

    #[rstest]
    #[case(3, b"1\n2\n3\n4\n".as_slice(), b"3\n4\n".as_slice())]
    #[case(1, b"1\n2\n".as_slice(), b"1\n2\n".as_slice())]
    #[case(0, b"1\n2\n".as_slice(), b"1\n2\n".as_slice())]
    #[case(100, b"1\n2\n".as_slice(), b"".as_slice())]
    fn from_start_lines_cases(#[case] count: u64, #[case] input: &[u8], #[case] expected: &[u8]) {
        let mut cfg = config(CountUnit::Lines, count);
        cfg.from_start = true;
        assert_eq!(run_stream(&cfg, input, 3), expected);
        assert_eq!(run_seekable(&cfg, input, 3), expected);
    }

    #[rstest]
    #[case(3, b"abcdef".as_slice(), b"cdef".as_slice())]
    #[case(1, b"abc".as_slice(), b"abc".as_slice())]
    #[case(0, b"abc".as_slice(), b"abc".as_slice())]
    #[case(100, b"abc".as_slice(), b"".as_slice())]
    fn from_start_bytes_cases(#[case] count: u64, #[case] input: &[u8], #[case] expected: &[u8]) {
        let mut cfg = config(CountUnit::Bytes, count);
        cfg.from_start = true;
        assert_eq!(run_stream(&cfg, input, 2), expected);
        assert_eq!(run_seekable(&cfg, input, 2), expected);
    }

    #[test]
    fn static_run_reports_missing_file_and_continues() {
        let dir = tempdir().unwrap();
        let real = dir.path().join("real.txt");
        std::fs::write(&real, b"content\n").unwrap();

        let mut cfg = config(CountUnit::Lines, 10);
        cfg.headers = HeaderPolicy::Never;
        cfg.files = vec![
            dir.path().join("missing").to_string_lossy().to_string(),
            real.to_string_lossy().to_string(),
        ];
        let flag = AtomicBool::new(false);
        let mut out = Vec::new();
        let result = run(&cfg, &flag, &mut out).unwrap();
        assert!(!result.all_ok);
        assert_eq!(out, b"content\n");
    }

    #[test]
    fn directory_operand_is_a_per_file_error() {
        let dir = tempdir().unwrap();
        let real = dir.path().join("real.txt");
        std::fs::write(&real, b"ok\n").unwrap();

        let mut cfg = config(CountUnit::Lines, 10);
        cfg.headers = HeaderPolicy::Never;
        cfg.files = vec![
            dir.path().to_string_lossy().to_string(),
            real.to_string_lossy().to_string(),
        ];
        let flag = AtomicBool::new(false);
        let mut out = Vec::new();
        let result = run(&cfg, &flag, &mut out).unwrap();
        assert!(!result.all_ok);
        assert_eq!(out, b"ok\n");
    }

    #[test]
    fn broken_pipe_yields_quiet_success() {
        struct ClosedPipe;
        impl Write for ClosedPipe {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::new(ErrorKind::BrokenPipe, "closed"))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let dir = tempdir().unwrap();
        let path = dir.path().join("in.txt");
        std::fs::write(&path, b"data\n").unwrap();

        let mut cfg = config(CountUnit::Lines, 10);
        cfg.files = vec![path.to_string_lossy().to_string()];
        let flag = AtomicBool::new(false);
        let result = run(&cfg, &flag, &mut ClosedPipe).unwrap();
        assert!(result.all_ok);
    }

    // --- follow_cycle tests: no timing, no threads ---

    struct FlushCounter {
        bytes: Vec<u8>,
        flushes: usize,
    }

    impl FlushCounter {
        fn new() -> Self {
            FlushCounter {
                bytes: Vec::new(),
                flushes: 0,
            }
        }
    }

    impl Write for FlushCounter {
        fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
            self.bytes.extend_from_slice(data);
            Ok(data.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.flushes += 1;
            Ok(())
        }
    }

    struct FollowFixture {
        sources: Vec<FollowSource>,
        settings: FollowSettings,
        last_printed: Option<usize>,
        header_state: HeaderState,
        out: FlushCounter,
        flag: AtomicBool,
        buf: Vec<u8>,
    }

    impl FollowFixture {
        fn new(settings: FollowSettings) -> Self {
            FollowFixture {
                sources: Vec::new(),
                settings,
                last_printed: None,
                header_state: HeaderState::new(),
                out: FlushCounter::new(),
                flag: AtomicBool::new(false),
                buf: vec![0u8; 64],
            }
        }

        fn add_opened(&mut self, id: usize, path: PathBuf) {
            let file = File::open(&path).unwrap();
            let position = file.metadata().unwrap().len();
            self.sources
                .push(FollowSource::opened(id, path, file, position));
        }

        fn cycle(&mut self) -> CycleReport {
            follow_cycle(
                &mut self.sources,
                &self.settings,
                &mut self.last_printed,
                &mut self.header_state,
                &self.flag,
                &mut self.out,
                &mut self.buf,
            )
            .unwrap()
        }
    }

    fn descriptor_settings() -> FollowSettings {
        FollowSettings {
            mode: FollowMode::Descriptor,
            retry: false,
            headers: false,
        }
    }

    fn name_settings(retry: bool) -> FollowSettings {
        FollowSettings {
            mode: FollowMode::Name,
            retry,
            headers: false,
        }
    }

    fn append(path: &PathBuf, data: &[u8]) {
        let mut file = OpenOptions::new().append(true).open(path).unwrap();
        file.write_all(data).unwrap();
    }

    #[test]
    fn appended_bytes_stream_and_cycle_flushes() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("log.txt");
        std::fs::write(&path, b"start\n").unwrap();

        let mut fixture = FollowFixture::new(descriptor_settings());
        fixture.add_opened(0, path.clone());

        let idle = fixture.cycle();
        assert!(!idle.wrote_any);
        assert_eq!(fixture.out.flushes, 0, "no-change cycle must not flush");

        append(&path, b"more\n");
        let busy = fixture.cycle();
        assert!(busy.wrote_any);
        assert_eq!(fixture.out.bytes, b"more\n");
        assert_eq!(fixture.out.flushes, 1, "output cycle must flush once");
    }

    #[test]
    fn truncation_resets_to_start_and_reprocesses() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("log.txt");
        std::fs::write(&path, b"original content\n").unwrap();

        let mut fixture = FollowFixture::new(descriptor_settings());
        fixture.add_opened(0, path.clone());

        std::fs::write(&path, b"new\n").unwrap();
        let report = fixture.cycle();
        assert!(report.wrote_any);
        assert_eq!(fixture.out.bytes, b"new\n");
    }

    #[test]
    fn name_mode_without_retry_drops_a_removed_source() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("log.txt");
        std::fs::write(&path, b"x\n").unwrap();

        let mut fixture = FollowFixture::new(name_settings(false));
        fixture.add_opened(0, path.clone());

        std::fs::remove_file(&path).unwrap();
        let report = fixture.cycle();
        assert!(report.dropped_any);
        assert!(fixture.sources.is_empty());
    }

    #[test]
    fn name_mode_holds_no_file_handle_between_cycles() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("log.txt");
        std::fs::write(&path, b"x\n").unwrap();

        let mut fixture = FollowFixture::new(name_settings(true));
        fixture.add_opened(0, path.clone());
        assert!(fixture.sources[0].file.is_some());

        fixture.cycle();

        assert!(fixture.sources[0].file.is_none());
    }

    #[test]
    fn failed_source_outcome_drops_only_without_retry() {
        assert!(failed_source_outcome(false).drop_source);
        assert!(!failed_source_outcome(true).drop_source);
        assert!(!failed_source_outcome(false).wrote);
    }

    #[test]
    fn abandon_suffix_names_the_consequence_only_without_retry() {
        assert!(abandon_suffix(true).is_empty());
        assert!(abandon_suffix(false).contains("--retry"));
    }

    #[test]
    fn name_mode_with_retry_survives_remove_and_recreate() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("log.txt");
        std::fs::write(&path, b"first\n").unwrap();

        let mut fixture = FollowFixture::new(name_settings(true));
        fixture.add_opened(0, path.clone());

        std::fs::remove_file(&path).unwrap();
        let gone = fixture.cycle();
        assert!(!gone.dropped_any);
        assert_eq!(fixture.sources.len(), 1);

        std::fs::write(&path, b"reborn\n").unwrap();
        let back = fixture.cycle();
        assert!(back.wrote_any);
        assert_eq!(fixture.out.bytes, b"reborn\n");
    }

    #[cfg(unix)]
    #[test]
    fn name_mode_detects_rotation_by_identity() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("log.txt");
        std::fs::write(&path, b"old old old\n").unwrap();

        let mut fixture = FollowFixture::new(name_settings(true));
        fixture.add_opened(0, path.clone());

        // Same-name replacement with equal length: only the inode differs.
        let rotated = dir.path().join("rotated");
        std::fs::write(&rotated, b"new new new\n").unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::rename(&rotated, &path).unwrap();

        let report = fixture.cycle();
        assert!(report.wrote_any);
        assert_eq!(fixture.out.bytes, b"new new new\n");
    }

    #[test]
    fn header_reprints_when_the_producing_source_changes() {
        let dir = tempdir().unwrap();
        let a = dir.path().join("a.log");
        let b = dir.path().join("b.log");
        std::fs::write(&a, b"").unwrap();
        std::fs::write(&b, b"").unwrap();

        let mut settings = descriptor_settings();
        settings.headers = true;
        let mut fixture = FollowFixture::new(settings);
        fixture.add_opened(0, a.clone());
        fixture.add_opened(1, b.clone());

        append(&a, b"1\n");
        fixture.cycle();
        append(&b, b"2\n");
        fixture.cycle();
        append(&a, b"3\n");
        fixture.cycle();

        let expected = format!(
            "==> {} <==\n1\n\n==> {} <==\n2\n\n==> {} <==\n3\n",
            a.display(),
            b.display(),
            a.display()
        );
        assert_eq!(fixture.out.bytes, expected.as_bytes());
    }

    #[test]
    fn header_not_reprinted_for_the_same_source() {
        let dir = tempdir().unwrap();
        let a = dir.path().join("a.log");
        std::fs::write(&a, b"").unwrap();

        let mut settings = descriptor_settings();
        settings.headers = true;
        let mut fixture = FollowFixture::new(settings);
        fixture.add_opened(0, a.clone());

        append(&a, b"1\n");
        fixture.cycle();
        append(&a, b"2\n");
        fixture.cycle();

        let expected = format!("==> {} <==\n1\n2\n", a.display());
        assert_eq!(fixture.out.bytes, expected.as_bytes());
    }

    // --- follow termination tests (no polling loop is entered) ---

    #[test]
    fn interrupt_after_a_failed_file_still_reports_interrupted() {
        // A writer that flips the shutdown flag on its first write simulates
        // Ctrl+C landing after an earlier per-file failure; the interrupted
        // outcome must win over the failure (main maps it to 130 first).
        struct FlagSettingWriter<'a> {
            flag: &'a AtomicBool,
        }
        impl Write for FlagSettingWriter<'_> {
            fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
                self.flag.store(true, std::sync::atomic::Ordering::Relaxed);
                Ok(data.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let dir = tempdir().unwrap();
        let real = dir.path().join("real.txt");
        std::fs::write(&real, b"line\n").unwrap();

        let mut cfg = config(CountUnit::Lines, 10);
        cfg.headers = HeaderPolicy::Never;
        cfg.files = vec![
            dir.path().join("missing").to_string_lossy().to_string(),
            real.to_string_lossy().to_string(),
        ];
        let flag = AtomicBool::new(false);
        let mut out = FlagSettingWriter { flag: &flag };
        let result = run(&cfg, &flag, &mut out).unwrap();
        assert_eq!(result.outcome, RunOutcome::Interrupted);
    }

    #[test]
    fn follow_missing_file_without_retry_fails_with_no_files_remaining() {
        let dir = tempdir().unwrap();
        let mut cfg = config(CountUnit::Lines, 10);
        cfg.follow = Some(FollowMode::Descriptor);
        cfg.files = vec![dir.path().join("absent").to_string_lossy().to_string()];

        let flag = AtomicBool::new(false);
        let mut out = Vec::new();
        let result = run(&cfg, &flag, &mut out).unwrap();
        assert_eq!(result.outcome, RunOutcome::Completed);
        assert!(!result.all_ok);
        assert!(out.is_empty());
    }

    #[test]
    fn follow_interrupt_flag_yields_interrupted() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("log.txt");
        std::fs::write(&path, b"data\n").unwrap();

        let mut cfg = config(CountUnit::Lines, 10);
        cfg.follow = Some(FollowMode::Descriptor);
        cfg.files = vec![path.to_string_lossy().to_string()];

        let flag = AtomicBool::new(true);
        let mut out = Vec::new();
        let result = run(&cfg, &flag, &mut out).unwrap();
        assert_eq!(result.outcome, RunOutcome::Interrupted);
    }

    #[test]
    fn follow_initial_output_positions_at_end() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("log.txt");
        std::fs::write(&path, b"1\n2\n3\n").unwrap();

        let cfg = config(CountUnit::Lines, 2);
        let mut file = File::open(&path).unwrap();
        let flag = AtomicBool::new(false);
        let mut out = Vec::new();
        let mut buf = vec![0u8; 64];
        let position = initial_file_output(&mut file, &cfg, &flag, &mut out, &mut buf)
            .unwrap()
            .unwrap();
        assert_eq!(out, b"2\n3\n");
        assert_eq!(position, 6);
    }
}

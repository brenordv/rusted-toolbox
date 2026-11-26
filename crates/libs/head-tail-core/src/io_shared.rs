use crate::models::{HeaderPolicy, RunOutcome, RunResult};
use anyhow::{Context, Result};
use common_cli::tool_exit_helpers::{exit_error, exit_success, exit_with_code};
use common_utils::constants::EXIT_CODE_INTERRUPTED_BY_USER;
use std::fs::File;
use std::io::{ErrorKind, Read, Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::error;

pub use common_cli::broken_pipe::{BrokenPipe, flush_out, write_out};

/// One input operand, resolved from the positional arguments.
pub enum Input<'a> {
    Stdin,
    File(&'a str),
}

impl Input<'_> {
    pub fn display_name(&self) -> &str {
        match self {
            Input::Stdin => "standard input",
            Input::File(name) => name,
        }
    }
}

/// Maps the positional operands to inputs: none means stdin, `-` means stdin.
pub fn resolve_inputs(files: &[String]) -> Vec<Input<'_>> {
    if files.is_empty() {
        return vec![Input::Stdin];
    }
    files
        .iter()
        .map(|name| {
            if name == "-" {
                Input::Stdin
            } else {
                Input::File(name)
            }
        })
        .collect()
}

/// Probes whether an open file is a seekable regular file, returning its
/// length. The probe reads no bytes; stdin and pipes take the streaming path.
pub fn probe_seekable(file: &mut File) -> Option<u64> {
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() {
        return None;
    }
    file.stream_position().ok()?;
    Some(metadata.len())
}

/// Drives one static engine pass over the resolved inputs: header policy,
/// per-input shutdown checks, and the continue-on-error accounting. The
/// marker errors (closed pipe, Ctrl+C) propagate; anything else is logged
/// with the tool prefix and turns the run into a failure at the end.
///
/// # Errors
/// Propagates only the [`BrokenPipe`] and [`Interrupted`] markers; every
/// other per-input error, writer failures included, is logged and accounted
/// in the returned flag, never returned as an error.
pub fn run_inputs<W: Write, F>(
    files: &[String],
    headers: HeaderPolicy,
    tool: &str,
    shutdown: &AtomicBool,
    out: &mut W,
    buf: &mut [u8],
    mut process_input: F,
) -> Result<bool>
where
    F: FnMut(&Input<'_>, bool, &mut HeaderState, &mut W, &mut [u8]) -> Result<()>,
{
    let inputs = resolve_inputs(files);
    let show_headers = headers.show(inputs.len());
    let mut header_state = HeaderState::new();
    let mut all_ok = true;

    for input in &inputs {
        check_shutdown(shutdown)?;
        if let Err(error) = process_input(input, show_headers, &mut header_state, out, buf) {
            if error.is::<BrokenPipe>() || error.is::<Interrupted>() {
                return Err(error);
            }
            error!("{tool}: {:#}", error);
            all_ok = false;
        }
    }

    Ok(all_ok)
}

/// Shared tail-end of an engine run: flushes, and maps the marker errors
/// (closed pipe, Ctrl+C) to their quiet outcomes.
///
/// # Errors
/// Returns only non-marker failures (an unwritable destination, say); the
/// markers become [`RunResult`] outcomes instead.
pub fn finish_run<W: Write>(result: Result<bool>, out: &mut W) -> Result<RunResult> {
    match result {
        Ok(all_ok) => match flush_out(out) {
            Ok(()) => Ok(RunResult::completed(all_ok)),
            Err(error) if error.is::<BrokenPipe>() => Ok(RunResult::completed(true)),
            Err(error) => Err(error),
        },
        Err(error) if error.is::<BrokenPipe>() => Ok(RunResult::completed(true)),
        Err(error) if error.is::<Interrupted>() => {
            let _ = out.flush();
            Ok(RunResult::interrupted())
        }
        Err(error) => Err(error),
    }
}

/// Maps an engine result to the shared exit contract: 130 on interrupt
/// (which wins over per-file failures), 0 when every input processed,
/// 1 otherwise.
pub fn exit_from_result(result: Result<RunResult>) -> ! {
    match result {
        Ok(run) if run.outcome == RunOutcome::Interrupted => {
            exit_with_code(EXIT_CODE_INTERRUPTED_BY_USER)
        }
        Ok(run) if run.all_ok => exit_success(),
        Ok(_) => exit_error(),
        Err(error) => {
            error!("{:#}", error);
            exit_error();
        }
    }
}

/// Marks a run stopped by the Ctrl+C shutdown flag.
///
/// The engines map it to `RunOutcome::Interrupted`, which the exit mapping
/// turns into exit code 130.
#[derive(Debug)]
pub struct Interrupted;

impl std::fmt::Display for Interrupted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "interrupted by the user")
    }
}

impl std::error::Error for Interrupted {}

/// Fails with the [`Interrupted`] marker when the shutdown flag is set.
///
/// # Errors
/// Fails exactly when the flag is set.
pub fn check_shutdown(shutdown: &AtomicBool) -> Result<()> {
    if shutdown.load(Ordering::Relaxed) {
        Err(anyhow::Error::new(Interrupted))
    } else {
        Ok(())
    }
}

/// Reads one chunk, retrying reads the OS interrupted with a signal.
///
/// # Errors
/// Propagates any read error other than `ErrorKind::Interrupted`.
pub fn read_chunk<R: Read>(reader: &mut R, buf: &mut [u8]) -> std::io::Result<usize> {
    loop {
        match reader.read(buf) {
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            other => return other,
        }
    }
}

/// Runs `on_item` for every delimiter-terminated item read from `reader`; a
/// trailing fragment without the delimiter counts as a final item. Callers
/// supply the retention policy (write-through overflow for head's elide,
/// capped keep-last for tail).
///
/// # Errors
/// Propagates read failures, `on_item` failures, and the [`Interrupted`]
/// marker when the shutdown flag is set mid-read.
pub fn split_items<R: Read, F>(
    reader: &mut R,
    delimiter: u8,
    shutdown: &AtomicBool,
    buf: &mut [u8],
    mut on_item: F,
) -> Result<()>
where
    F: FnMut(Vec<u8>) -> Result<()>,
{
    let mut partial: Vec<u8> = Vec::new();
    loop {
        check_shutdown(shutdown)?;
        let n = read_chunk(reader, buf).context("failed to read input")?;
        if n == 0 {
            break;
        }
        let mut rest = &buf[..n];
        while let Some(pos) = rest.iter().position(|&b| b == delimiter) {
            partial.extend_from_slice(&rest[..=pos]);
            on_item(std::mem::take(&mut partial))?;
            rest = &rest[pos + 1..];
        }
        partial.extend_from_slice(rest);
    }

    if !partial.is_empty() {
        on_item(partial)?;
    }
    Ok(())
}

/// Opens and routes one input operand: stdin goes to `stream`, a file operand
/// is opened, gets its header, and goes to `seekable` (regular files) or
/// `stream` (pipes and special files). The engines supply the two paths as
/// closures over their own configuration.
///
/// # Errors
/// Propagates open failures, header write failures, and whatever the chosen
/// closure returns, with the input name attached as context.
pub fn process_input_source<W, S, T>(
    input: &Input<'_>,
    show_headers: bool,
    header_state: &mut HeaderState,
    out: &mut W,
    buf: &mut [u8],
    mut seekable: S,
    mut stream: T,
) -> Result<()>
where
    W: Write,
    S: FnMut(&mut File, u64, &mut W, &mut [u8]) -> Result<()>,
    T: FnMut(&mut dyn Read, &mut W, &mut [u8]) -> Result<()>,
{
    match input {
        Input::Stdin => {
            if show_headers {
                header_state.write(out, input.display_name())?;
            }
            let stdin = std::io::stdin();
            let mut lock = stdin.lock();
            stream(&mut lock, out, buf).context("error reading standard input")
        }
        Input::File(name) => {
            let mut file =
                File::open(name).with_context(|| format!("cannot open '{name}' for reading"))?;
            if show_headers {
                header_state.write(out, name)?;
            }
            // A reported length of 0 also covers procfs-style virtual files,
            // which stat at 0 yet yield content when read; the streaming path
            // handles those and genuinely empty files alike, the way GNU
            // falls back when st_size is not usable.
            let result = match probe_seekable(&mut file) {
                Some(len) if len > 0 => seekable(&mut file, len, out, buf),
                _ => stream(&mut file, out, buf),
            };
            result.with_context(|| format!("error reading '{name}'"))
        }
    }
}

/// Fills `buf` as far as the source allows, stopping only at EOF.
///
/// Returns the byte count actually read, which is less than `buf.len()` only
/// when the source ended early (for the backward scan: the file shrank).
fn read_to_fill<R: Read>(reader: &mut R, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        let n = read_chunk(reader, &mut buf[filled..])?;
        if n == 0 {
            break;
        }
        filled += n;
    }
    Ok(filled)
}

/// Tracks whether any file name header was printed during a run.
///
/// Every header after the first is preceded by one blank line, matching the
/// GNU `write_header` rule; headers are always newline-terminated text, even
/// under `-z`.
pub struct HeaderState {
    printed_any: bool,
}

impl Default for HeaderState {
    fn default() -> Self {
        HeaderState::new()
    }
}

impl HeaderState {
    pub fn new() -> Self {
        HeaderState { printed_any: false }
    }

    /// Emits the `==> NAME <==` header for one input.
    ///
    /// # Errors
    /// Propagates writer failures, with a closed pipe as the marker.
    pub fn write<W: Write>(&mut self, out: &mut W, name: &str) -> Result<()> {
        let prefix = if self.printed_any { "\n" } else { "" };
        write_out(out, format!("{prefix}==> {name} <==\n").as_bytes())?;
        self.printed_any = true;
        Ok(())
    }
}

/// Finds the offset where the last `count` delimiter-separated items begin.
///
/// Reads fixed chunks backward from `len`, counting delimiters. A trailing
/// fragment (input not ending in the delimiter) counts as one item. Fewer
/// than `count` items yields offset 0; `count == 0` yields `len` (no items
/// from the end). A short read mid-scan means the source shrank; the scan
/// terminates by clamping to offset 0 rather than retrying.
///
/// # Errors
/// Propagates seek and read failures on the input.
pub fn backward_scan_start<R: Read + Seek>(
    input: &mut R,
    len: u64,
    count: u64,
    delimiter: u8,
    chunk_size: usize,
) -> Result<u64> {
    if len == 0 {
        return Ok(0);
    }
    if count == 0 {
        return Ok(len);
    }

    let mut needed = count;
    let mut first_chunk = true;
    let mut pos = len;
    let mut buf = vec![0u8; chunk_size.max(1)];

    while pos > 0 {
        let read_start = pos.saturating_sub(buf.len() as u64);
        let want = usize::try_from(pos - read_start).unwrap_or(buf.len());
        input
            .seek(SeekFrom::Start(read_start))
            .context("failed to seek during the backward scan")?;
        let got = read_to_fill(input, &mut buf[..want])
            .context("failed to read during the backward scan")?;
        if got == 0 {
            return Ok(0);
        }

        let chunk = &buf[..got];
        if first_chunk {
            first_chunk = false;
            if chunk[got - 1] == delimiter {
                // The final delimiter terminates the last item; one more
                // delimiter must be found to reach that item's start.
                needed = needed.saturating_add(1);
            }
        }

        let mut i = got;
        while i > 0 {
            i -= 1;
            if chunk[i] == delimiter {
                needed -= 1;
                if needed == 0 {
                    return Ok(read_start + i as u64 + 1);
                }
            }
        }

        if got < want {
            return Ok(0);
        }
        pos = read_start;
    }

    Ok(0)
}

/// Copies up to `limit` bytes from `reader` to `out`, returning the count copied.
///
/// # Errors
/// Propagates read failures, writer failures (closed pipe as the marker), and
/// the [`Interrupted`] marker when the shutdown flag is set mid-copy.
pub fn copy_limited<R: Read, W: Write>(
    reader: &mut R,
    out: &mut W,
    limit: u64,
    shutdown: &AtomicBool,
    buf: &mut [u8],
) -> Result<u64> {
    let mut remaining = limit;
    let mut copied = 0u64;
    while remaining > 0 {
        check_shutdown(shutdown)?;
        let want = usize::try_from(remaining.min(buf.len() as u64)).unwrap_or(buf.len());
        let n = read_chunk(reader, &mut buf[..want]).context("failed to read input")?;
        if n == 0 {
            break;
        }
        write_out(out, &buf[..n])?;
        copied = copied.saturating_add(n as u64);
        remaining = remaining.saturating_sub(n as u64);
    }
    Ok(copied)
}

/// Copies from `reader` to `out` until EOF, returning the count copied.
///
/// # Errors
/// Propagates read failures, writer failures (closed pipe as the marker), and
/// the [`Interrupted`] marker when the shutdown flag is set mid-copy.
pub fn copy_to_end<R: Read, W: Write>(
    reader: &mut R,
    out: &mut W,
    shutdown: &AtomicBool,
    buf: &mut [u8],
) -> Result<u64> {
    let mut copied = 0u64;
    loop {
        check_shutdown(shutdown)?;
        let n = read_chunk(reader, buf).context("failed to read input")?;
        if n == 0 {
            return Ok(copied);
        }
        write_out(out, &buf[..n])?;
        copied = copied.saturating_add(n as u64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::sync::atomic::AtomicBool;

    fn scan(data: &[u8], count: u64, delimiter: u8, chunk: usize) -> u64 {
        let mut cursor = Cursor::new(data.to_vec());
        backward_scan_start(&mut cursor, data.len() as u64, count, delimiter, chunk).unwrap()
    }

    #[test]
    fn backward_scan_finds_last_items_with_trailing_delimiter() {
        assert_eq!(scan(b"a\nb\nc\n", 2, b'\n', 64), 2);
        assert_eq!(scan(b"a\nb\nc\n", 1, b'\n', 64), 4);
        assert_eq!(scan(b"a\nb\nc\n", 3, b'\n', 64), 0);
    }

    #[test]
    fn backward_scan_counts_trailing_fragment_as_item() {
        assert_eq!(scan(b"a\nb\nc", 1, b'\n', 64), 4);
        assert_eq!(scan(b"a\nb\nc", 2, b'\n', 64), 2);
        assert_eq!(scan(b"abc", 1, b'\n', 64), 0);
    }

    #[test]
    fn backward_scan_handles_delimiter_at_chunk_boundary() {
        // Chunk size 2 over "ab\ncd\nef" puts delimiters on both sides of
        // chunk boundaries.
        let data = b"ab\ncd\nef";
        for chunk in 1..=4 {
            assert_eq!(scan(data, 1, b'\n', chunk), 6, "chunk={chunk}");
            assert_eq!(scan(data, 2, b'\n', chunk), 3, "chunk={chunk}");
            assert_eq!(scan(data, 3, b'\n', chunk), 0, "chunk={chunk}");
        }
    }

    #[test]
    fn backward_scan_file_smaller_than_chunk() {
        assert_eq!(scan(b"x\ny\n", 1, b'\n', 4096), 2);
    }

    #[test]
    fn backward_scan_fewer_items_than_requested_yields_zero() {
        assert_eq!(scan(b"only\n", 10, b'\n', 64), 0);
    }

    #[test]
    fn backward_scan_zero_count_yields_len() {
        assert_eq!(scan(b"a\nb\n", 0, b'\n', 64), 4);
    }

    #[test]
    fn backward_scan_empty_input_yields_zero() {
        assert_eq!(scan(b"", 3, b'\n', 64), 0);
    }

    #[test]
    fn backward_scan_clamps_when_the_source_is_shorter_than_len() {
        // A len larger than the real content stands in for a file that shrank
        // after its length was captured: the scan clamps to offset 0 rather
        // than retrying, unless the short data already satisfied the count.
        let data = b"a\nb\n";

        // The single read comes back short (got < want) without covering the
        // requested item count.
        let mut cursor = Cursor::new(data.to_vec());
        assert_eq!(backward_scan_start(&mut cursor, 8, 5, b'\n', 8).unwrap(), 0);

        // The first backward chunk lands entirely past the real EOF (got == 0).
        let mut cursor = Cursor::new(data.to_vec());
        assert_eq!(backward_scan_start(&mut cursor, 8, 1, b'\n', 4).unwrap(), 0);

        // A short read that still covers the count returns the real offset.
        let mut cursor = Cursor::new(data.to_vec());
        assert_eq!(backward_scan_start(&mut cursor, 8, 1, b'\n', 8).unwrap(), 2);
    }

    #[test]
    fn backward_scan_supports_nul_delimiter() {
        assert_eq!(scan(b"a\0b\0c\0", 1, 0, 64), 4);
        assert_eq!(scan(b"a\0b\0c", 2, 0, 3), 2);
    }

    #[test]
    fn copy_limited_stops_at_limit_and_at_eof() {
        let flag = AtomicBool::new(false);
        let mut buf = vec![0u8; 4];

        let mut out = Vec::new();
        let copied =
            copy_limited(&mut Cursor::new(b"abcdef"), &mut out, 3, &flag, &mut buf).unwrap();
        assert_eq!(copied, 3);
        assert_eq!(out, b"abc");

        let mut out = Vec::new();
        let copied = copy_limited(&mut Cursor::new(b"ab"), &mut out, 10, &flag, &mut buf).unwrap();
        assert_eq!(copied, 2);
        assert_eq!(out, b"ab");
    }

    #[test]
    fn copy_to_end_copies_everything() {
        let flag = AtomicBool::new(false);
        let mut buf = vec![0u8; 3];
        let mut out = Vec::new();
        let copied = copy_to_end(&mut Cursor::new(b"abcdefg"), &mut out, &flag, &mut buf).unwrap();
        assert_eq!(copied, 7);
        assert_eq!(out, b"abcdefg");
    }

    #[test]
    fn shutdown_flag_maps_to_interrupted_marker() {
        let flag = AtomicBool::new(true);
        let mut buf = vec![0u8; 3];
        let mut out = Vec::new();
        let err = copy_to_end(&mut Cursor::new(b"abc"), &mut out, &flag, &mut buf).unwrap_err();
        assert!(err.is::<Interrupted>());
    }

    fn route_file(path: &std::path::Path) -> &'static str {
        let name = path.to_string_lossy().to_string();
        let input = Input::File(&name);
        let mut state = HeaderState::new();
        let mut out = Vec::new();
        let mut buf = vec![0u8; 8];
        let seekable_hit = std::cell::Cell::new(false);
        let stream_hit = std::cell::Cell::new(false);
        process_input_source(
            &input,
            false,
            &mut state,
            &mut out,
            &mut buf,
            |_, _, _, _| {
                seekable_hit.set(true);
                Ok(())
            },
            |_, _, _| {
                stream_hit.set(true);
                Ok(())
            },
        )
        .unwrap();
        match (seekable_hit.get(), stream_hit.get()) {
            (true, false) => "seekable",
            (false, true) => "stream",
            other => panic!("unexpected routing: {other:?}"),
        }
    }

    #[test]
    fn zero_length_file_routes_to_the_stream_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty");
        std::fs::write(&path, b"").unwrap();
        assert_eq!(route_file(&path), "stream");
    }

    #[test]
    fn nonempty_regular_file_routes_to_the_seekable_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("full");
        std::fs::write(&path, b"data\n").unwrap();
        assert_eq!(route_file(&path), "seekable");
    }

    #[test]
    fn headers_use_blank_line_separator_after_first() {
        let mut out = Vec::new();
        let mut state = HeaderState::new();
        state.write(&mut out, "a.txt").unwrap();
        state.write(&mut out, "b.txt").unwrap();
        assert_eq!(out, b"==> a.txt <==\n\n==> b.txt <==\n");
    }

    #[test]
    fn finish_run_maps_markers_to_quiet_outcomes() {
        let mut out = Vec::new();
        let ok = finish_run(Ok(true), &mut out).unwrap();
        assert_eq!(ok, RunResult::completed(true));

        let pipe = finish_run(Err(anyhow::Error::new(BrokenPipe)), &mut out).unwrap();
        assert_eq!(pipe, RunResult::completed(true));

        let interrupted = finish_run(Err(anyhow::Error::new(Interrupted)), &mut out).unwrap();
        assert_eq!(interrupted, RunResult::interrupted());
    }
}

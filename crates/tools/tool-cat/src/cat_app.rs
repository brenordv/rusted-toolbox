use crate::models::CatConfig;

use anyhow::{Context, Result};
use common_cli::broken_pipe::{flush_out, write_out, BrokenPipe};
use common_utils::constants::SIZE_128KB;
use tracing::{debug, error};

use std::fs::File;
use std::io::{self, ErrorKind, Read, Write};

/// Width of the decimal line-number buffer; a `u64` never exceeds twenty digits.
const LINE_NUMBER_DIGITS: usize = 20;

/// Minimum width of the printed line-number field, matching GNU cat's `%6d`.
const LINE_NUMBER_MIN_WIDTH: usize = 6;

/// State carried across every input of a single invocation.
///
/// The line counter and blank-run flag persist across files so `-n` numbers
/// continuously and `-s` squeezes across a file boundary. The within-line flags
/// persist too, so an unterminated last line of one file joins the first bytes
/// of the next.
struct CatState {
    line_digits: [u8; LINE_NUMBER_DIGITS],
    line_num_start: usize,
    line_has_content: bool,
    number_emitted: bool,
    prev_line_blank: bool,
    pending_cr: bool,
}

impl CatState {
    fn new() -> Self {
        CatState {
            line_digits: [b'0'; LINE_NUMBER_DIGITS],
            line_num_start: LINE_NUMBER_DIGITS - 1,
            line_has_content: false,
            number_emitted: false,
            prev_line_blank: false,
            pending_cr: false,
        }
    }

    fn reset_line(&mut self) {
        self.line_has_content = false;
        self.number_emitted = false;
    }

    /// Advances the line number by incrementing its ASCII digits in place.
    ///
    /// Avoids a per-line integer-to-decimal conversion: only the trailing digits
    /// that carry are touched, and the field widens by one when the leading digit
    /// rolls over.
    fn next_line_number(&mut self) {
        let mut i = LINE_NUMBER_DIGITS - 1;
        loop {
            if self.line_digits[i] < b'9' {
                self.line_digits[i] += 1;
                return;
            }
            self.line_digits[i] = b'0';
            if i == self.line_num_start {
                self.line_num_start -= 1;
                self.line_digits[self.line_num_start] = b'1';
                return;
            }
            i -= 1;
        }
    }

    /// Appends the current line number, right-justified to the minimum width, and a tab.
    fn write_line_number(&self, cooked: &mut Vec<u8>) {
        const PAD: &[u8] = b"      ";
        let width = LINE_NUMBER_DIGITS - self.line_num_start;
        if width < LINE_NUMBER_MIN_WIDTH {
            cooked.extend_from_slice(&PAD[..LINE_NUMBER_MIN_WIDTH - width]);
        }
        cooked.extend_from_slice(&self.line_digits[self.line_num_start..]);
        cooked.push(b'\t');
    }
}

/// Concatenates every input to `out`, returning whether the whole run succeeded.
///
/// A missing or unreadable file is reported and the run continues with the next
/// input, ending in a failure result. A closed output pipe ends the run at once
/// with success. `true` means exit 0, `false` means exit 1.
pub fn run<W: Write>(options: &CatConfig, out: &mut W) -> bool {
    let mut state = CatState::new();
    let mut all_ok = true;

    let inputs: Vec<Option<&str>> = if options.files.is_empty() {
        vec![None]
    } else {
        options.files.iter().map(|f| Some(f.as_str())).collect()
    };

    for input in inputs {
        match cat_file(input, options, &mut state, out) {
            Ok(()) => {}
            Err(e) if e.is::<BrokenPipe>() => return broken_pipe_exit(),
            Err(e) => {
                error!("cat: {:#}", e);
                all_ok = false;
            }
        }
    }

    let mut trailing = Vec::new();
    finish_trailing_cr(options, &mut state, &mut trailing);

    if let Err(e) = write_out(out, &trailing).and_then(|()| flush_out(out)) {
        if e.is::<BrokenPipe>() {
            return broken_pipe_exit();
        }
        error!("cat: {:#}", e);
        all_ok = false;
    }

    all_ok
}

fn broken_pipe_exit() -> bool {
    debug!("stopping early: output pipe closed by the consumer");
    true
}

/// Opens one input and streams it through the cooked or raw path.
fn cat_file<W: Write>(
    path: Option<&str>,
    options: &CatConfig,
    state: &mut CatState,
    out: &mut W,
) -> Result<()> {
    match path {
        None | Some("-") => {
            let stdin = io::stdin();
            let mut reader = stdin.lock();
            process_reader(&mut reader, options, state, out, true, "standard input")
        }
        Some(name) => {
            let mut file = File::open(name).with_context(|| format!("failed to open '{name}'"))?;
            process_reader(&mut file, options, state, out, false, name)
        }
    }
}

/// Dispatches to the formatting scanner or the plain copy based on the options.
fn process_reader<R: Read, W: Write>(
    reader: &mut R,
    options: &CatConfig,
    state: &mut CatState,
    out: &mut W,
    is_blocking: bool,
    source: &str,
) -> Result<()> {
    if options.needs_line_processing() {
        cook_stream(reader, options, state, out, SIZE_128KB, is_blocking, source)
    } else {
        raw_copy(reader, out, SIZE_128KB, is_blocking, source)
    }
}

/// Copies bytes straight through in fixed chunks, with no formatting.
fn raw_copy<R: Read, W: Write>(
    reader: &mut R,
    out: &mut W,
    chunk_size: usize,
    is_blocking: bool,
    source: &str,
) -> Result<()> {
    let mut buf = vec![0u8; chunk_size];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                write_out(out, &buf[..n])?;
                if is_blocking {
                    flush_out(out)?;
                }
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) => return Err(e).with_context(|| format!("error reading {source}")),
        }
    }
    Ok(())
}

/// Scans the input in fixed chunks, applying the active formatting options.
///
/// Each chunk is rendered into a reusable in-memory buffer and written out in a
/// single call, so output cost scales with the input size rather than the line
/// count. The buffer is flushed after every chunk, and on a blocking source the
/// writer is flushed too so `-n` stays responsive on a pipe or terminal.
fn cook_stream<R: Read, W: Write>(
    reader: &mut R,
    options: &CatConfig,
    state: &mut CatState,
    out: &mut W,
    chunk_size: usize,
    is_blocking: bool,
    source: &str,
) -> Result<()> {
    let mut buf = vec![0u8; chunk_size];
    let mut cooked: Vec<u8> = Vec::with_capacity(chunk_size);
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                process_chunk(&buf[..n], options, state, &mut cooked);
                write_out(out, &cooked)?;
                cooked.clear();
                if is_blocking {
                    flush_out(out)?;
                }
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) => return Err(e).with_context(|| format!("error reading {source}")),
        }
    }
    Ok(())
}

/// Renders one chunk of input into `cooked`, applying the active formatting options.
///
/// When no byte needs rewriting (`-n`, `-b`, `-s`, or plain numbering combos), the
/// only significant byte is the newline, so a faster line-oriented path handles it.
fn process_chunk(chunk: &[u8], options: &CatConfig, state: &mut CatState, cooked: &mut Vec<u8>) {
    if options.show_nonprinting || options.show_tabs || options.show_ends {
        process_chunk_rendered(chunk, options, state, cooked);
    } else {
        process_chunk_plain(chunk, options, state, cooked);
    }
}

/// Fast path for modes that leave every byte intact and only act on line ends.
///
/// The only significant byte is the newline, so each line's content is scanned to
/// its terminator and copied in one call instead of one test per byte.
fn process_chunk_plain(
    chunk: &[u8],
    options: &CatConfig,
    state: &mut CatState,
    cooked: &mut Vec<u8>,
) {
    let len = chunk.len();
    let mut i = 0;
    while i < len {
        let start = i;
        while i < len && chunk[i] != b'\n' {
            i += 1;
        }
        if i > start {
            state.line_has_content = true;
            ensure_number_for_content(options, state, cooked);
            cooked.extend_from_slice(&chunk[start..i]);
        }
        if i < len {
            handle_newline(options, state, cooked);
            i += 1;
        }
    }
}

/// Per-byte path for modes that rewrite bytes (`-v`, `-T`, `-E` and their combos).
fn process_chunk_rendered(
    chunk: &[u8],
    options: &CatConfig,
    state: &mut CatState,
    cooked: &mut Vec<u8>,
) {
    if state.pending_cr && chunk.first() != Some(&b'\n') {
        state.pending_cr = false;
        render_content_byte(b'\r', options, cooked);
    }

    let len = chunk.len();
    let mut i = 0;
    while i < len {
        let byte = chunk[i];

        if byte == b'\n' {
            handle_newline(options, state, cooked);
            i += 1;
            continue;
        }

        if byte == b'\r' && options.show_ends {
            state.line_has_content = true;
            ensure_number_for_content(options, state, cooked);
            match chunk.get(i + 1) {
                Some(b'\n') => cooked.extend_from_slice(b"^M"),
                Some(_) => render_content_byte(b'\r', options, cooked),
                None => state.pending_cr = true,
            }
            i += 1;
            continue;
        }

        state.line_has_content = true;
        ensure_number_for_content(options, state, cooked);

        if is_passthrough(byte, options) {
            let start = i;
            i += 1;
            while i < len && is_passthrough(chunk[i], options) {
                i += 1;
            }
            cooked.extend_from_slice(&chunk[start..i]);
        } else {
            render_content_byte(byte, options, cooked);
            i += 1;
        }
    }
}

/// Reports whether a byte renders as itself under the active options.
///
/// A run of such bytes can be copied in one write instead of one call per byte.
/// The cases mirror `render_content_byte` exactly: a newline is always a line
/// terminator, a tab passes through only without `-T`, a `\r` before a shown line
/// end is owned by the line-end logic, and with `-v` only the printable range
/// 0x20..=0x7e survives unchanged.
fn is_passthrough(byte: u8, options: &CatConfig) -> bool {
    match byte {
        b'\n' => false,
        b'\t' => !options.show_tabs,
        b'\r' if options.show_ends => false,
        _ if !options.show_nonprinting => true,
        0x20..=0x7e => true,
        _ => false,
    }
}

/// Emits everything owed at a line terminator: numbering, squeeze, `$`, newline.
fn handle_newline(options: &CatConfig, state: &mut CatState, cooked: &mut Vec<u8>) {
    let is_blank = !state.line_has_content;

    if options.squeeze_blank && is_blank && state.prev_line_blank {
        state.reset_line();
        return;
    }

    if is_blank {
        state.prev_line_blank = true;
        if options.number && !options.number_nonblank {
            state.next_line_number();
            state.write_line_number(cooked);
        }
    } else {
        state.prev_line_blank = false;
    }

    if options.show_ends {
        if state.pending_cr {
            cooked.extend_from_slice(b"^M");
            state.pending_cr = false;
        }
        cooked.push(b'$');
    }

    cooked.push(b'\n');
    state.reset_line();
}

/// Writes the line-number prefix once, on the first content byte of a line.
fn ensure_number_for_content(options: &CatConfig, state: &mut CatState, cooked: &mut Vec<u8>) {
    if options.number && !state.number_emitted {
        state.next_line_number();
        state.write_line_number(cooked);
        state.number_emitted = true;
    }
}

/// Renders a single non-terminator byte under `-T` and `-v`.
///
/// `-T` turns a tab into `^I`; `-v` shows control bytes as `^X`, DEL as `^?`,
/// and high bytes with `M-` notation split at 128 + 32. Without `-v` the byte
/// passes through unchanged.
fn render_content_byte(byte: u8, options: &CatConfig, cooked: &mut Vec<u8>) {
    if byte == b'\t' {
        if options.show_tabs {
            cooked.extend_from_slice(b"^I");
        } else {
            cooked.push(b'\t');
        }
        return;
    }

    if !options.show_nonprinting {
        cooked.push(byte);
        return;
    }

    if byte < 32 {
        cooked.extend_from_slice(&[b'^', byte + 64]);
        return;
    }

    if byte < 127 {
        cooked.push(byte);
        return;
    }

    if byte == 127 {
        cooked.extend_from_slice(b"^?");
        return;
    }

    cooked.extend_from_slice(b"M-");
    if byte >= 160 {
        if byte < 255 {
            cooked.push(byte - 128);
        } else {
            cooked.extend_from_slice(b"^?");
        }
    } else {
        cooked.extend_from_slice(&[b'^', byte - 128 + 64]);
    }
}

/// Renders a carriage return deferred at the very end of all input.
fn finish_trailing_cr(options: &CatConfig, state: &mut CatState, cooked: &mut Vec<u8>) {
    if state.pending_cr {
        state.pending_cr = false;
        render_content_byte(b'\r', options, cooked);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli_utils::CliArgs;
    use clap::Parser;
    use common_cli::test_writers::FailAfter;
    use rstest::*;
    use tempfile::tempdir;

    fn opts() -> CatConfig {
        CatConfig {
            number_nonblank: false,
            show_ends: false,
            number: false,
            squeeze_blank: false,
            show_tabs: false,
            show_nonprinting: false,
            files: vec![],
        }
    }

    fn run_cook(
        input: &[u8],
        options: &CatConfig,
        chunk: usize,
        state: &mut CatState,
        out: &mut Vec<u8>,
    ) {
        let mut reader = input;
        cook_stream(&mut reader, options, state, out, chunk, false, "test").unwrap();
    }

    fn cook(input: &[u8], options: &CatConfig, chunk: usize) -> Vec<u8> {
        let mut state = CatState::new();
        let mut out = Vec::new();
        run_cook(input, options, chunk, &mut state, &mut out);
        finish_trailing_cr(options, &mut state, &mut out);
        out
    }

    #[test]
    fn raw_copy_passes_bytes_through_including_non_utf8_and_no_final_newline() {
        let input: &[u8] = &[b'a', 0xFF, b'\n', 0x00, b'b'];
        let mut out = Vec::new();
        let mut reader = input;

        raw_copy(&mut reader, &mut out, 4, false, "test").unwrap();

        assert_eq!(out, input);
    }

    #[test]
    fn number_all_lines_uses_six_wide_field_and_tab() {
        let mut o = opts();
        o.number = true;

        let out = cook(b"line1\nline2\n", &o, 64);

        assert_eq!(out, b"     1\tline1\n     2\tline2\n");
    }

    #[test]
    fn number_continues_across_two_files_sharing_state() {
        let mut o = opts();
        o.number = true;
        let mut state = CatState::new();
        let mut out = Vec::new();

        run_cook(b"a\n", &o, 64, &mut state, &mut out);
        run_cook(b"b\n", &o, 64, &mut state, &mut out);
        finish_trailing_cr(&o, &mut state, &mut out);

        assert_eq!(out, b"     1\ta\n     2\tb\n");
    }

    #[test]
    fn number_nonblank_skips_blank_lines_entirely() {
        let mut o = opts();
        o.number = true;
        o.number_nonblank = true;

        let out = cook(b"a\n\nb\n", &o, 64);

        assert_eq!(out, b"     1\ta\n\n     2\tb\n");
    }

    #[test]
    fn number_nonblank_leaves_leading_blank_line_empty() {
        let mut o = opts();
        o.number = true;
        o.number_nonblank = true;

        let out = cook(b"\nx\n", &o, 64);

        assert_eq!(out, b"\n     1\tx\n");
    }

    #[test]
    fn number_nonblank_counts_crlf_line_as_nonempty() {
        let mut o = opts();
        o.number = true;
        o.number_nonblank = true;

        let out = cook(b"\r\n", &o, 64);

        assert_eq!(out, b"     1\t\r\n");
    }

    fn seed_line_number(state: &mut CatState, ascii: &[u8]) {
        state.line_num_start = LINE_NUMBER_DIGITS - ascii.len();
        state.line_digits[state.line_num_start..].copy_from_slice(ascii);
    }

    #[test]
    fn line_number_field_pads_short_numbers_to_width_six() {
        let mut state = CatState::new();
        seed_line_number(&mut state, b"42");
        let mut out = Vec::new();

        state.write_line_number(&mut out);

        assert_eq!(out, b"    42\t");
    }

    #[test]
    fn line_number_carries_and_widens_past_six_digits() {
        let mut state = CatState::new();
        seed_line_number(&mut state, b"999999");
        let mut out = Vec::new();

        state.next_line_number();
        state.write_line_number(&mut out);

        assert_eq!(out, b"1000000\t");
    }

    #[test]
    fn line_number_carries_a_single_digit_rollover() {
        let mut state = CatState::new();
        seed_line_number(&mut state, b"19");
        let mut out = Vec::new();

        state.next_line_number();
        state.write_line_number(&mut out);

        assert_eq!(out, b"    20\t");
    }

    #[test]
    fn squeeze_blank_collapses_runs_to_one() {
        let mut o = opts();
        o.squeeze_blank = true;

        let out = cook(b"a\n\n\n\nb\n", &o, 64);

        assert_eq!(out, b"a\n\nb\n");
    }

    #[test]
    fn squeeze_blank_collapses_run_spanning_chunk_boundary() {
        let mut o = opts();
        o.squeeze_blank = true;

        let out = cook(b"a\n\n\n\nb\n", &o, 3);

        assert_eq!(out, b"a\n\nb\n");
    }

    #[test]
    fn squeeze_blank_collapses_run_spanning_file_boundary() {
        let mut o = opts();
        o.squeeze_blank = true;
        let mut state = CatState::new();
        let mut out = Vec::new();

        run_cook(b"a\n\n", &o, 64, &mut state, &mut out);
        run_cook(b"\n\nb\n", &o, 64, &mut state, &mut out);
        finish_trailing_cr(&o, &mut state, &mut out);

        assert_eq!(out, b"a\n\nb\n");
    }

    #[test]
    fn show_ends_marks_line_ends_and_crlf() {
        let mut o = opts();
        o.show_ends = true;

        let out = cook(b"a\nb\r\n", &o, 64);

        assert_eq!(out, b"a$\nb^M$\n");
    }

    #[test]
    fn show_ends_omits_dollar_on_unterminated_final_line() {
        let mut o = opts();
        o.show_ends = true;

        let out = cook(b"a\nb", &o, 64);

        assert_eq!(out, b"a$\nb");
    }

    #[test]
    fn show_tabs_renders_tab_as_caret_i() {
        let mut o = opts();
        o.show_tabs = true;

        let out = cook(b"a\tb\n", &o, 64);

        assert_eq!(out, b"a^Ib\n");
    }

    #[rstest]
    #[case(b"\x01".as_slice(), b"^A".as_slice())]
    #[case(b"\x1b".as_slice(), b"^[".as_slice())]
    #[case(b"a\tb".as_slice(), b"a\tb".as_slice())]
    #[case(b"a\nb\n".as_slice(), b"a\nb\n".as_slice())]
    #[case(b"\x7f".as_slice(), b"^?".as_slice())]
    #[case(b"\x80".as_slice(), b"M-^@".as_slice())]
    #[case(b"\xa0".as_slice(), b"M- ".as_slice())]
    #[case("é".as_bytes(), b"M-CM-)".as_slice())]
    #[case(b"\xff".as_slice(), b"M-^?".as_slice())]
    fn show_nonprinting_renders_bytes(#[case] input: &[u8], #[case] expected: &[u8]) {
        let mut o = opts();
        o.show_nonprinting = true;

        let out = cook(input, &o, 64);

        assert_eq!(out, expected);
    }

    #[test]
    fn show_all_renders_crlf_as_single_caret_m_then_dollar() {
        let mut o = opts();
        o.show_nonprinting = true;
        o.show_ends = true;
        o.show_tabs = true;

        let out = cook(b"a\r\n", &o, 64);

        assert_eq!(out, b"a^M$\n");
    }

    #[test]
    fn line_straddling_chunk_boundary_numbers_once() {
        let mut o = opts();
        o.number = true;

        let out = cook(b"abcdef\n", &o, 3);

        assert_eq!(out, b"     1\tabcdef\n");
    }

    #[test]
    fn crlf_split_across_chunk_boundary_under_show_ends() {
        let mut o = opts();
        o.show_ends = true;

        let out = cook(b"ab\r\ncd\n", &o, 3);

        assert_eq!(out, b"ab^M$\ncd$\n");
    }

    #[rstest]
    #[case(false)]
    #[case(true)]
    fn crlf_only_line_split_at_boundary_numbers_before_render(#[case] number_nonblank: bool) {
        let mut o = opts();
        o.number = true;
        o.number_nonblank = number_nonblank;
        o.show_ends = true;

        let out = cook(b"\r\n", &o, 1);

        assert_eq!(out, b"     1\t^M$\n");
    }

    #[test]
    fn crlf_only_line_split_at_boundary_plain_number() {
        let mut o = opts();
        o.number = true;

        let out = cook(b"\r\n", &o, 1);

        assert_eq!(out, b"     1\t\r\n");
    }

    #[test]
    fn parse_to_run_with_number_nonblank_flag() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("in.txt");
        std::fs::write(&path, b"a\n\nb\n").unwrap();
        let args = CliArgs::try_parse_from(["cat", "-b", path.to_str().unwrap()]).unwrap();
        let config = CatConfig::from_args(&args);
        let mut out = Vec::new();

        let ok = run(&config, &mut out);

        assert!(ok);
        assert_eq!(out, b"     1\ta\n\n     2\tb\n");
    }

    #[test]
    fn parse_to_run_with_show_all_flag() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("in.txt");
        std::fs::write(&path, b"a\tb\r\n").unwrap();
        let args = CliArgs::try_parse_from(["cat", "-A", path.to_str().unwrap()]).unwrap();
        let config = CatConfig::from_args(&args);
        let mut out = Vec::new();

        let ok = run(&config, &mut out);

        assert!(ok);
        assert_eq!(out, b"a^Ib^M$\n");
    }

    #[test]
    fn run_with_number_and_squeeze_numbers_the_kept_blank_line() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("in.txt");
        std::fs::write(&path, b"a\n\n\n\nb\n").unwrap();
        let mut o = opts();
        o.number = true;
        o.squeeze_blank = true;
        o.files = vec![path.to_str().unwrap().to_string()];
        let mut out = Vec::new();

        let ok = run(&o, &mut out);

        assert!(ok);
        assert_eq!(out, b"     1\ta\n     2\t\n     3\tb\n");
    }

    #[test]
    fn run_with_number_and_show_ends_across_two_files() {
        let dir = tempdir().unwrap();
        let first = dir.path().join("first.txt");
        let second = dir.path().join("second.txt");
        std::fs::write(&first, b"a\n").unwrap();
        std::fs::write(&second, b"b\n").unwrap();
        let mut o = opts();
        o.number = true;
        o.show_ends = true;
        o.files = vec![
            first.to_str().unwrap().to_string(),
            second.to_str().unwrap().to_string(),
        ];
        let mut out = Vec::new();

        let ok = run(&o, &mut out);

        assert!(ok);
        assert_eq!(out, b"     1\ta$\n     2\tb$\n");
    }

    #[test]
    fn broken_pipe_during_output_ends_run_with_success() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("data.txt");
        std::fs::write(&path, b"hello world, this is more than eight bytes\n").unwrap();
        let mut o = opts();
        o.files = vec![path.to_str().unwrap().to_string()];
        let mut out = FailAfter::broken_pipe(8);

        let ok = run(&o, &mut out);

        assert!(ok);
    }

    #[test]
    fn missing_file_is_reported_but_processing_continues() {
        let dir = tempdir().unwrap();
        let real = dir.path().join("real.txt");
        std::fs::write(&real, b"real content\n").unwrap();
        let mut o = opts();
        o.files = vec![
            "definitely_missing_file_xyz.txt".to_string(),
            real.to_str().unwrap().to_string(),
        ];
        let mut out = Vec::new();

        let ok = run(&o, &mut out);

        assert!(!ok);
        assert_eq!(out, b"real content\n");
    }
}

use crate::models::GetLinesConfig;
use aho_corasick::{AhoCorasick, AhoCorasickBuilder};
use anyhow::{bail, Context, Result};
use common_utils::constants::SIZE_128KB;
use common_utils::string_utils::sanitize_string_for_filename;
use std::collections::HashSet;
use std::fs::{create_dir_all, File};
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::warn;

/// A single line longer than this (with no newline) aborts the run: the input is not line-oriented.
const MAX_LINE_BYTES: usize = 64 * 1024 * 1024;

/// In console mode, the buffered writer is flushed after this many matched lines so
/// interactive output appears while the run progresses.
const CONSOLE_FLUSH_INTERVAL: usize = 256;

/// How the run ended.
#[derive(Debug, PartialEq, Eq)]
pub enum RunOutcome {
    /// The whole input was scanned.
    Completed,
    /// A Ctrl+C shutdown was observed mid-scan; output is partial.
    Interrupted,
}

/// Matches search terms against a line.
///
/// The ASCII engine searches raw bytes with overlapping Aho-Corasick so that nested terms
/// (`error` containing `or`) are all reported. The Unicode fallback preserves the previous
/// Unicode-aware case folding for any term carrying a non-ASCII character.
enum Matcher {
    Ascii {
        automaton: AhoCorasick,
        term_count: usize,
    },
    Unicode(Vec<String>),
}

impl Matcher {
    /// Builds the matcher for a set of already-normalized (trimmed, lowercased) terms.
    ///
    /// The ASCII engine is used only when every term is pure ASCII; a single non-ASCII term
    /// routes all matching through the Unicode fallback.
    fn build(terms: &[String]) -> Result<Matcher> {
        if terms.iter().all(|term| term.is_ascii()) {
            let automaton = AhoCorasickBuilder::new()
                .ascii_case_insensitive(true)
                .build(terms)
                .context("failed to build the search automaton")?;

            Ok(Matcher::Ascii {
                automaton,
                term_count: terms.len(),
            })
        } else {
            Ok(Matcher::Unicode(terms.to_vec()))
        }
    }

    /// Fills `hits` with the distinct term indices present in `raw`, in ascending order.
    ///
    /// `raw` is the line with its end-of-line bytes already stripped.
    fn collect_hits(&self, raw: &[u8], hits: &mut Vec<usize>) {
        hits.clear();

        match self {
            Matcher::Ascii {
                automaton,
                term_count,
            } => {
                for found in automaton.find_overlapping_iter(raw) {
                    let index = found.pattern().as_usize();
                    if !hits.contains(&index) {
                        hits.push(index);
                        if hits.len() == *term_count {
                            break;
                        }
                    }
                }
                hits.sort_unstable();
            }
            Matcher::Unicode(terms) => {
                let decoded = String::from_utf8_lossy(raw);
                let lowered = decoded.to_lowercase();
                for (index, term) in terms.iter().enumerate() {
                    if lowered.contains(term.as_str()) {
                        hits.push(index);
                    }
                }
            }
        }
    }
}

/// A per-term output file paired with the context needed for error messages.
struct FileSink {
    term: String,
    path: PathBuf,
    writer: BufWriter<File>,
}

/// Where matched lines are written.
enum Sinks {
    /// One buffered file per term, aligned with the term index.
    Files(Vec<FileSink>),
    /// A single buffered writer over standard output.
    Console(BufWriter<Box<dyn Write>>),
}

impl Sinks {
    /// Writes a matched line to the sinks for the terms in `hits`.
    ///
    /// File mode writes the line to each matched term's file. Console mode has no per-term
    /// separation, so a line matching several terms is written once.
    fn write_match(&mut self, hits: &[usize], bytes: &[u8]) -> Result<()> {
        match self {
            Sinks::Files(files) => {
                for &term_index in hits {
                    let sink = &mut files[term_index];
                    sink.writer.write_all(bytes).with_context(|| {
                        format!(
                            "failed writing to output file {} for term '{}'",
                            sink.path.display(),
                            sink.term
                        )
                    })?;
                }
                Ok(())
            }
            Sinks::Console(writer) => writer.write_all(bytes).context("failed writing to stdout"),
        }
    }

    /// Flushes every underlying writer, propagating the first failure.
    fn flush_all(&mut self) -> Result<()> {
        match self {
            Sinks::Files(files) => {
                for sink in files.iter_mut() {
                    sink.writer.flush().with_context(|| {
                        format!(
                            "failed flushing output file {} for term '{}'",
                            sink.path.display(),
                            sink.term
                        )
                    })?;
                }
                Ok(())
            }
            Sinks::Console(writer) => writer.flush().context("failed flushing stdout"),
        }
    }
}

/// Returns `true` if `name` matches a Windows reserved device name (case-insensitive).
///
/// The restriction applies even with an extension, so a term that sanitizes to one of these
/// must be renamed before it becomes a filename stem.
fn is_windows_reserved(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();

    if matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL") {
        return true;
    }

    is_numbered_device(&upper, "COM") || is_numbered_device(&upper, "LPT")
}

/// Returns `true` if `upper` is `prefix` followed by a single device number (1-9 or its superscript).
fn is_numbered_device(upper: &str, prefix: &str) -> bool {
    match upper.strip_prefix(prefix) {
        Some(rest) => matches!(
            rest,
            "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "\u{b9}" | "\u{b2}" | "\u{b3}"
        ),
        None => false,
    }
}

/// Builds one output filename stem per term, aligned with the term indices.
///
/// Each term is sanitized; an empty result falls back to `term-<index>`, reserved device names
/// are prefixed with `term-`, and any collision with an already-assigned stem is resolved by
/// appending `-2`, `-3`, ... until the stem is free. Comparison is case-insensitive so the
/// stems stay distinct on case-insensitive filesystems.
fn build_output_filenames(terms: &[String]) -> Vec<String> {
    let mut assigned: Vec<String> = Vec::with_capacity(terms.len());
    let mut used: HashSet<String> = HashSet::with_capacity(terms.len());

    for (index, term) in terms.iter().enumerate() {
        let mut base = sanitize_string_for_filename(term);

        if base.is_empty() {
            base = format!("term-{}", index);
        }

        if is_windows_reserved(&base) {
            base = format!("term-{}", base);
        }

        let mut candidate = base.clone();
        let mut suffix = 2;
        while used.contains(&candidate.to_lowercase()) {
            candidate = format!("{}-{}", base, suffix);
            suffix += 1;
        }

        used.insert(candidate.to_lowercase());
        assigned.push(candidate);
    }

    assigned
}

/// Opens the output sinks described by `config`.
///
/// In file mode the output directory is created and one file per term is opened up front, so a
/// creation failure surfaces before any scanning happens.
fn build_sinks(config: &GetLinesConfig) -> Result<Sinks> {
    match &config.output {
        Some(output_dir) => {
            create_dir_all(output_dir).with_context(|| {
                format!("failed to create output directory {}", output_dir.display())
            })?;

            let names = build_output_filenames(&config.search);
            let mut files = Vec::with_capacity(config.search.len());

            for (term, name) in config.search.iter().zip(names) {
                let path = output_dir.join(format!("{}.txt", name));
                let file = File::create(&path).with_context(|| {
                    format!(
                        "failed to create output file {} for term '{}'",
                        path.display(),
                        term
                    )
                })?;

                files.push(FileSink {
                    term: term.clone(),
                    path,
                    writer: BufWriter::new(file),
                });
            }

            Ok(Sinks::Files(files))
        }
        None => {
            let writer: Box<dyn Write> = Box::new(std::io::stdout());
            Ok(Sinks::Console(BufWriter::new(writer)))
        }
    }
}

/// Runs the search over `config.file`, writing matches to the configured sinks.
///
/// Every write and flush error is propagated with the offending term and path; the sinks are
/// always flushed before returning, whether the scan completed, was interrupted, or failed.
pub fn run(config: &GetLinesConfig, shutdown_signal: Arc<AtomicBool>) -> Result<RunOutcome> {
    run_with_cap(config, shutdown_signal, MAX_LINE_BYTES)
}

/// [`run`] with an explicit per-line byte cap, so tests can exercise the cap with a tiny value.
fn run_with_cap(
    config: &GetLinesConfig,
    shutdown_signal: Arc<AtomicBool>,
    max_line_bytes: usize,
) -> Result<RunOutcome> {
    let file = File::open(&config.file)
        .with_context(|| format!("failed to open input file {}", config.file.display()))?;
    let reader = BufReader::with_capacity(SIZE_128KB, file);

    let matcher = Matcher::build(&config.search)?;
    let mut sinks = build_sinks(config)?;

    let scan_result = scan(
        reader,
        &matcher,
        &mut sinks,
        config,
        shutdown_signal,
        max_line_bytes,
    );
    let flush_result = sinks.flush_all();

    match scan_result {
        Ok(outcome) => flush_result.map(|()| outcome),
        Err(error) => Err(error),
    }
}

/// The single-threaded scan loop.
fn scan(
    mut reader: BufReader<File>,
    matcher: &Matcher,
    sinks: &mut Sinks,
    config: &GetLinesConfig,
    shutdown_signal: Arc<AtomicBool>,
    max_line_bytes: usize,
) -> Result<RunOutcome> {
    let is_console = matches!(sinks, Sinks::Console(_));

    let mut buffer: Vec<u8> = Vec::with_capacity(8 * 1024);
    let mut hits: Vec<usize> = Vec::new();
    let mut output_line: String = String::new();
    let mut line_number: usize = 0;
    let mut matched_lines: usize = 0;

    loop {
        if shutdown_signal.load(Ordering::Relaxed) {
            warn!("interrupted after {} lines; output is partial", line_number);
            return Ok(RunOutcome::Interrupted);
        }

        buffer.clear();
        let bytes_read = (&mut reader)
            .take(max_line_bytes as u64 + 1)
            .read_until(b'\n', &mut buffer)
            .with_context(|| format!("failed to read input near line {}", line_number + 1))?;

        if bytes_read == 0 {
            break;
        }
        line_number += 1;

        let has_newline = buffer.last() == Some(&b'\n');
        if !has_newline && buffer.len() > max_line_bytes {
            bail!(
                "line {} exceeds the {}-byte limit; input does not look line-oriented",
                line_number,
                max_line_bytes
            );
        }

        let mut end = buffer.len();
        if has_newline {
            end -= 1;
            if end > 0 && buffer[end - 1] == b'\r' {
                end -= 1;
            }
        }
        let raw = &buffer[..end];

        matcher.collect_hits(raw, &mut hits);
        if hits.is_empty() {
            continue;
        }

        let decoded = String::from_utf8_lossy(raw);
        output_line.clear();
        if !config.hide_line_numbers {
            output_line.push_str(&line_number.to_string());
            output_line.push('\t');
        }
        output_line.push_str(&decoded);
        output_line.push('\n');

        sinks.write_match(&hits, output_line.as_bytes())?;

        matched_lines += 1;
        if is_console && matched_lines.is_multiple_of(CONSOLE_FLUSH_INTERVAL) {
            sinks.flush_all()?;
        }
    }

    Ok(RunOutcome::Completed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

    fn hits_of(matcher: &Matcher, line: &[u8]) -> Vec<usize> {
        let mut hits = Vec::new();
        matcher.collect_hits(line, &mut hits);
        hits
    }

    fn terms(list: &[&str]) -> Vec<String> {
        list.iter().map(|term| term.to_string()).collect()
    }

    fn config_for(file: &Path, output: Option<PathBuf>, hide_line_numbers: bool) -> GetLinesConfig {
        GetLinesConfig {
            search: terms(&["error"]),
            file: file.to_path_buf(),
            output,
            hide_line_numbers,
        }
    }

    fn run_ok(config: &GetLinesConfig) -> RunOutcome {
        run(config, Arc::new(AtomicBool::new(false))).unwrap()
    }

    // --- Matcher ---------------------------------------------------------

    #[test]
    fn ascii_matcher_is_case_insensitive() {
        let matcher = Matcher::build(&terms(&["error"])).unwrap();
        assert_eq!(hits_of(&matcher, b"An ERROR happened"), vec![0]);
    }

    #[test]
    fn overlapping_terms_are_both_reported() {
        let matcher = Matcher::build(&terms(&["error", "or"])).unwrap();
        // "or" is nested inside "error"; non-overlapping search would lose one of them.
        assert_eq!(hits_of(&matcher, b"a single error here"), vec![0, 1]);
    }

    #[test]
    fn repeated_occurrences_report_each_term_once() {
        let matcher = Matcher::build(&terms(&["ab"])).unwrap();
        assert_eq!(hits_of(&matcher, b"abababab"), vec![0]);
    }

    #[test]
    fn multiple_terms_on_one_line() {
        let matcher = Matcher::build(&terms(&["foo", "bar"])).unwrap();
        assert_eq!(hits_of(&matcher, b"foo and bar"), vec![0, 1]);
    }

    #[test]
    fn non_ascii_term_routes_to_unicode_engine() {
        let matcher = Matcher::build(&terms(&["café"])).unwrap();
        assert!(matches!(matcher, Matcher::Unicode(_)));
        assert_eq!(hits_of(&matcher, "a CAFÉ open".as_bytes()), vec![0]);
    }

    #[test]
    fn mixed_term_set_routes_all_matching_through_unicode_engine() {
        // A single non-ASCII term disables the ASCII fast path for the whole set.
        let matcher = Matcher::build(&terms(&["error", "café"])).unwrap();
        assert!(matches!(matcher, Matcher::Unicode(_)));

        // The ASCII term still matches, case-insensitively, through the Unicode engine.
        assert_eq!(hits_of(&matcher, b"an ERROR happened"), vec![0]);
        assert_eq!(hits_of(&matcher, "the CAFÉ closed".as_bytes()), vec![1]);
        assert_eq!(
            hits_of(&matcher, "ERROR at the CAFÉ".as_bytes()),
            vec![0, 1]
        );
    }

    // --- Filename map ----------------------------------------------------

    #[test]
    fn filenames_suffix_collisions_including_suffix_that_collides_again() {
        let names = build_output_filenames(&terms(&["foo/", "foo_", "foo_-2"]));
        assert_eq!(names, terms(&["foo_", "foo_-2", "foo_-2-2"]));
    }

    #[test]
    fn filenames_avoid_windows_reserved_names() {
        let names = build_output_filenames(&terms(&["nul"]));
        assert_eq!(names, terms(&["term-nul"]));
    }

    #[test]
    fn dot_only_term_falls_back_to_indexed_name() {
        let names = build_output_filenames(&terms(&["abc", "..."]));
        assert_eq!(names, terms(&["abc", "term-1"]));
    }

    // --- Engine ----------------------------------------------------------

    #[test]
    fn invalid_utf8_line_does_not_stop_the_scan() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.log");
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"alpha error\n");
        bytes.extend_from_slice(&[0xff, 0xfe]);
        bytes.extend_from_slice(b" beta error\n");
        bytes.extend_from_slice(b"gamma error\n");
        fs::write(&input, &bytes).unwrap();

        let out = dir.path().join("out");
        run_ok(&config_for(&input, Some(out.clone()), false));

        let content = fs::read_to_string(out.join("error.txt")).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].starts_with("1\t"));
        assert!(lines[1].starts_with("2\t"));
        assert!(lines[2].starts_with("3\tgamma error"));
        // The invalid bytes survive as the replacement character rather than truncating the file.
        assert!(lines[1].contains('\u{fffd}'));
    }

    #[test]
    fn crlf_input_produces_output_without_carriage_return() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.log");
        fs::write(&input, b"foo error\r\nbar error\r\n").unwrap();

        let out = dir.path().join("out");
        run_ok(&config_for(&input, Some(out.clone()), false));

        let content = fs::read_to_string(out.join("error.txt")).unwrap();
        assert_eq!(content, "1\tfoo error\n2\tbar error\n");
        assert!(!content.contains('\r'));
    }

    #[test]
    fn final_line_without_newline_is_processed() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.log");
        fs::write(&input, b"first error\nsecond error").unwrap();

        let out = dir.path().join("out");
        run_ok(&config_for(&input, Some(out.clone()), false));

        let content = fs::read_to_string(out.join("error.txt")).unwrap();
        assert_eq!(content, "1\tfirst error\n2\tsecond error\n");
    }

    #[test]
    fn line_without_newline_over_cap_returns_error() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.log");
        fs::write(&input, b"this line has no newline and is long").unwrap();

        let out = dir.path().join("out");
        let config = config_for(&input, Some(out), false);
        let result = run_with_cap(&config, Arc::new(AtomicBool::new(false)), 8);
        assert!(result.is_err());
    }

    #[test]
    fn hide_line_numbers_is_honored() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.log");
        fs::write(&input, b"foo error\n").unwrap();

        let out = dir.path().join("out");
        run_ok(&config_for(&input, Some(out.clone()), true));

        let content = fs::read_to_string(out.join("error.txt")).unwrap();
        assert_eq!(content, "foo error\n");
    }

    #[test]
    fn file_mode_writes_expected_lines_per_term() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.log");
        fs::write(&input, b"user john\nadmin panel\nuser alice\nsystem boot\n").unwrap();

        let out = dir.path().join("out");
        let config = GetLinesConfig {
            search: terms(&["user", "admin"]),
            file: input,
            output: Some(out.clone()),
            hide_line_numbers: false,
        };
        assert_eq!(run_ok(&config), RunOutcome::Completed);

        let user = fs::read_to_string(out.join("user.txt")).unwrap();
        assert_eq!(user, "1\tuser john\n3\tuser alice\n");

        let admin = fs::read_to_string(out.join("admin.txt")).unwrap();
        assert_eq!(admin, "2\tadmin panel\n");
    }

    #[test]
    fn input_open_failure_creates_no_output_files() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("does-not-exist.log");
        let out = dir.path().join("out");

        let config = config_for(&missing, Some(out.clone()), false);
        let result = run(&config, Arc::new(AtomicBool::new(false)));

        assert!(result.is_err());
        assert!(!out.exists());
    }

    #[test]
    fn output_dir_that_is_a_file_returns_error() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.log");
        fs::write(&input, b"foo error\n").unwrap();

        // The "output directory" already exists as a regular file.
        let out = dir.path().join("out");
        fs::write(&out, b"not a directory").unwrap();

        let config = config_for(&input, Some(out), false);
        let result = run(&config, Arc::new(AtomicBool::new(false)));
        assert!(result.is_err());
    }

    #[test]
    fn shutdown_signal_yields_interrupted_outcome() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("in.log");
        fs::write(&input, b"foo error\nbar error\n").unwrap();

        let out = dir.path().join("out");
        let config = config_for(&input, Some(out.clone()), false);

        let already_down = Arc::new(AtomicBool::new(true));
        let outcome = run(&config, already_down).unwrap();

        assert_eq!(outcome, RunOutcome::Interrupted);
        // The sinks are still opened and flushed even on an immediate interrupt.
        assert!(out.join("error.txt").exists());
    }

    /// A `Write` backed by a shared buffer so a test can inspect what the console sink emitted.
    #[derive(Clone)]
    struct SharedBuffer(std::rc::Rc<std::cell::RefCell<Vec<u8>>>);

    impl Write for SharedBuffer {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.borrow_mut().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn console_writes_a_multi_term_match_once() {
        let buffer = SharedBuffer(std::rc::Rc::new(std::cell::RefCell::new(Vec::new())));
        let writer: Box<dyn Write> = Box::new(buffer.clone());
        let mut sinks = Sinks::Console(BufWriter::new(writer));

        // The line matched two distinct terms; console output must not duplicate it.
        sinks
            .write_match(&[0, 1], b"2\ta single error here\n")
            .unwrap();
        sinks.flush_all().unwrap();

        let written = String::from_utf8(buffer.0.borrow().clone()).unwrap();
        assert_eq!(written, "2\ta single error here\n");
    }
}

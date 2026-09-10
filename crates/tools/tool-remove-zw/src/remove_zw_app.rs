use crate::models::{InputSource, OutputTarget, RemoveZwArgs};
use anyhow::{anyhow, Context, Result};
use common_utils::constants::SIZE_8KB;
use once_cell::sync::Lazy;
use regex::Regex;
use std::borrow::Cow;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

static FORMAT_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\p{Cf}").unwrap());

const BOM_UTF8: [u8; 3] = [0xEF, 0xBB, 0xBF];
const BOM_UTF16_LE: [u8; 2] = [0xFF, 0xFE];
const BOM_UTF16_BE: [u8; 2] = [0xFE, 0xFF];
const BOM_UTF32_LE: [u8; 4] = [0xFF, 0xFE, 0x00, 0x00];
const BOM_UTF32_BE: [u8; 4] = [0x00, 0x00, 0xFE, 0xFF];

/// Leading byte-order mark detected at the start of a byte stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bom {
    None,
    Utf8,
    Utf16Le,
    Utf16Be,
    Utf32Le,
    Utf32Be,
}

impl Bom {
    fn is_utf8(self) -> bool {
        matches!(self, Bom::Utf8)
    }
}

/// Detect a leading BOM. The 4-byte UTF-32 signatures are tested before the
/// 2-byte UTF-16 ones because UTF-16 LE (`FF FE`) is a prefix of UTF-32 LE
/// (`FF FE 00 00`).
fn detect_bom(bytes: &[u8]) -> Bom {
    if bytes.starts_with(&BOM_UTF32_LE) {
        Bom::Utf32Le
    } else if bytes.starts_with(&BOM_UTF32_BE) {
        Bom::Utf32Be
    } else if bytes.starts_with(&BOM_UTF8) {
        Bom::Utf8
    } else if bytes.starts_with(&BOM_UTF16_LE) {
        Bom::Utf16Le
    } else if bytes.starts_with(&BOM_UTF16_BE) {
        Bom::Utf16Be
    } else {
        Bom::None
    }
}

/// Strip a leading UTF-8 BOM if present, returning the remaining bytes and
/// whether a BOM was removed.
fn strip_leading_utf8_bom(bytes: &[u8]) -> (&[u8], bool) {
    if detect_bom(bytes).is_utf8() {
        (&bytes[BOM_UTF8.len()..], true)
    } else {
        (bytes, false)
    }
}

/// How a leading byte sample classifies for processing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SampleKind {
    Text,
    Binary,
    UnsupportedEncoding,
}

/// Classify a leading byte sample as text, binary, or an unsupported encoding.
fn classify_sample(sample: &[u8]) -> SampleKind {
    match detect_bom(sample) {
        Bom::Utf16Le | Bom::Utf16Be | Bom::Utf32Le | Bom::Utf32Be => {
            return SampleKind::UnsupportedEncoding
        }
        Bom::Utf8 | Bom::None => {}
    }

    let (body, _) = strip_leading_utf8_bom(sample);

    if body.contains(&0) {
        return SampleKind::Binary;
    }

    match std::str::from_utf8(body) {
        Ok(_) => SampleKind::Text,
        // A trailing multibyte sequence split by the sample boundary is not a
        // real error: `error_len()` is `None` only when the buffer ends
        // mid-character, which a valid UTF-8 file legitimately can.
        Err(err) if err.error_len().is_none() => SampleKind::Text,
        Err(_) => SampleKind::Binary,
    }
}

/// What cleaning a byte buffer produced.
#[derive(Debug)]
struct CleanResult {
    cleaned: String,
    bom_removed: bool,
    zero_width_removed: usize,
}

impl CleanResult {
    fn changed(&self) -> bool {
        self.bom_removed || self.zero_width_removed > 0
    }
}

/// Strip every Unicode format character from `bytes`, and the leading UTF-8
/// BOM unless `keep_bom` is set (a kept BOM stays in `cleaned` byte-exact and
/// does not count as a change).
///
/// Returns `None` only when the input (after any UTF-8 BOM) is not valid
/// UTF-8. That is the single failure mode, so callers read `None` as "not
/// text": a binary skip for files, a hard error for stdin.
fn clean_bytes(bytes: &[u8], keep_bom: bool) -> Option<CleanResult> {
    let (body, had_bom) = strip_leading_utf8_bom(bytes);
    let text = std::str::from_utf8(body).ok()?;
    let (cleaned, zero_width_removed) = strip_format_chars(text);

    let (cleaned, bom_removed) = if had_bom && keep_bom {
        (format!("\u{FEFF}{cleaned}"), false)
    } else {
        (cleaned.into_owned(), had_bom)
    };

    Some(CleanResult {
        cleaned,
        bom_removed,
        zero_width_removed,
    })
}

/// Running per-input tally, reported as a summary at the end of a run and
/// used by `main` to derive the `--check` exit code.
#[derive(Debug, Default)]
pub(crate) struct RunStats {
    pub(crate) modified: usize,
    pub(crate) unchanged: usize,
    pub(crate) skipped: usize,
}

pub fn run(args: &RemoveZwArgs) -> Result<RunStats> {
    let expanded_inputs = expand_inputs(args)?;
    let mut stats = RunStats::default();

    for input in expanded_inputs {
        match input {
            InputSource::Stdin => {
                let result = clean_stdin(io::stdin(), args.keep_bom)?;
                handle_result(args, &InputSource::Stdin, &result, &mut stats)?;
            }
            InputSource::File(path) => process_file(args, path, &mut stats)?,
            InputSource::Directory(_) => {
                return Err(anyhow!(
                    "Internal error: directories should have been expanded"
                ));
            }
        }
    }

    report_summary(args, &stats);
    Ok(stats)
}

fn process_file(args: &RemoveZwArgs, path: PathBuf, stats: &mut RunStats) -> Result<()> {
    if should_skip_by_extension(&path, &args.extensions) {
        report_skip(args, &path, "extension filter", stats);
        return Ok(());
    }

    if args.extensions.is_empty() {
        match classify_file(&path)? {
            SampleKind::Text => {}
            SampleKind::Binary => {
                report_skip(args, &path, "binary file", stats);
                return Ok(());
            }
            SampleKind::UnsupportedEncoding => {
                report_skip(args, &path, "UTF-16/UTF-32 not supported", stats);
                return Ok(());
            }
        }
    }

    let bytes =
        fs::read(&path).with_context(|| format!("Failed to read file '{}'", path.display()))?;

    match clean_bytes(&bytes, args.keep_bom) {
        Some(result) => handle_result(args, &InputSource::File(path), &result, stats),
        None => {
            report_skip(args, &path, "binary file", stats);
            Ok(())
        }
    }
}

/// Report or write one cleaned input and update the tally.
fn handle_result(
    args: &RemoveZwArgs,
    input: &InputSource,
    result: &CleanResult,
    stats: &mut RunStats,
) -> Result<()> {
    let changed = result.changed();
    if changed {
        stats.modified += 1;
    } else {
        stats.unchanged += 1;
    }

    let label = input_label(input);

    if args.report_only() {
        println!("{}", dry_run_line(&label, result));
        return Ok(());
    }

    match resolve_disposition(args, input, changed) {
        Disposition::Skip => {
            if args.verbose {
                eprintln!("remove-zw: {} -> unchanged, nothing written", label);
            }
        }
        Disposition::Write(target) => {
            write_target(&target, &result.cleaned)
                .with_context(|| format!("Failed to write output for {}", label))?;
            if args.verbose {
                let change = if changed {
                    format_change_summary(result)
                } else {
                    "unchanged".to_string()
                };
                eprintln!(
                    "remove-zw: {} -> {} ({})",
                    label,
                    describe_target(&target),
                    change
                );
            }
        }
    }

    Ok(())
}

fn report_skip(args: &RemoveZwArgs, path: &Path, reason: &str, stats: &mut RunStats) {
    stats.skipped += 1;
    if args.report_only() {
        println!("{}", skip_line(path, reason));
    } else if args.verbose {
        eprintln!("remove-zw: skipping '{}' ({})", path.display(), reason);
    }
}

fn report_summary(args: &RemoveZwArgs, stats: &RunStats) {
    if args.dry_run {
        println!("{}", dry_run_summary_line(stats));
    } else if args.check {
        println!("{}", check_summary_line(stats));
    } else if args.verbose {
        eprintln!(
            "remove-zw: {} modified, {} unchanged, {} skipped.",
            stats.modified, stats.unchanged, stats.skipped
        );
    }
}

/// Dry-run report line for one processed input.
fn dry_run_line(label: &str, result: &CleanResult) -> String {
    if result.changed() {
        format!(
            "would modify: {} ({})",
            label,
            format_change_summary(result)
        )
    } else {
        format!("unchanged: {}", label)
    }
}

/// Dry-run (and verbose) report line for one skipped input.
fn skip_line(path: &Path, reason: &str) -> String {
    format!("skipped: {} ({})", path.display(), reason)
}

/// Closing dry-run summary line.
fn dry_run_summary_line(stats: &RunStats) -> String {
    format!("Dry run: {}", summary_counts(stats))
}

/// Closing check-mode summary line; it prints only when the scan completed,
/// so exits 0 and 1 always carry it and exit 2 never does.
fn check_summary_line(stats: &RunStats) -> String {
    format!("Check: {}", summary_counts(stats))
}

fn summary_counts(stats: &RunStats) -> String {
    format!(
        "{} would modify, {} unchanged, {} skipped.",
        stats.modified, stats.unchanged, stats.skipped
    )
}

/// One-line description of what a clean removed, e.g. "BOM removed, 3
/// zero-width removed". Only the non-zero parts appear.
fn format_change_summary(result: &CleanResult) -> String {
    let mut parts = Vec::new();
    if result.bom_removed {
        parts.push("BOM removed".to_string());
    }
    if result.zero_width_removed > 0 {
        parts.push(format!("{} zero-width removed", result.zero_width_removed));
    }
    if parts.is_empty() {
        "no changes".to_string()
    } else {
        parts.join(", ")
    }
}

fn input_label(input: &InputSource) -> String {
    match input {
        InputSource::Stdin => "stdin".to_string(),
        InputSource::File(path) | InputSource::Directory(path) => path.display().to_string(),
    }
}

/// Where cleaned content for one input should go.
#[derive(Debug, PartialEq, Eq)]
enum WriteTarget {
    Stdout,
    NewFile(PathBuf),
    InPlace(PathBuf),
}

#[derive(Debug, PartialEq, Eq)]
enum Disposition {
    Skip,
    Write(WriteTarget),
}

/// Decide whether and where to write one input's cleaned content.
///
/// An explicit `--output` target is always written (even when nothing
/// changed); the auto-generated sidecar and in-place rewrite are skipped when
/// the input was already clean. Under `--in-place`, stdin is a no-op that
/// still emits to stdout.
fn resolve_disposition(args: &RemoveZwArgs, input: &InputSource, changed: bool) -> Disposition {
    if args.in_place {
        return match input {
            InputSource::File(path) if changed => {
                Disposition::Write(WriteTarget::InPlace(path.clone()))
            }
            InputSource::File(_) => Disposition::Skip,
            InputSource::Stdin => Disposition::Write(WriteTarget::Stdout),
            InputSource::Directory(_) => Disposition::Skip,
        };
    }

    if let Some(output) = &args.output {
        return match output {
            OutputTarget::Stdout => Disposition::Write(WriteTarget::Stdout),
            OutputTarget::File(path) => Disposition::Write(WriteTarget::NewFile(path.clone())),
        };
    }

    match input {
        InputSource::Stdin => Disposition::Write(WriteTarget::Stdout),
        InputSource::File(path) if changed => {
            Disposition::Write(WriteTarget::NewFile(build_output_path(path)))
        }
        InputSource::File(_) | InputSource::Directory(_) => Disposition::Skip,
    }
}

fn describe_target(target: &WriteTarget) -> String {
    match target {
        WriteTarget::Stdout => "stdout".to_string(),
        WriteTarget::NewFile(path) => path.display().to_string(),
        WriteTarget::InPlace(path) => format!("{} (in place)", path.display()),
    }
}

/// The stdin arm of `run`: reads the whole stream and cleans it. Fails when
/// the stream cannot be read or its bytes are not valid UTF-8 text.
fn clean_stdin(reader: impl Read, keep_bom: bool) -> Result<CleanResult> {
    let bytes = read_stdin(reader).context("Failed to read from stdin")?;
    clean_bytes(&bytes, keep_bom).ok_or_else(|| anyhow!("stdin is not valid UTF-8 text"))
}

/// Reads a byte stream to its end. `run` hands it the real stdin.
fn read_stdin(mut reader: impl Read) -> Result<Vec<u8>> {
    let mut buffer = Vec::new();
    reader
        .read_to_end(&mut buffer)
        .context("Failed to read stdin")?;
    Ok(buffer)
}

fn write_target(target: &WriteTarget, content: &str) -> Result<()> {
    match target {
        WriteTarget::Stdout => write_stdout(content),
        WriteTarget::NewFile(path) => write_file(path, content),
        WriteTarget::InPlace(path) => write_in_place(path, content),
    }
}

fn write_stdout(content: &str) -> Result<()> {
    let mut stdout = io::stdout();
    stdout
        .write_all(content.as_bytes())
        .context("Failed to write to stdout")?;
    stdout.flush().context("Failed to flush stdout")?;
    Ok(())
}

fn write_file(path: &Path, content: &str) -> Result<()> {
    fs::write(path, content).with_context(|| format!("Failed to write file '{}'", path.display()))
}

fn write_in_place(path: &Path, content: &str) -> Result<()> {
    let temp_path = build_temp_path(path);

    let mut temp = create_temp_file(&temp_path, path)?;
    if let Err(err) = temp
        .write_all(content.as_bytes())
        .and_then(|_| temp.flush())
    {
        let _ = fs::remove_file(&temp_path);
        return Err(anyhow!(
            "Failed to write temp file '{}': {}",
            temp_path.display(),
            err
        ));
    }
    drop(temp);

    // On platforms where the temp's permissions can't be fixed at creation
    // (Windows mirrors only the readonly flag), carry them over before the
    // swap so a read-only source stays read-only.
    #[cfg(not(unix))]
    if let Ok(metadata) = fs::metadata(path) {
        let _ = fs::set_permissions(&temp_path, metadata.permissions());
    }

    // `fs::rename` replaces an existing destination on Unix and Windows 10
    // 1607+, so the original is never deleted first (no crash window).
    if let Err(err) = fs::rename(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(anyhow!(
            "Failed to replace '{}' with temp file: {}",
            path.display(),
            err
        ));
    }

    Ok(())
}

/// Create the in-place temp file, failing if one already exists so a stray or
/// colliding temp is never clobbered. On Unix the file is created with the
/// source's mode, so the cleaned content is never briefly exposed at the
/// default umask and the final file keeps the source's permissions.
#[cfg(unix)]
fn create_temp_file(temp_path: &Path, source: &Path) -> Result<fs::File> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let mode = fs::metadata(source)
        .map(|metadata| metadata.permissions().mode())
        .unwrap_or(0o600);
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(temp_path)
        .with_context(|| format!("Failed to create temp file '{}'", temp_path.display()))
}

#[cfg(not(unix))]
fn create_temp_file(temp_path: &Path, _source: &Path) -> Result<fs::File> {
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temp_path)
        .with_context(|| format!("Failed to create temp file '{}'", temp_path.display()))
}

fn build_temp_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_else(|| Cow::Borrowed("output"));
    let temp_name = format!("{}.{}.remove-zw.tmp", file_name, std::process::id());
    path.with_file_name(temp_name)
}

fn build_output_path(path: &Path) -> PathBuf {
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy())
        .unwrap_or_else(|| Cow::Borrowed("output"));

    let new_name = match path.extension().map(|ext| ext.to_string_lossy()) {
        Some(ext) if !ext.is_empty() => format!("{}.cleaned.{}", stem, ext),
        _ => format!("{}.cleaned", stem),
    };

    path.with_file_name(new_name)
}

fn strip_format_chars(input: &str) -> (Cow<'_, str>, usize) {
    let removed = FORMAT_RE.find_iter(input).count();
    if removed == 0 {
        return (Cow::Borrowed(input), 0);
    }

    let cleaned = FORMAT_RE.replace_all(input, "");
    (cleaned, removed)
}

fn expand_inputs(args: &RemoveZwArgs) -> Result<Vec<InputSource>> {
    let mut expanded = Vec::new();

    for input in &args.inputs {
        match input {
            InputSource::Stdin => expanded.push(InputSource::Stdin),
            InputSource::File(path) => expanded.push(InputSource::File(path.clone())),
            InputSource::Directory(path) => {
                let files = collect_files_in_dir(path, args.recursive, &args.extensions)?;
                for file in files {
                    expanded.push(InputSource::File(file));
                }
            }
        }
    }

    Ok(expanded)
}

fn collect_files_in_dir(
    root: &Path,
    recursive: bool,
    extensions: &[String],
) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(current) = stack.pop() {
        for entry in fs::read_dir(&current)
            .with_context(|| format!("Failed to read directory '{}'", current.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            // `file_type()` uses lstat semantics, so a symlinked directory is
            // neither recursed into nor collected.
            let file_type = entry.file_type()?;

            if file_type.is_dir() {
                if recursive {
                    stack.push(path);
                }
                continue;
            }

            if file_type.is_file() && !should_skip_by_extension(&path, extensions) {
                files.push(path);
            }
        }
    }

    Ok(files)
}

fn should_skip_by_extension(path: &Path, extensions: &[String]) -> bool {
    if extensions.is_empty() {
        return false;
    }

    let ext = match path.extension().and_then(|ext| ext.to_str()) {
        Some(ext) => ext.to_lowercase(),
        None => return true,
    };

    !extensions.iter().any(|allowed| allowed == &ext)
}

fn classify_file(path: &Path) -> Result<SampleKind> {
    let mut file = fs::File::open(path)
        .with_context(|| format!("Failed to open file '{}'", path.display()))?;
    let mut buffer = vec![0u8; SIZE_8KB];
    let read = file
        .read(&mut buffer)
        .with_context(|| format!("Failed to read file '{}'", path.display()))?;

    Ok(classify_sample(&buffer[..read]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_args() -> RemoveZwArgs {
        RemoveZwArgs {
            inputs: Vec::new(),
            output: None,
            in_place: false,
            recursive: false,
            extensions: Vec::new(),
            verbose: false,
            dry_run: false,
            check: false,
            keep_bom: false,
        }
    }

    #[test]
    fn removes_format_chars() {
        let input = "hello\u{200B}world\u{200C}";
        let (cleaned, removed) = strip_format_chars(input);
        assert_eq!(cleaned, "helloworld");
        assert_eq!(removed, 2);
    }

    #[test]
    fn leaves_non_format_chars() {
        let input = "hello world";
        let (cleaned, removed) = strip_format_chars(input);
        assert_eq!(cleaned, "hello world");
        assert_eq!(removed, 0);
    }

    #[test]
    fn builds_output_path_with_extension() {
        let path = Path::new("sample.txt");
        assert_eq!(build_output_path(path), PathBuf::from("sample.cleaned.txt"));
    }

    #[test]
    fn builds_output_path_without_extension() {
        let path = Path::new("sample");
        assert_eq!(build_output_path(path), PathBuf::from("sample.cleaned"));
    }

    #[test]
    fn detect_bom_recognizes_each_signature() {
        assert_eq!(detect_bom(&[0xEF, 0xBB, 0xBF, b'a']), Bom::Utf8);
        assert_eq!(detect_bom(&[0xFE, 0xFF, 0, b'a']), Bom::Utf16Be);
        assert_eq!(detect_bom(&[0xFF, 0xFE, b'a', 0]), Bom::Utf16Le);
        assert_eq!(detect_bom(&[0x00, 0x00, 0xFE, 0xFF]), Bom::Utf32Be);
        assert_eq!(detect_bom(&[0xFF, 0xFE, 0x00, 0x00]), Bom::Utf32Le);
        assert_eq!(detect_bom(b"plain"), Bom::None);
    }

    #[test]
    fn detect_bom_disambiguates_utf32le_from_utf16le() {
        // UTF-16 LE is a prefix of UTF-32 LE; a real char after FF FE must
        // stay UTF-16 LE, only trailing zeros make it UTF-32 LE.
        assert_eq!(detect_bom(&[0xFF, 0xFE, 0x41, 0x00]), Bom::Utf16Le);
        assert_eq!(detect_bom(&[0xFF, 0xFE, 0x00, 0x00]), Bom::Utf32Le);
    }

    #[test]
    fn detect_bom_handles_short_buffers() {
        assert_eq!(detect_bom(&[]), Bom::None);
        assert_eq!(detect_bom(&[0xEF]), Bom::None);
        assert_eq!(detect_bom(&[0xEF, 0xBB]), Bom::None);
        assert_eq!(detect_bom(&[0x00, 0x00, 0xFE]), Bom::None);
    }

    #[test]
    fn classify_sample_variants() {
        assert_eq!(classify_sample(b"plain ascii text"), SampleKind::Text);
        assert_eq!(classify_sample("hell\u{00F3}".as_bytes()), SampleKind::Text);
        assert_eq!(
            classify_sample(&[0xEF, 0xBB, 0xBF, b'h', b'i']),
            SampleKind::Text
        );
        assert_eq!(classify_sample(&[]), SampleKind::Text);
        assert_eq!(classify_sample(&[b'a', 0x00, b'b']), SampleKind::Binary);
        assert_eq!(classify_sample(&[0xFF, 0x28, 0x80]), SampleKind::Binary);
        assert_eq!(
            classify_sample(&[0xFF, 0xFE, b'h', 0x00]),
            SampleKind::UnsupportedEncoding
        );
        assert_eq!(
            classify_sample(&[0xFE, 0xFF, 0x00, b'h']),
            SampleKind::UnsupportedEncoding
        );
        assert_eq!(
            classify_sample(&[0xFF, 0xFE, 0x00, 0x00]),
            SampleKind::UnsupportedEncoding
        );
        assert_eq!(
            classify_sample(&[0x00, 0x00, 0xFE, 0xFF]),
            SampleKind::UnsupportedEncoding
        );
    }

    #[test]
    fn classify_sample_tolerates_boundary_split_multibyte() {
        // A 4-byte emoji whose bytes are cut off at the sample boundary is
        // valid text, not binary. 0xF0 0x9F 0x98 is the first three bytes of
        // U+1F600 with the last byte missing.
        assert_eq!(
            classify_sample(&[b'h', b'i', 0xF0, 0x9F, 0x98]),
            SampleKind::Text
        );
        // A genuinely invalid lead/continuation pair is binary.
        assert_eq!(
            classify_sample(&[b'h', b'i', 0xF0, 0x28]),
            SampleKind::Binary
        );
    }

    #[test]
    fn clean_bytes_counts_bom_and_zero_width_separately() {
        let mut bytes = BOM_UTF8.to_vec();
        bytes.extend_from_slice("a\u{200B}b\u{200C}".as_bytes());
        let result = clean_bytes(&bytes, false).unwrap();
        assert_eq!(result.cleaned, "ab");
        assert!(result.bom_removed);
        assert_eq!(result.zero_width_removed, 2);
        assert!(result.changed());
    }

    #[test]
    fn clean_bytes_bom_only() {
        let result = clean_bytes(&BOM_UTF8, false).unwrap();
        assert_eq!(result.cleaned, "");
        assert!(result.bom_removed);
        assert_eq!(result.zero_width_removed, 0);
        assert!(result.changed());
    }

    #[test]
    fn clean_bytes_zero_width_only() {
        let result = clean_bytes("x\u{FEFF}y".as_bytes(), false).unwrap();
        // A mid-stream U+FEFF is a zero-width char, not a leading BOM.
        assert_eq!(result.cleaned, "xy");
        assert!(!result.bom_removed);
        assert_eq!(result.zero_width_removed, 1);
    }

    #[test]
    fn clean_bytes_clean_input_reports_no_change() {
        let result = clean_bytes(b"nothing to strip", false).unwrap();
        assert!(!result.changed());
        assert!(!result.bom_removed);
        assert_eq!(result.zero_width_removed, 0);
    }

    #[test]
    fn clean_bytes_empty_input() {
        let result = clean_bytes(&[], false).unwrap();
        assert_eq!(result.cleaned, "");
        assert!(!result.changed());
    }

    #[test]
    fn clean_bytes_rejects_non_utf8() {
        assert!(clean_bytes(&[0xFF, 0x28, 0x80], false).is_none());
        assert!(clean_bytes(&[0xFF, 0x28, 0x80], true).is_none());
    }

    #[test]
    fn keep_bom_retains_bom_bytes_and_counts_only_zero_width() {
        let mut bytes = BOM_UTF8.to_vec();
        bytes.extend_from_slice("a\u{200B}b\u{200C}".as_bytes());
        let result = clean_bytes(&bytes, true).unwrap();
        assert!(result.cleaned.as_bytes().starts_with(&BOM_UTF8));
        assert_eq!(&result.cleaned.as_bytes()[BOM_UTF8.len()..], b"ab");
        assert!(!result.bom_removed);
        assert_eq!(result.zero_width_removed, 2);
        assert!(result.changed());
    }

    #[test]
    fn keep_bom_bom_only_input_is_unchanged_and_keeps_the_bytes() {
        let result = clean_bytes(&BOM_UTF8, true).unwrap();
        assert!(!result.changed());
        assert!(!result.bom_removed);
        // The kept BOM must survive in `cleaned`, so an explicit --output
        // target cannot silently drop it.
        assert_eq!(result.cleaned.as_bytes(), BOM_UTF8);
    }

    #[test]
    fn keep_bom_without_bom_matches_default_behavior() {
        let with_flag = clean_bytes("a\u{200B}b".as_bytes(), true).unwrap();
        let without_flag = clean_bytes("a\u{200B}b".as_bytes(), false).unwrap();
        assert_eq!(with_flag.cleaned, without_flag.cleaned);
        assert_eq!(
            with_flag.zero_width_removed,
            without_flag.zero_width_removed
        );
    }

    #[test]
    fn keep_bom_still_removes_mid_stream_feff() {
        let mut bytes = BOM_UTF8.to_vec();
        bytes.extend_from_slice("x\u{FEFF}y".as_bytes());
        let result = clean_bytes(&bytes, true).unwrap();
        assert!(result.cleaned.as_bytes().starts_with(&BOM_UTF8));
        assert_eq!(&result.cleaned.as_bytes()[BOM_UTF8.len()..], b"xy");
        assert_eq!(result.zero_width_removed, 1);
        assert!(!result.bom_removed);
    }

    #[test]
    fn format_change_summary_wording() {
        let bom_and_zw = CleanResult {
            cleaned: String::new(),
            bom_removed: true,
            zero_width_removed: 3,
        };
        assert_eq!(
            format_change_summary(&bom_and_zw),
            "BOM removed, 3 zero-width removed"
        );

        let zw_only = CleanResult {
            cleaned: String::new(),
            bom_removed: false,
            zero_width_removed: 2,
        };
        assert_eq!(format_change_summary(&zw_only), "2 zero-width removed");

        let bom_only = CleanResult {
            cleaned: String::new(),
            bom_removed: true,
            zero_width_removed: 0,
        };
        assert_eq!(format_change_summary(&bom_only), "BOM removed");

        let none = CleanResult {
            cleaned: String::new(),
            bom_removed: false,
            zero_width_removed: 0,
        };
        assert_eq!(format_change_summary(&none), "no changes");
    }

    #[test]
    fn disposition_explicit_output_file_always_writes() {
        // B1: an explicit --output FILE is written even when nothing changed.
        let mut args = base_args();
        args.output = Some(OutputTarget::File(PathBuf::from("out.txt")));
        let input = InputSource::File(PathBuf::from("in.txt"));
        assert_eq!(
            resolve_disposition(&args, &input, false),
            Disposition::Write(WriteTarget::NewFile(PathBuf::from("out.txt")))
        );
    }

    #[test]
    fn disposition_output_dash_writes_stdout() {
        let mut args = base_args();
        args.output = Some(OutputTarget::Stdout);
        let input = InputSource::File(PathBuf::from("in.txt"));
        assert_eq!(
            resolve_disposition(&args, &input, false),
            Disposition::Write(WriteTarget::Stdout)
        );
    }

    #[test]
    fn disposition_sidecar_skips_when_unchanged() {
        let args = base_args();
        let input = InputSource::File(PathBuf::from("in.txt"));
        assert_eq!(resolve_disposition(&args, &input, false), Disposition::Skip);
        assert_eq!(
            resolve_disposition(&args, &input, true),
            Disposition::Write(WriteTarget::NewFile(PathBuf::from("in.cleaned.txt")))
        );
    }

    #[test]
    fn disposition_in_place_stdin_goes_to_stdout() {
        // B3: --in-place is a no-op for stdin, routing to stdout.
        let mut args = base_args();
        args.in_place = true;
        assert_eq!(
            resolve_disposition(&args, &InputSource::Stdin, true),
            Disposition::Write(WriteTarget::Stdout)
        );
    }

    #[test]
    fn dry_run_report_lines_are_formatted() {
        let changed = CleanResult {
            cleaned: String::new(),
            bom_removed: true,
            zero_width_removed: 3,
        };
        assert_eq!(
            dry_run_line("docs/a.txt", &changed),
            "would modify: docs/a.txt (BOM removed, 3 zero-width removed)"
        );

        let clean = CleanResult {
            cleaned: String::new(),
            bom_removed: false,
            zero_width_removed: 0,
        };
        assert_eq!(dry_run_line("docs/b.txt", &clean), "unchanged: docs/b.txt");

        assert_eq!(
            skip_line(Path::new("docs/logo.png"), "binary file"),
            "skipped: docs/logo.png (binary file)"
        );

        let stats = RunStats {
            modified: 1,
            unchanged: 2,
            skipped: 3,
        };
        assert_eq!(
            dry_run_summary_line(&stats),
            "Dry run: 1 would modify, 2 unchanged, 3 skipped."
        );
    }

    #[test]
    fn disposition_in_place_file_skips_when_unchanged() {
        let mut args = base_args();
        args.in_place = true;
        let input = InputSource::File(PathBuf::from("in.txt"));
        assert_eq!(resolve_disposition(&args, &input, false), Disposition::Skip);
        assert_eq!(
            resolve_disposition(&args, &input, true),
            Disposition::Write(WriteTarget::InPlace(PathBuf::from("in.txt")))
        );
    }

    #[test]
    fn extension_filter_empty_never_skips() {
        assert!(!should_skip_by_extension(Path::new("note.txt"), &[]));
        assert!(!should_skip_by_extension(Path::new("Makefile"), &[]));
    }

    #[test]
    fn extension_filter_skips_extensionless_file() {
        let filter = vec!["txt".to_string()];
        assert!(should_skip_by_extension(Path::new("Makefile"), &filter));
    }

    #[test]
    fn extension_filter_matches_case_insensitively() {
        // The filter entries arrive lowercased from parse_extensions; the
        // file's extension is lowercased before comparison.
        let filter = vec!["txt".to_string()];
        assert!(!should_skip_by_extension(Path::new("NOTE.TXT"), &filter));
        assert!(!should_skip_by_extension(Path::new("note.txt"), &filter));
        assert!(should_skip_by_extension(Path::new("note.md"), &filter));
    }

    #[test]
    fn clean_stdin_cleans_valid_utf8() {
        let result = clean_stdin("a\u{200B}b\u{FEFF}c".as_bytes(), false).unwrap();
        assert_eq!(result.cleaned, "abc");
        assert_eq!(result.zero_width_removed, 2);
        assert!(!result.bom_removed);
    }

    #[test]
    fn clean_stdin_rejects_invalid_utf8() {
        let err = clean_stdin(&[0xFF, 0x28, 0x80][..], false).unwrap_err();
        assert!(err.to_string().contains("not valid UTF-8"));
    }

    // --- End-to-end tests over run(), writing to temp files ---

    use std::fs as stdfs;
    use tempfile::tempdir;

    fn dirty_bytes() -> Vec<u8> {
        let mut bytes = BOM_UTF8.to_vec();
        bytes.extend_from_slice("hello\u{200B}world".as_bytes());
        bytes
    }

    #[test]
    fn run_sidecar_writes_cleaned_file() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("note.txt");
        stdfs::write(&src, dirty_bytes()).unwrap();

        let mut args = base_args();
        args.inputs = vec![InputSource::File(src.clone())];
        run(&args).unwrap();

        let sidecar = dir.path().join("note.cleaned.txt");
        assert_eq!(stdfs::read_to_string(&sidecar).unwrap(), "helloworld");
    }

    #[test]
    fn run_sidecar_skipped_for_clean_file() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("clean.txt");
        stdfs::write(&src, b"already clean").unwrap();

        let mut args = base_args();
        args.inputs = vec![InputSource::File(src)];
        run(&args).unwrap();

        assert!(!dir.path().join("clean.cleaned.txt").exists());
    }

    #[test]
    fn run_in_place_replaces_file_including_bom() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("note.txt");
        stdfs::write(&src, dirty_bytes()).unwrap();

        let mut args = base_args();
        args.inputs = vec![InputSource::File(src.clone())];
        args.in_place = true;
        run(&args).unwrap();

        // BOM stripped (a change on its own) and zero-width removed in place.
        assert_eq!(stdfs::read(&src).unwrap(), b"helloworld");
    }

    #[test]
    fn run_in_place_leaves_clean_file_untouched() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("note.txt");
        stdfs::write(&src, b"clean content").unwrap();

        let mut args = base_args();
        args.inputs = vec![InputSource::File(src.clone())];
        args.in_place = true;
        run(&args).unwrap();

        assert_eq!(stdfs::read(&src).unwrap(), b"clean content");
        // No stray temp file left behind.
        let leftovers: Vec<_> = stdfs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("remove-zw.tmp"))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn run_in_place_directory_cleans_each_file() {
        // B2: --in-place over a directory input processes each file.
        let dir = tempdir().unwrap();
        stdfs::write(dir.path().join("a.txt"), "a\u{200B}a".as_bytes()).unwrap();
        stdfs::write(dir.path().join("b.txt"), "b\u{200C}b".as_bytes()).unwrap();

        let mut args = base_args();
        args.inputs = vec![InputSource::Directory(dir.path().to_path_buf())];
        args.in_place = true;
        run(&args).unwrap();

        assert_eq!(
            stdfs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "aa"
        );
        assert_eq!(
            stdfs::read_to_string(dir.path().join("b.txt")).unwrap(),
            "bb"
        );
    }

    #[test]
    fn run_output_file_created_for_clean_input() {
        // B1: --output FILE is written even when the input is already clean.
        let dir = tempdir().unwrap();
        let src = dir.path().join("in.txt");
        let out = dir.path().join("out.txt");
        stdfs::write(&src, b"already clean").unwrap();

        let mut args = base_args();
        args.inputs = vec![InputSource::File(src)];
        args.output = Some(OutputTarget::File(out.clone()));
        run(&args).unwrap();

        assert_eq!(stdfs::read_to_string(&out).unwrap(), "already clean");
    }

    #[test]
    fn run_dry_run_writes_nothing() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("note.txt");
        let original = dirty_bytes();
        stdfs::write(&src, &original).unwrap();

        let mut args = base_args();
        args.inputs = vec![InputSource::File(src.clone())];
        args.dry_run = true;
        run(&args).unwrap();

        // Source untouched and no sidecar created.
        assert_eq!(stdfs::read(&src).unwrap(), original);
        assert!(!dir.path().join("note.cleaned.txt").exists());
    }

    #[test]
    fn run_check_writes_nothing_and_reports_counts() {
        let dir = tempdir().unwrap();
        stdfs::write(dir.path().join("dirty.txt"), "a\u{200B}b".as_bytes()).unwrap();
        stdfs::write(dir.path().join("clean.txt"), b"clean").unwrap();
        stdfs::write(dir.path().join("blob.dat"), [0u8, 1, 2, 3, 0]).unwrap();

        let mut args = base_args();
        args.inputs = vec![InputSource::Directory(dir.path().to_path_buf())];
        args.check = true;
        let stats = run(&args).unwrap();

        assert_eq!(stats.modified, 1);
        assert_eq!(stats.unchanged, 1);
        assert_eq!(stats.skipped, 1);
        assert!(!dir.path().join("dirty.cleaned.txt").exists());
        assert_eq!(
            stdfs::read(dir.path().join("dirty.txt")).unwrap(),
            "a\u{200B}b".as_bytes()
        );
    }

    #[test]
    fn run_keep_bom_with_output_writes_the_bom_bytes() {
        // An explicit --output target is written even for a clean input; the
        // kept BOM must land in the written bytes.
        let dir = tempdir().unwrap();
        let src = dir.path().join("in.txt");
        let out = dir.path().join("out.txt");
        let mut bytes = BOM_UTF8.to_vec();
        bytes.extend_from_slice(b"content");
        stdfs::write(&src, &bytes).unwrap();

        let mut args = base_args();
        args.inputs = vec![InputSource::File(src)];
        args.output = Some(OutputTarget::File(out.clone()));
        args.keep_bom = true;
        let stats = run(&args).unwrap();

        assert_eq!(stats.unchanged, 1);
        assert_eq!(stdfs::read(&out).unwrap(), bytes);
    }

    #[test]
    fn run_keep_bom_in_place_leaves_bom_only_dirty_file_untouched() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("bom.txt");
        let mut bytes = BOM_UTF8.to_vec();
        bytes.extend_from_slice(b"text");
        stdfs::write(&src, &bytes).unwrap();

        let mut args = base_args();
        args.inputs = vec![InputSource::File(src.clone())];
        args.in_place = true;
        args.keep_bom = true;
        let stats = run(&args).unwrap();

        assert_eq!(stats.unchanged, 1);
        assert_eq!(stdfs::read(&src).unwrap(), bytes);
    }

    #[test]
    fn run_returns_stats_for_a_mixed_directory() {
        let dir = tempdir().unwrap();
        stdfs::write(dir.path().join("a.txt"), "a\u{200B}a".as_bytes()).unwrap();
        stdfs::write(dir.path().join("b.txt"), "b\u{200C}b".as_bytes()).unwrap();
        stdfs::write(dir.path().join("clean.txt"), b"clean").unwrap();

        let mut args = base_args();
        args.inputs = vec![InputSource::Directory(dir.path().to_path_buf())];
        args.in_place = true;
        let stats = run(&args).unwrap();

        assert_eq!(stats.modified, 2);
        assert_eq!(stats.unchanged, 1);
        assert_eq!(stats.skipped, 0);
    }

    #[test]
    fn check_summary_line_wording() {
        let stats = RunStats {
            modified: 1,
            unchanged: 2,
            skipped: 3,
        };
        assert_eq!(
            check_summary_line(&stats),
            "Check: 1 would modify, 2 unchanged, 3 skipped."
        );
    }

    #[test]
    fn run_skips_utf16_file_intact() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("u16.txt");
        let utf16: Vec<u8> = vec![0xFF, 0xFE, b'h', 0x00, b'i', 0x00];
        stdfs::write(&src, &utf16).unwrap();

        let mut args = base_args();
        args.inputs = vec![InputSource::File(src.clone())];
        run(&args).unwrap();

        assert_eq!(stdfs::read(&src).unwrap(), utf16);
        assert!(!dir.path().join("u16.cleaned.txt").exists());
    }

    #[test]
    fn run_skips_matching_extension_non_utf8_body() {
        // Extensions opt out of classification; a non-UTF-8 body is still
        // skipped via the clean_bytes decode failure, not written.
        let dir = tempdir().unwrap();
        let src = dir.path().join("data.bin");
        let bytes: Vec<u8> = vec![0xFF, 0x28, 0x80, 0x00];
        stdfs::write(&src, &bytes).unwrap();

        let mut args = base_args();
        args.inputs = vec![InputSource::File(src.clone())];
        args.extensions = vec!["bin".to_string()];
        run(&args).unwrap();

        assert!(!dir.path().join("data.cleaned.bin").exists());
    }

    #[test]
    fn collect_files_does_not_follow_symlinked_dirs() {
        // Regression guard for directory sweeps: a symlinked subdirectory is
        // not descended into. On platforms without symlink support the test
        // degrades to asserting only the real file is collected.
        let dir = tempdir().unwrap();
        let real = dir.path().join("real");
        stdfs::create_dir(&real).unwrap();
        stdfs::write(real.join("keep.txt"), "x\u{200B}x".as_bytes()).unwrap();

        let outside = tempdir().unwrap();
        stdfs::write(outside.path().join("outside.txt"), b"do not touch").unwrap();

        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), real.join("link")).ok();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(outside.path(), real.join("link")).ok();

        let collected = collect_files_in_dir(&real, true, &[]).unwrap();
        assert!(collected.iter().any(|p| p.ends_with("keep.txt")));
        assert!(collected.iter().all(|p| !p.ends_with("outside.txt")));
    }

    #[test]
    fn run_skips_file_with_nonmatching_extension() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("note.md");
        stdfs::write(&src, "x\u{200B}x".as_bytes()).unwrap();

        let mut args = base_args();
        args.inputs = vec![InputSource::File(src)];
        args.extensions = vec!["txt".to_string()];
        run(&args).unwrap();

        assert!(!dir.path().join("note.cleaned.md").exists());
    }

    #[test]
    fn run_dry_run_over_directory_reports_without_writing() {
        // Exercises the dry-run report + skip paths and summary over a mix of
        // dirty, clean, and binary files; nothing is written.
        let dir = tempdir().unwrap();
        stdfs::write(dir.path().join("dirty.txt"), "a\u{200B}b".as_bytes()).unwrap();
        stdfs::write(dir.path().join("clean.txt"), b"clean").unwrap();
        stdfs::write(dir.path().join("blob.dat"), [0u8, 1, 2, 3, 0]).unwrap();

        let mut args = base_args();
        args.inputs = vec![InputSource::Directory(dir.path().to_path_buf())];
        args.dry_run = true;
        args.verbose = true;
        run(&args).unwrap();

        assert!(!dir.path().join("dirty.cleaned.txt").exists());
        assert_eq!(
            stdfs::read(dir.path().join("dirty.txt")).unwrap(),
            "a\u{200B}b".as_bytes()
        );
    }

    #[test]
    fn run_verbose_write_path_is_reported() {
        // Covers the verbose "modified -> destination" reporting branch.
        let dir = tempdir().unwrap();
        let src = dir.path().join("v.txt");
        stdfs::write(&src, "a\u{200B}b".as_bytes()).unwrap();

        let mut args = base_args();
        args.inputs = vec![InputSource::File(src)];
        args.verbose = true;
        run(&args).unwrap();

        assert_eq!(
            stdfs::read_to_string(dir.path().join("v.cleaned.txt")).unwrap(),
            "ab"
        );
    }

    #[test]
    fn classify_file_reads_leading_sample_from_disk() {
        let dir = tempdir().unwrap();

        let text = dir.path().join("text.txt");
        stdfs::write(&text, "plain text content").unwrap();
        assert_eq!(classify_file(&text).unwrap(), SampleKind::Text);

        let binary = dir.path().join("blob.dat");
        stdfs::write(&binary, [b'a', 0x00, b'b', 0x01]).unwrap();
        assert_eq!(classify_file(&binary).unwrap(), SampleKind::Binary);

        let utf16 = dir.path().join("u16.txt");
        stdfs::write(&utf16, [0xFF, 0xFE, b'h', 0x00, b'i', 0x00]).unwrap();
        assert_eq!(
            classify_file(&utf16).unwrap(),
            SampleKind::UnsupportedEncoding
        );
    }

    // --- End-to-end tests over the tracked fixtures in test-files/ ---

    fn fixture_path(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("test-files")
            .join(name)
    }

    #[test]
    fn run_cleans_complex_fixture_copy_in_place() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("complex.txt");
        stdfs::copy(fixture_path("complex.txt"), &src).unwrap();

        let original = stdfs::read_to_string(fixture_path("complex.txt")).unwrap();
        assert!(FORMAT_RE.is_match(&original), "fixture lost its Cf chars");

        let mut args = base_args();
        args.inputs = vec![InputSource::File(src.clone())];
        args.in_place = true;
        run(&args).unwrap();

        let cleaned = stdfs::read_to_string(&src).unwrap();
        assert!(!FORMAT_RE.is_match(&cleaned));
        assert!(cleaned.len() < original.len());
        // The visible text survives; only the invisible characters go.
        assert!(cleaned.contains("Line one: HelloWorld"));
        assert!(cleaned.contains("Line five: End."));
    }

    #[test]
    fn run_reports_no_cf_fixture_unchanged() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("no_cf.txt");
        stdfs::copy(fixture_path("no_cf.txt"), &src).unwrap();

        let mut args = base_args();
        args.inputs = vec![InputSource::File(src.clone())];
        run(&args).unwrap();

        // Already clean: no sidecar is written and the copy is untouched.
        assert!(!dir.path().join("no_cf.cleaned.txt").exists());
        assert_eq!(
            stdfs::read(&src).unwrap(),
            stdfs::read(fixture_path("no_cf.txt")).unwrap()
        );
    }

    #[test]
    fn run_cleans_each_single_char_fixture() {
        // Every fixture embeds its format character mid-line ("A<char>B"), so
        // none of them start with a BOM.
        let names = [
            "cf_u200b.txt",
            "cf_u200c.txt",
            "cf_u200d.txt",
            "cf_u2060.txt",
            "cf_ufeff.txt",
        ];
        let dir = tempdir().unwrap();
        for name in names {
            stdfs::copy(fixture_path(name), dir.path().join(name)).unwrap();
        }

        let mut args = base_args();
        args.inputs = vec![InputSource::Directory(dir.path().to_path_buf())];
        args.in_place = true;
        run(&args).unwrap();

        for name in names {
            let original = stdfs::read_to_string(fixture_path(name)).unwrap();
            assert!(FORMAT_RE.is_match(&original), "{name} lost its Cf char");
            let cleaned = stdfs::read_to_string(dir.path().join(name)).unwrap();
            assert!(!FORMAT_RE.is_match(&cleaned), "{name} was not cleaned");
            assert!(cleaned.len() < original.len(), "{name} did not shrink");
            assert!(cleaned.contains("AB"), "{name} lost its visible text");
        }
    }
}

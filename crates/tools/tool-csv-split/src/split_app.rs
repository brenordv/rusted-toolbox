use crate::models::SplitArgs;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use common_cli::broken_pipe::{BrokenPipe, flush_out, write_out};
use common_utils::constants::{SIZE_64KB, SIZE_128KB};
use common_utils::datetime_utc_utils::DateTimeUtcUtils;
use common_utils::string_utils::{format_bytes_to_string, format_duration_to_string};
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{debug, warn};

/// How a split run ended: the whole input was processed, or the user
/// interrupted it after some parts were already written.
#[derive(Debug, PartialEq)]
pub enum RunOutcome {
    Completed,
    Interrupted,
}

/// Creates a buffered file reader with 128KB buffer for input file.
///
/// # Errors
/// Returns error if input file cannot be opened
pub fn get_input_file_reader(args: &SplitArgs) -> Result<BufReader<File>> {
    let input_file = File::open(&args.input_file).context("Failed to open input file")?;

    // Use a larger buffer size (128KB) for better performance with large files
    Ok(BufReader::with_capacity(SIZE_128KB, input_file))
}

/// Splits input file into multiple files based on line count with graceful shutdown.
///
/// Reads file line by line, creates output files with specified prefix and numbering.
/// In CSV mode, preserves headers in each output file. Provides progress feedback.
/// On interruption the line already read is still written, so no data is dropped.
///
/// # Errors
/// Returns error if file operations fail (including flushing an output part)
/// or the input is not valid UTF-8.
pub fn process_input_file(
    args: &SplitArgs,
    shutdown_signal: Arc<AtomicBool>,
) -> Result<RunOutcome> {
    process_input_with(args, shutdown_signal, |path| {
        File::create(path)
            .with_context(|| format!("Failed to create output file: [{}]", path.display()))
    })
}

/// The engine behind [`process_input_file`], generic over how a part's writer
/// is created. The seam exists for the tests: a real part-flush failure is not
/// forcible portably, and the exit-1-on-flush-failure contract must stay
/// pinned at both `close_part` call sites.
fn process_input_with<W: Write>(
    args: &SplitArgs,
    shutdown_signal: Arc<AtomicBool>,
    mut create_part: impl FnMut(&Path) -> Result<W>,
) -> Result<RunOutcome> {
    // Open the input file
    let mut reader = get_input_file_reader(args)?;

    let start_time = Utc::now();

    let feedback_interval = args.feedback_interval as u64;

    let mut current_file_number = 1;

    let mut current_line_count = 0;

    let mut current_output_writer: Option<BufWriter<W>> = None;

    let mut total_lines_processed: u64 = 0;

    let mut output_filename = String::new();

    let mut total_data_read: u64 = 0;

    let mut feedback_enabled = true;

    let mut outcome = RunOutcome::Completed;

    // Pre-allocate string buffer for line reading to avoid repeated allocations
    let mut line_buffer = String::with_capacity(1024);

    // Get the CSV header, if in CSV mode.
    let csv_header = try_get_csv_header(args, &mut reader)?;

    loop {
        // Clear the buffer and read the next line
        line_buffer.clear();

        let bytes_read = match reader.read_line(&mut line_buffer) {
            Ok(0) => break, // End of a file
            Ok(bytes) => bytes,
            Err(e) => {
                return Err(e).context("Failed to read from the input file (is it valid UTF-8?)");
            }
        };

        total_data_read += bytes_read as u64;

        // Remove trailing newline for consistent processing
        let line = if line_buffer.ends_with('\n') {
            line_buffer.pop();
            if line_buffer.ends_with('\r') {
                line_buffer.pop();
            }
            &line_buffer
        } else {
            &line_buffer
        };

        // Create a new output file if needed
        if current_line_count == 0 {
            let output_path = create_output_filename(args, current_file_number);
            output_filename = output_path
                .file_name()
                .and_then(|f| f.to_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("file_{}", current_file_number));

            let file = create_part(&output_path)?;

            // Use BufWriter with a large buffer (64KB) for better write performance
            let mut writer = BufWriter::with_capacity(SIZE_64KB, file);

            // Write CSV header if in CSV mode
            if let Some(ref header) = csv_header {
                writeln!(writer, "{}", header).context(format!(
                    "Failed to write CSV header to output file: [{}]",
                    output_path.display()
                ))?;
            }

            current_output_writer = Some(writer);
        }

        // Write line to the current output file
        if let Some(ref mut writer) = current_output_writer {
            writeln!(writer, "{}", line).context(format!(
                "Failed to write line to output file: [{}]",
                output_filename
            ))?;
        }

        current_line_count += 1;
        total_lines_processed += 1;

        // The line already read is written before honoring the shutdown, so an
        // interrupted run never drops data it has consumed.
        if shutdown_signal.load(Ordering::Relaxed) {
            print_status("\n- Saving progress and exiting gracefully...");
            outcome = RunOutcome::Interrupted;
            break;
        }

        // Check if we need to start a new file
        if current_line_count >= args.lines_per_file {
            if let Some(writer) = current_output_writer.take() {
                close_part(writer, &output_filename)?;
            }
            current_line_count = 0;
            current_file_number += 1;
        }

        // Update progress less frequently to avoid I/O overhead
        if feedback_enabled && total_lines_processed.is_multiple_of(feedback_interval) {
            let mut stdout = std::io::stdout();
            if let Err(e) = update_progress_feedback(
                &mut stdout,
                &start_time,
                current_file_number,
                current_line_count,
                total_lines_processed,
                &output_filename,
                total_data_read,
            ) {
                if e.is::<BrokenPipe>() {
                    debug!("Progress feedback disabled: stdout closed by the consumer");
                } else {
                    warn!("Progress feedback disabled: cannot write to stdout: {}", e);
                }
                feedback_enabled = false;
            }
        }
    }

    // Ensure final file is properly flushed
    if let Some(writer) = current_output_writer {
        close_part(writer, &output_filename)?;
    }

    if feedback_enabled {
        let mut stdout = std::io::stdout();
        if let Err(e) = update_progress_feedback(
            &mut stdout,
            &start_time,
            current_file_number,
            current_line_count,
            total_lines_processed,
            &output_filename,
            total_data_read,
        ) {
            if e.is::<BrokenPipe>() {
                debug!("Final progress feedback skipped: stdout closed by the consumer");
            } else {
                warn!(
                    "Final progress feedback failed: cannot write to stdout: {}",
                    e
                );
            }
        }
    }

    print_status(&format!(
        "\n\n- Elapsed time: {}",
        format_duration_to_string(start_time.get_elapsed_time())
    ));

    Ok(outcome)
}

/// Flushes and closes one output part's writer.
///
/// The flush is explicit because `BufWriter`'s `Drop` discards flush errors,
/// and an incomplete part on disk must surface as a run failure.
///
/// # Errors
/// Returns the flush error with the part filename attached.
fn close_part(mut writer: BufWriter<impl Write>, name: &str) -> Result<()> {
    writer
        .flush()
        .with_context(|| format!("Failed to flush output file: [{}]", name))
}

/// Writes one status line (message plus newline) to `output`, mapping a
/// closed pipe to the shared [`BrokenPipe`] marker.
///
/// # Errors
/// Fails with the [`BrokenPipe`] marker when the consumer closed the pipe,
/// or with the underlying I/O error for any other write or flush failure.
fn write_status(output: &mut impl Write, msg: &str) -> Result<()> {
    write_out(output, msg.as_bytes())?;
    write_out(output, b"\n")?;
    flush_out(output)
}

/// Prints one status line to stdout without ever failing the split (its
/// product is the part files): a closed consumer is a debug note, any other
/// stdout failure a warning.
fn print_status(msg: &str) {
    let mut stdout = std::io::stdout();
    if let Err(e) = write_status(&mut stdout, msg) {
        if e.is::<BrokenPipe>() {
            debug!("Status output skipped: stdout closed by the consumer");
        } else {
            warn!("Cannot write status output to stdout: {}", e);
        }
    }
}

/// Writes one progress-feedback line (lines/second, data processed, current
/// file info) to `output`, overwriting the current console line.
///
/// # Errors
/// Returns the write or flush error, with a closed pipe mapped to the shared
/// `BrokenPipe` marker, so the caller can stop further feedback and stay
/// quiet when the consumer simply went away.
fn update_progress_feedback(
    output: &mut impl Write,
    start_time: &DateTime<Utc>,
    current_file_number: i32,
    current_line_count: usize,
    total_lines_processed: u64,
    current_output_file: &str,
    total_bytes_read: u64,
) -> Result<()> {
    let elapsed = start_time.get_elapsed_time();
    let lines_per_second = total_lines_processed as f64 / elapsed.as_seconds_f64();

    let msg = format!(
        "\r[L/s:{:.2}][Total Lines:{} Data:{} Files:{}][Cur. File:{} - {}]                        ",
        lines_per_second,
        total_lines_processed,
        format_bytes_to_string(total_bytes_read),
        current_file_number,
        current_line_count,
        current_output_file
    );

    write_out(output, msg.as_bytes())?;
    flush_out(output)
}

/// Creates output file path with prefix, input name, and file number.
///
/// Uses .csv extension in CSV mode, .txt otherwise.
fn create_output_filename(args: &SplitArgs, current_file_number: i32) -> PathBuf {
    let output_dir = PathBuf::from(&args.output_dir);

    let extension = if args.csv_mode { "csv" } else { "txt" };
    let output_filename = format!(
        "{}_{}_{}.{}",
        args.prefix, args.input_filename_without_extension, current_file_number, extension
    );

    output_dir.join(output_filename)
}

/// Reads CSV header line when CSV mode is enabled.
///
/// Returns header line with newlines trimmed for consistent handling.
///
/// # Errors
/// Returns error if header line cannot be read
fn try_get_csv_header(args: &SplitArgs, reader: &mut BufReader<File>) -> Result<Option<String>> {
    if !args.csv_mode {
        return Ok(None);
    }

    let mut header_line = String::new();
    let bytes_read = reader
        .read_line(&mut header_line)
        .context("Failed to read CSV header line")?;

    if bytes_read == 0 {
        warn!("CSV mode enabled but no header line found");
        return Ok(None);
    }

    // Remove trailing newline for consistent handling
    if header_line.ends_with('\n') {
        header_line.pop();
        if header_line.ends_with('\r') {
            header_line.pop();
        }
    }

    print_status("CSV mode: Header line detected and will be repeated in each file");
    Ok(Some(header_line))
}

#[cfg(test)]
mod tests {
    use super::*;
    use common_cli::test_writers::{ClosedPipe, FailingFlush};
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

    fn args_for(
        input: &Path,
        output_dir: &Path,
        lines_per_file: usize,
        csv_mode: bool,
    ) -> SplitArgs {
        SplitArgs {
            input_file: input.to_string_lossy().to_string(),
            output_dir: output_dir.to_string_lossy().to_string(),
            input_filename_without_extension: "input".to_string(),
            lines_per_file,
            prefix: "split".to_string(),
            csv_mode,
            feedback_interval: 100,
        }
    }

    #[test]
    fn get_input_file_reader_opens_existing_file() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("input.txt");
        fs::write(&input, "a\nb\n").unwrap();
        let args = args_for(&input, dir.path(), 10, false);

        assert!(get_input_file_reader(&args).is_ok());
    }

    #[test]
    fn get_input_file_reader_errors_on_missing_file() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("nope.txt");
        let args = args_for(&input, dir.path(), 10, false);

        assert!(get_input_file_reader(&args).is_err());
    }

    #[test]
    fn process_input_file_splits_by_line_count() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("input.txt");
        fs::write(&input, "l1\nl2\nl3\nl4\nl5\n").unwrap();
        let outdir = dir.path().join("out");
        fs::create_dir_all(&outdir).unwrap();
        let args = args_for(&input, &outdir, 2, false);

        let outcome = process_input_file(&args, Arc::new(AtomicBool::new(false))).unwrap();

        assert_eq!(outcome, RunOutcome::Completed);
        assert_eq!(
            fs::read_to_string(outdir.join("split_input_1.txt")).unwrap(),
            "l1\nl2\n"
        );
        assert_eq!(
            fs::read_to_string(outdir.join("split_input_2.txt")).unwrap(),
            "l3\nl4\n"
        );
        assert_eq!(
            fs::read_to_string(outdir.join("split_input_3.txt")).unwrap(),
            "l5\n"
        );
        assert!(!outdir.join("split_input_4.txt").exists());
    }

    #[test]
    fn part_boundary_flush_failure_stops_the_run() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("input.txt");
        fs::write(&input, "l1\nl2\nl3\nl4\n").unwrap();
        let args = args_for(&input, dir.path(), 2, false);

        // Two lines per part: the first part closes, and flushes, at the
        // mid-run boundary.
        let error = process_input_with(&args, Arc::new(AtomicBool::new(false)), |_| {
            Ok(FailingFlush)
        })
        .unwrap_err();

        assert!(
            error.to_string().contains("Failed to flush output file"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn final_part_flush_failure_stops_the_run() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("input.txt");
        fs::write(&input, "l1\n").unwrap();
        let args = args_for(&input, dir.path(), 100, false);

        // One short part: the only close, and flush, is the final one after
        // the read loop.
        let error = process_input_with(&args, Arc::new(AtomicBool::new(false)), |_| {
            Ok(FailingFlush)
        })
        .unwrap_err();

        assert!(
            error.to_string().contains("Failed to flush output file"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn empty_input_in_csv_mode_completes_with_no_parts() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("input.csv");
        fs::write(&input, "").unwrap();
        let outdir = dir.path().join("out");
        fs::create_dir_all(&outdir).unwrap();
        let args = args_for(&input, &outdir, 2, true);

        let outcome = process_input_file(&args, Arc::new(AtomicBool::new(false))).unwrap();

        assert_eq!(outcome, RunOutcome::Completed);
        assert_eq!(fs::read_dir(&outdir).unwrap().count(), 0);
    }

    #[test]
    fn process_input_file_repeats_header_in_csv_mode() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("input.csv");
        fs::write(&input, "id,name\n1,a\n2,b\n3,c\n4,d\n").unwrap();
        let outdir = dir.path().join("out");
        fs::create_dir_all(&outdir).unwrap();
        let args = args_for(&input, &outdir, 2, true);

        process_input_file(&args, Arc::new(AtomicBool::new(false))).unwrap();

        assert_eq!(
            fs::read_to_string(outdir.join("split_input_1.csv")).unwrap(),
            "id,name\n1,a\n2,b\n"
        );
        assert_eq!(
            fs::read_to_string(outdir.join("split_input_2.csv")).unwrap(),
            "id,name\n3,c\n4,d\n"
        );
    }

    #[test]
    fn process_input_file_writes_crlf_body_lines_with_lf() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("input.txt");
        fs::write(&input, "l1\r\nl2\r\n").unwrap();
        let outdir = dir.path().join("out");
        fs::create_dir_all(&outdir).unwrap();
        let args = args_for(&input, &outdir, 10, false);

        process_input_file(&args, Arc::new(AtomicBool::new(false))).unwrap();

        assert_eq!(
            fs::read_to_string(outdir.join("split_input_1.txt")).unwrap(),
            "l1\nl2\n"
        );
    }

    #[test]
    fn process_input_file_interrupt_keeps_the_line_already_read() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("input.txt");
        fs::write(&input, "l1\nl2\nl3\n").unwrap();
        let outdir = dir.path().join("out");
        fs::create_dir_all(&outdir).unwrap();
        let args = args_for(&input, &outdir, 2, false);

        let outcome = process_input_file(&args, Arc::new(AtomicBool::new(true))).unwrap();

        assert_eq!(outcome, RunOutcome::Interrupted);
        assert_eq!(
            fs::read_to_string(outdir.join("split_input_1.txt")).unwrap(),
            "l1\n"
        );
    }

    #[test]
    fn process_input_file_propagates_read_errors() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("input.txt");
        // 0xFF is never valid UTF-8, so read_line fails on the second line.
        fs::write(&input, [b'l', b'1', b'\n', 0xFF, 0xFE, b'\n']).unwrap();
        let outdir = dir.path().join("out");
        fs::create_dir_all(&outdir).unwrap();
        let args = args_for(&input, &outdir, 10, false);

        let result = process_input_file(&args, Arc::new(AtomicBool::new(false)));

        assert!(result.is_err());
    }

    #[test]
    fn update_progress_feedback_writes_one_overwriting_line() {
        let start = Utc::now();
        let mut output: Vec<u8> = Vec::new();

        update_progress_feedback(&mut output, &start, 2, 5, 105, "split_input_2.txt", 1024)
            .unwrap();

        let text = String::from_utf8(output).unwrap();
        assert!(text.starts_with("\r[L/s:"));
        assert!(text.contains("Total Lines:105"));
        assert!(text.contains("Files:2"));
        assert!(text.contains("split_input_2.txt"));
    }

    #[test]
    fn close_part_propagates_flush_failures_naming_the_part() {
        let writer = BufWriter::new(FailingFlush);

        let error = close_part(writer, "split_input_7.txt").unwrap_err();

        assert!(format!("{:#}", error).contains("split_input_7.txt"));
    }

    #[test]
    fn close_part_succeeds_on_a_healthy_writer() {
        let writer = BufWriter::new(Vec::new());

        assert!(close_part(writer, "split_input_1.txt").is_ok());
    }

    #[test]
    fn write_status_writes_the_message_with_a_trailing_newline() {
        let mut out: Vec<u8> = Vec::new();

        write_status(&mut out, "\n- Saving progress").unwrap();

        assert_eq!(out, b"\n- Saving progress\n");
    }

    #[test]
    fn write_status_maps_a_closed_pipe_to_the_marker() {
        let error = write_status(&mut ClosedPipe, "done").unwrap_err();

        assert!(error.is::<BrokenPipe>());
    }

    #[test]
    fn write_status_keeps_other_errors_ordinary() {
        let error = write_status(&mut FailingFlush, "done").unwrap_err();

        assert!(!error.is::<BrokenPipe>());
    }

    #[test]
    fn create_output_filename_uses_txt_extension_by_default() {
        let dir = tempdir().unwrap();
        let args = args_for(Path::new("input.txt"), dir.path(), 2, false);

        let path = create_output_filename(&args, 3);

        assert_eq!(
            path.file_name().unwrap().to_string_lossy(),
            "split_input_3.txt"
        );
    }

    #[test]
    fn create_output_filename_uses_csv_extension_in_csv_mode() {
        let dir = tempdir().unwrap();
        let args = args_for(Path::new("input.csv"), dir.path(), 2, true);

        let path = create_output_filename(&args, 1);

        assert_eq!(
            path.file_name().unwrap().to_string_lossy(),
            "split_input_1.csv"
        );
    }

    #[test]
    fn try_get_csv_header_returns_none_when_not_csv_mode() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("input.txt");
        fs::write(&input, "id,name\n").unwrap();
        let args = args_for(&input, dir.path(), 2, false);
        let mut reader = get_input_file_reader(&args).unwrap();

        let header = try_get_csv_header(&args, &mut reader).unwrap();

        assert!(header.is_none());
    }

    #[test]
    fn try_get_csv_header_reads_first_line_in_csv_mode() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("input.csv");
        fs::write(&input, "id,name\r\n1,a\n").unwrap();
        let args = args_for(&input, dir.path(), 2, true);
        let mut reader = get_input_file_reader(&args).unwrap();

        let header = try_get_csv_header(&args, &mut reader).unwrap();

        assert_eq!(header, Some("id,name".to_string()));
    }
}

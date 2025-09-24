use crate::models::SplitArgs;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use log::error;
use shared::constants::general::{SIZE_128KB, SIZE_64KB};
use shared::utils::datetime_utc_utils::DateTimeUtcUtils;
use shared::utils::format_bytes_to_string::format_bytes_to_string;
use shared::utils::format_duration_to_string::format_duration_to_string;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

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
///
/// # Errors
/// Returns error if file operations fail
pub fn process_input_file(args: &SplitArgs, shutdown_signal: Arc<AtomicBool>) -> Result<()> {
    // Open the input file
    let mut reader = get_input_file_reader(args)?;

    let start_time = Utc::now();

    let feedback_interval = args.feedback_interval as f64;

    let mut current_file_number = 1;

    let mut current_line_count = 0;

    let mut current_output_writer: Option<BufWriter<File>> = None;

    let mut total_lines_processed: f64 = 0.0;

    let mut output_filename = String::new();

    let mut total_data_read: u64 = 0;

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
                error!("Error reading line: {}", e);
                break;
            }
        };

        total_data_read += bytes_read as u64;

        if shutdown_signal.load(Ordering::Relaxed) {
            println!("\n- Saving progress and exiting gracefully...");
            break;
        }

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

            let file = File::create(&output_path).context(format!(
                "Failed to create output file: [{}]",
                &output_path.display()
            ))?;

            // Use BufWriter with a large buffer (64KB) for better write performance
            let mut writer = BufWriter::with_capacity(SIZE_64KB, file);

            // Write CSV header if in CSV mode
            if let Some(ref header) = csv_header {
                writeln!(writer, "{}", header).context(format!(
                    "Failed to write CSV header to output file: [{}]",
                    &output_path.display()
                ))?;
            }

            current_output_writer = Some(writer);
        }

        // Write line to the current output file
        if let Some(ref mut writer) = current_output_writer {
            writeln!(writer, "{}", line).context(format!(
                "Failed to write line to output file: [{}]",
                &output_filename
            ))?;
        }

        current_line_count += 1;
        total_lines_processed += 1.0;

        // Check if we need to start a new file
        if current_line_count >= args.lines_per_file {
            // Flush and close the current file
            if let Some(mut writer) = current_output_writer.take() {
                if let Err(e) = writer.flush() {
                    eprintln!(
                        "Warning: Failed to flush output file {}: {}",
                        output_filename, e
                    );
                }
            }
            current_line_count = 0;
            current_file_number += 1;
        }

        // Update progress less frequently to avoid I/O overhead
        if total_lines_processed % feedback_interval == 0.0 {
            update_progress_feedback(
                &start_time,
                current_file_number,
                current_line_count,
                total_lines_processed,
                &output_filename,
                &total_data_read,
            );
        }
    }

    // Ensure final file is properly flushed
    if let Some(mut writer) = current_output_writer {
        if let Err(e) = writer.flush() {
            eprintln!("Warning: Failed to flush final output file: {}", e);
        }
    }

    update_progress_feedback(
        &start_time,
        current_file_number,
        current_line_count,
        total_lines_processed,
        &output_filename,
        &total_data_read,
    );

    println!();
    println!(
        "\n- Elapsed time: {}",
        format_duration_to_string(start_time.get_elapsed_time())
    );

    Ok(())
}

/// Displays progress feedback with lines/second, data processed, and current file info.
///
/// Overwrites console line with real-time progress information.
///
/// # Panics
/// Panics if stdout flush fails
fn update_progress_feedback(
    start_time: &DateTime<Utc>,
    current_file_number: i32,
    current_line_count: usize,
    total_lines_processed: f64,
    current_output_file: &str,
    total_bytes_read: &u64,
) {
    let elapsed = start_time.get_elapsed_time();
    let lines_per_second = total_lines_processed / elapsed.as_seconds_f64();

    let msg = format!(
        "[L/s:{:.2}][Total Lines:{:.0} Data:{} Files:{}][Cur. File:{} - {}]                        ",
        lines_per_second,
        total_lines_processed,
        format_bytes_to_string(total_bytes_read),
        current_file_number,
        current_line_count,
        current_output_file
    );

    print!("\r{}", msg);

    std::io::stdout().flush().expect("Failed to flush stdout");
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
        error!("Warning: CSV mode enabled but no header line found");
        return Ok(None);
    }

    // Remove trailing newline for consistent handling
    if header_line.ends_with('\n') {
        header_line.pop();
        if header_line.ends_with('\r') {
            header_line.pop();
        }
    }

    println!("CSV mode: Header line detected and will be repeated in each file");
    Ok(Some(header_line))
}

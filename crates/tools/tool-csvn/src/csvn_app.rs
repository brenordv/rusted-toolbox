use crate::models::CsvNConfig;
use anyhow::{Context, Result};
use chrono::{DateTime, TimeDelta, Utc};
use common_utils::constants::SIZE_128KB;
use common_utils::datetime_utc_utils::DateTimeUtcUtils;
use common_utils::string_utils::format_duration_to_string;
use common_utils_ext::sanitize_str_regex::clean_str_regex;
use csv::{Reader, ReaderBuilder, StringRecord, Writer, WriterBuilder};
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::warn;

/// Progress feedback is printed at most this often, regardless of the row interval.
const FEEDBACK_MIN_INTERVAL: Duration = Duration::from_millis(250);

/// Determines the headers for CSV processing.
///
/// Uses CLI headers if provided, otherwise reads them from the file's first row.
///
/// # Errors
/// Returns an error if headers cannot be read from the file.
fn resolve_headers(config: &CsvNConfig, reader: &mut Reader<File>) -> Result<Vec<String>> {
    match &config.headers {
        Some(headers) => Ok(headers.clone()),
        None => {
            let headers = reader
                .headers()
                .context("Failed to read headers from the CSV")?
                .iter()
                .map(|field| field.trim().to_string())
                .collect();
            Ok(headers)
        }
    }
}

/// Precomputes the default value for every column, indexed by header position.
///
/// A column-specific default takes precedence over the wildcard (`*`) default;
/// columns without either are left as `None`.
fn build_column_defaults(
    value_map: &HashMap<String, String>,
    headers: &[String],
) -> Vec<Option<String>> {
    let wildcard = value_map.get("*");

    headers
        .iter()
        .map(|header| value_map.get(&header.to_lowercase()).or(wildcard).cloned())
        .collect()
}

/// Creates a buffered CSV writer for the normalized output file.
///
/// The output name is the input name with a `_normalized` suffix inserted before
/// the extension (or appended when there is none), keeping the parent directory intact.
///
/// # Errors
/// Returns an error if the input path has no file name or the output cannot be created.
pub fn get_output_normalized_file(input_file: &Path) -> Result<Writer<File>> {
    let mut output_name: OsString = input_file
        .file_stem()
        .context("Input file path has no file name")?
        .to_os_string();
    output_name.push("_normalized");
    if let Some(extension) = input_file.extension() {
        output_name.push(".");
        output_name.push(extension);
    }

    let normalized_path = input_file.with_file_name(output_name);

    let file = File::create(&normalized_path).with_context(|| {
        format!(
            "Unable to open file for writing: {}",
            normalized_path.display()
        )
    })?;

    let writer = WriterBuilder::new()
        .buffer_capacity(SIZE_128KB)
        .from_writer(file);

    Ok(writer)
}

/// Processes CSV file normalization with graceful shutdown support.
///
/// Fills empty fields with defaults, repairs rows whose column count differs from the
/// header (padding short rows, truncating long ones), and reports repaired and skipped counts.
///
/// Returns `true` when the run was interrupted by a shutdown signal.
///
/// # Errors
/// Returns an error if file operations fail.
pub fn process_file(config: &CsvNConfig, shutdown_signal: Arc<AtomicBool>) -> Result<bool> {
    let mut reader = ReaderBuilder::new()
        .flexible(true)
        .has_headers(config.headers.is_none())
        .buffer_capacity(SIZE_128KB)
        .from_path(&config.input_file)
        .with_context(|| format!("Unable to open input file: {}", config.input_file.display()))?;

    let headers = resolve_headers(config, &mut reader)?;
    let column_defaults = build_column_defaults(&config.default_value_map, &headers);

    let mut output_file = get_output_normalized_file(&config.input_file)?;
    output_file
        .write_record(&headers)
        .context("Failed to write headers to output file")?;

    let stats = run_normalization(
        &mut reader,
        &mut output_file,
        &headers,
        &column_defaults,
        config,
        &shutdown_signal,
    )?;

    output_file.flush().context("Failed to flush output file")?;

    report_summary(&stats);

    Ok(stats.interrupted)
}

/// Row counters accumulated by the normalization loop.
#[derive(Debug)]
struct NormalizationStats {
    line_count: usize,
    repaired_rows: usize,
    skipped_rows: usize,
    interrupted: bool,
}

/// Runs the normalization loop over an already-open reader.
///
/// Parse-class errors (for example invalid UTF-8) skip the offending row and the run
/// carries on; an I/O-class read error aborts the run, since the reader cannot advance
/// past it and retrying would loop forever. `interrupted` is set when the shutdown
/// signal was observed mid-run; the rows written so far stay in the output either way.
///
/// # Errors
/// Returns an error when reading fails with an I/O-class error or a normalized row
/// cannot be written.
fn run_normalization<R: Read, W: Write>(
    reader: &mut Reader<R>,
    output_file: &mut Writer<W>,
    headers: &[String],
    column_defaults: &[Option<String>],
    config: &CsvNConfig,
    shutdown_signal: &AtomicBool,
) -> Result<NormalizationStats> {
    let start_time = Utc::now();
    let mut stats = NormalizationStats {
        line_count: 0,
        repaired_rows: 0,
        skipped_rows: 0,
        interrupted: false,
    };
    let mut warned_columns = vec![false; headers.len()];

    let mut record = StringRecord::new();
    let mut output_record = StringRecord::new();
    let mut last_feedback = Instant::now();

    loop {
        if shutdown_signal.load(Ordering::Relaxed) {
            stats.interrupted = true;
            println!("\n- Saving progress and exiting gracefully...");
            println!("- Processed [{}] lines before shutdown", stats.line_count);
            break;
        }

        match reader.read_record(&mut record) {
            Ok(true) => {}
            Ok(false) => break,
            Err(e) if e.is_io_error() => {
                // Terminate the in-place progress line so the error is not
                // appended to it on the console.
                println!();
                return Err(e).with_context(|| {
                    format!(
                        "Failed to read the input CSV {} after {} rows",
                        config.input_file.display(),
                        stats.line_count
                    )
                });
            }
            Err(e) => {
                stats.skipped_rows += 1;
                warn!("Skipping unparseable row: {}", e);
                continue;
            }
        }

        if record.len() != headers.len() {
            stats.repaired_rows += 1;
        }

        normalize_into(
            column_defaults,
            headers,
            &record,
            config.clean_string,
            &mut output_record,
            &mut warned_columns,
        );

        output_file
            .write_record(&output_record)
            .context("Failed to write normalized line to output file")?;

        stats.line_count += 1;
        if stats.line_count.is_multiple_of(config.feedback_interval)
            && last_feedback.elapsed() >= FEEDBACK_MIN_INTERVAL
        {
            update_process_feedback(start_time, stats.line_count)?;
            last_feedback = Instant::now();
        }
    }

    update_process_feedback(start_time, stats.line_count)?;
    println!();

    Ok(stats)
}

/// Writes the completion summary, including any repaired or skipped row counts.
fn report_summary(stats: &NormalizationStats) {
    if stats.interrupted {
        println!(
            "[OK] Progress saved successfully. {} lines processed.",
            stats.line_count
        );
    } else {
        println!(
            "[OK] File processing completed successfully. {} lines processed.",
            stats.line_count
        );
    }

    if stats.repaired_rows > 0 {
        println!(
            "- Repaired {} row(s) whose column count did not match the header.",
            stats.repaired_rows
        );
    }

    if stats.skipped_rows > 0 {
        println!("- Skipped {} unparseable row(s).", stats.skipped_rows);
    }
}

/// Normalizes a record into `output`, filling empty fields with column defaults.
///
/// Reads each column by position, so duplicate header names resolve to their own value.
/// Missing trailing fields (short rows) are treated as empty and filled; extra fields
/// (long rows) are dropped. Warns once per column that has an empty field but no default.
fn normalize_into(
    column_defaults: &[Option<String>],
    headers: &[String],
    record: &StringRecord,
    clean_string: bool,
    output: &mut StringRecord,
    warned_columns: &mut [bool],
) {
    output.clear();

    for (index, header) in headers.iter().enumerate() {
        let value = record.get(index).unwrap_or("").trim();

        if value.is_empty() {
            match &column_defaults[index] {
                Some(default) => output.push_field(default),
                None => {
                    if !warned_columns[index] {
                        warned_columns[index] = true;
                        warn!(
                            "No default value mapped for column [{}]. Leaving empty fields empty.",
                            header
                        );
                    }
                    output.push_field("");
                }
            }
        } else if clean_string {
            output.push_field(&clean_str_regex(value));
        } else {
            output.push_field(value);
        }
    }
}

/// Displays processing progress feedback.
///
/// Shows lines processed, elapsed time, and processing speed.
///
/// # Errors
/// Returns an error if stdout cannot be flushed.
fn update_process_feedback(start_time: DateTime<Utc>, line_count: usize) -> Result<()> {
    let elapsed = start_time.get_elapsed_time();

    // Feedback line has some padding to the right to make it look nicer.
    print!(
        "[Lines processed: {}][Elapsed Time: {}][Speed: {:.2} lines/s]                                 \r",
        line_count,
        format_duration_to_string(elapsed),
        lines_per_second(line_count, elapsed)
    );
    std::io::stdout()
        .flush()
        .context("Failed to flush stdout.")?;

    Ok(())
}

/// Lines-per-second rate, or `0.0` when no measurable time has elapsed.
fn lines_per_second(line_count: usize, elapsed: TimeDelta) -> f64 {
    let seconds = elapsed.as_seconds_f64();
    if seconds > 0.0 {
        line_count as f64 / seconds
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn run(config: &CsvNConfig) -> bool {
        let signal = Arc::new(AtomicBool::new(false));
        process_file(config, signal).unwrap()
    }

    fn config_for(
        input: PathBuf,
        headers: Option<Vec<String>>,
        clean_string: bool,
        map: HashMap<String, String>,
    ) -> CsvNConfig {
        CsvNConfig::new(input, headers, clean_string, map, 100)
    }

    #[test]
    fn build_column_defaults_wildcard_and_specific_combine() {
        let map = HashMap::from([
            ("*".to_string(), "N/A".to_string()),
            ("city".to_string(), "London".to_string()),
        ]);
        let headers = vec!["Name".to_string(), "City".to_string(), "Age".to_string()];

        let defaults = build_column_defaults(&map, &headers);

        assert_eq!(defaults[0].as_deref(), Some("N/A"));
        assert_eq!(defaults[1].as_deref(), Some("London"));
        assert_eq!(defaults[2].as_deref(), Some("N/A"));
    }

    #[test]
    fn build_column_defaults_leaves_unmapped_columns_none() {
        let map = HashMap::from([("name".to_string(), "unknown".to_string())]);
        let headers = vec!["Name".to_string(), "City".to_string()];

        let defaults = build_column_defaults(&map, &headers);

        assert_eq!(defaults[0].as_deref(), Some("unknown"));
        assert_eq!(defaults[1], None);
    }

    #[test]
    fn normalize_into_fills_empty_and_trims_and_cleans() {
        let defaults = vec![Some("Unknown".to_string()), None];
        let headers = vec!["Name".to_string(), "Notes".to_string()];
        let record = StringRecord::from(vec!["", "  Good\u{0}customer  "]);
        let mut output = StringRecord::new();
        let mut warned = vec![false; headers.len()];

        normalize_into(&defaults, &headers, &record, true, &mut output, &mut warned);

        assert_eq!(output.get(0), Some("Unknown"));
        assert_eq!(output.get(1), Some("Goodcustomer"));
    }

    #[test]
    fn normalize_into_reads_duplicate_headers_by_position() {
        let defaults = vec![None, None];
        let headers = vec!["id".to_string(), "id".to_string()];
        let record = StringRecord::from(vec!["first", "second"]);
        let mut output = StringRecord::new();
        let mut warned = vec![false; headers.len()];

        normalize_into(
            &defaults,
            &headers,
            &record,
            false,
            &mut output,
            &mut warned,
        );

        assert_eq!(output.get(0), Some("first"));
        assert_eq!(output.get(1), Some("second"));
    }

    #[test]
    fn process_file_repairs_short_and_long_rows() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("data.csv");
        fs::write(
            &input,
            "name,city,age\nJohn\nAlice,Paris,30,extra\n,Rome,40\n",
        )
        .unwrap();
        let config = config_for(
            input,
            None,
            false,
            HashMap::from([("*".to_string(), "N/A".to_string())]),
        );

        let interrupted = run(&config);

        let output = fs::read_to_string(dir.path().join("data_normalized.csv")).unwrap();
        assert!(!interrupted);
        assert_eq!(
            output,
            "name,city,age\nJohn,N/A,N/A\nAlice,Paris,30\nN/A,Rome,40\n"
        );
    }

    #[test]
    fn process_file_with_cli_headers_keeps_first_data_row() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("headerless.csv");
        fs::write(&input, "John,Paris,25\nAlice,Rome,30\n").unwrap();
        let config = config_for(
            input,
            Some(vec![
                "name".to_string(),
                "city".to_string(),
                "age".to_string(),
            ]),
            false,
            HashMap::from([("*".to_string(), "N/A".to_string())]),
        );

        run(&config);

        let output = fs::read_to_string(dir.path().join("headerless_normalized.csv")).unwrap();
        assert_eq!(output, "name,city,age\nJohn,Paris,25\nAlice,Rome,30\n");
    }

    #[test]
    fn normalize_into_warns_once_for_empty_column_without_default() {
        let defaults = vec![None];
        let headers = vec!["notes".to_string()];
        let record = StringRecord::from(vec![""]);
        let mut output = StringRecord::new();
        let mut warned = vec![false];

        normalize_into(
            &defaults,
            &headers,
            &record,
            false,
            &mut output,
            &mut warned,
        );

        assert_eq!(output.get(0), Some(""));
        assert_eq!(warned, vec![true]);

        normalize_into(
            &defaults,
            &headers,
            &record,
            false,
            &mut output,
            &mut warned,
        );

        assert_eq!(output.get(0), Some(""));
        assert_eq!(warned, vec![true]);
    }

    #[test]
    fn process_file_skips_unparseable_rows_and_completes() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("data.csv");
        // The second data row holds invalid UTF-8, which makes read_record error;
        // the row is skipped and the run carries on with the next one.
        fs::write(&input, b"name,city\nJohn,Paris\nBad,\xFF\xFE\nAlice,Rome\n").unwrap();
        let config = config_for(
            input,
            None,
            false,
            HashMap::from([("*".to_string(), "N/A".to_string())]),
        );

        let interrupted = run(&config);

        let output = fs::read_to_string(dir.path().join("data_normalized.csv")).unwrap();
        assert!(!interrupted);
        assert_eq!(output, "name,city\nJohn,Paris\nAlice,Rome\n");
    }

    /// A reader whose every read fails, modeling a dropped network share mid-run.
    struct FailingReader;

    impl Read for FailingReader {
        fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("simulated read failure"))
        }
    }

    #[test]
    fn run_normalization_aborts_on_persistent_io_error() {
        let headers = vec!["name".to_string(), "city".to_string()];
        let defaults = build_column_defaults(&HashMap::new(), &headers);
        let config = config_for(PathBuf::from("in.csv"), None, false, HashMap::new());
        let mut reader = ReaderBuilder::new()
            .flexible(true)
            .has_headers(false)
            .from_reader(FailingReader);
        let mut output = WriterBuilder::new().from_writer(Vec::new());
        let signal = AtomicBool::new(false);

        let error = run_normalization(
            &mut reader,
            &mut output,
            &headers,
            &defaults,
            &config,
            &signal,
        )
        .unwrap_err();

        let chain = format!("{:#}", error);
        assert!(chain.contains("Failed to read the input CSV"));
        assert!(chain.contains("after 0 rows"));
        assert!(chain.contains("simulated read failure"));
    }

    #[test]
    fn lines_per_second_is_zero_when_no_time_elapsed() {
        assert_eq!(lines_per_second(0, TimeDelta::zero()), 0.0);
        assert_eq!(lines_per_second(500, TimeDelta::zero()), 0.0);
    }

    #[test]
    fn lines_per_second_divides_by_elapsed_seconds() {
        assert_eq!(lines_per_second(100, TimeDelta::seconds(2)), 50.0);
    }

    #[test]
    fn get_output_normalized_file_inserts_suffix_before_extension() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("data.csv");

        let _writer = get_output_normalized_file(&input).unwrap();

        assert!(dir.path().join("data_normalized.csv").exists());
    }

    #[test]
    fn get_output_normalized_file_handles_extensionless_file_in_dotted_dir() {
        let dir = tempdir().unwrap();
        let dotted = dir.path().join("archive.v2");
        fs::create_dir(&dotted).unwrap();
        let input = dotted.join("data");

        let _writer = get_output_normalized_file(&input).unwrap();

        assert!(dotted.join("data_normalized").exists());
    }
}

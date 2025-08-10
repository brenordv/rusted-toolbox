use crate::tools::csvn::models::CsvNConfig;
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use csv::{StringRecord, Writer, WriterBuilder};
use shared::system::mmap_csv_reader::MmapCsvReader;
use shared::utils::datetime_utc_utils::DateTimeUtcUtils;
use shared::utils::format_duration_to_string::format_duration_to_string;
use shared::utils::sanitize_str_regex::clean_str_regex;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use string_interner::DefaultSymbol;

/// Determines headers for CSV processing.
///
/// Uses CLI headers if provided, otherwise extracts from the file's first row.
///
/// # Errors
/// Returns error if headers cannot be read from file
pub fn ensure_headers(
    arg_headers: &Option<Vec<String>>,
    reader: &mut MmapCsvReader,
) -> Result<Vec<String>> {
    if let Some(headers) = arg_headers {
        Ok(headers.clone())
    } else {
        let headers = reader
            .headers()
            .context("Failed to read headers from the CSV ")?
            .clone()
            .iter()
            .map(|s| s.trim().to_string())
            .collect();

        Ok(headers)
    }
}

/// Creates a buffered CSV writer for normalized output.
///
/// Output file has "_normalized" suffix and 128KB buffer.
///
/// # Errors
/// Returns error if an output file cannot be created
pub fn get_output_normalized_file(input_file: &PathBuf) -> Result<Writer<File>> {
    let input_str;

    match input_file.to_str() {
        Some(s) => input_str = s,
        None => {
            return Err(anyhow!(
                "Input file path is not valid UTF-8. Non-UTF-8 paths are unsupported."
            ));
        }
    };

    let normalized_path = if let Some(dot_index) = input_str.rfind('.') {
        format!(
            "{}_normalized{}",
            &input_str[..dot_index],
            &input_str[dot_index..]
        )
    } else {
        format!("{}_normalized", input_str)
    };

    let file = File::create(&normalized_path).context(format!(
        "Unable to open file for writing: {}",
        normalized_path
    ))?;

    let wtr = WriterBuilder::new()
        .buffer_capacity(131_072) // 128 KiB internal buffer
        .from_writer(file);

    Ok(wtr)
}

/// Processes CSV file normalization with graceful shutdown support.
///
/// Creates a normalized output file, fills empty fields with defaults, provides progress updates.
/// Silently skips malformed CSV lines for performance.
///
/// # Errors
/// Returns error if file operations fail
pub fn process_file(args: &mut CsvNConfig, shutdown_signal: Arc<AtomicBool>) -> Result<()> {
    let mut reader = MmapCsvReader::new(&args.input_file)?;

    let headers = ensure_headers(&args.headers, &mut reader)?;

    let value_map = update_default_value_map(args, &headers)?;

    let mut output_file = get_output_normalized_file(&args.input_file)?;

    // Write headers first
    output_file
        .write_record(&headers)
        .context("Failed to write headers to output file")?;

    let start_time = Utc::now();

    let mut line_count: u64 = 0;

    let feedback_interval = args.feedback_interval;

    for record in reader.records().filter_map(Result::ok) {
        // Check for a shutdown signal
        if shutdown_signal.load(Ordering::Relaxed) {
            println!("\n💾 Saving progress and exiting gracefully...");
            println!("📊 Processed [{}] lines before shutdown", line_count);
            break;
        }

        let normalized_record =
            normalize_record(&args, &value_map, &headers, record, &args.clean_string)?;

        output_file
            .write_record(&normalized_record)
            .context("Failed to write normalized line to output file")?;

        line_count += 1;
        if line_count % feedback_interval == 0 {
            update_process_feedback(start_time, &line_count)?;
        }
    }

    update_process_feedback(start_time, &line_count)?;
    println!();

    // Ensure all data is written to the disk
    match output_file.flush() {
        Ok(_) => {
            if shutdown_signal.load(Ordering::Relaxed) {
                println!(
                    "✅ Progress saved successfully. {} lines processed.",
                    line_count
                );
            } else {
                println!(
                    "✅ File processing completed successfully. {} lines processed.",
                    line_count
                );
            }
            Ok(())
        }
        Err(e) => {
            eprintln!("❌ Failed to flush output file: {}", e);
            Err(anyhow!(e))
        }
    }
}

/// Updates a default value map based on file headers.
///
/// Expands wildcard (*) keys to all headers or uses specific column mappings.
///
/// # Errors
/// Returns error if a wildcard key exists without value
fn update_default_value_map(
    config: &mut CsvNConfig,
    file_headers: &Vec<String>,
) -> Result<HashMap<String, DefaultSymbol>> {
    let mut interned_map = HashMap::new();

    if config.default_value_map.is_empty()
        || (config.default_value_map.len() == 1 && config.default_value_map.contains_key("*"))
    {
        // Handle wildcard case
        let default_value = config
            .default_value_map
            .get("*")
            .context("When using a wildcard key, a value should be provided!")?;

        // Intern the wildcard value once
        let interned_symbol = config.string_interner.get_or_intern(default_value);

        // Apply to all headers
        for header in file_headers {
            interned_map.insert(header.to_lowercase(), interned_symbol);
        }
    } else {
        // Use pre-interned values from config
        for (key, symbol) in &config.interned_defaults {
            interned_map.insert(key.clone(), *symbol);
        }
    }

    Ok(interned_map)
}

/// Normalizes CSV record by filling empty fields with default values.
///
/// Trims whitespace and replaces empty fields with defaults.
/// Warns if no default value is found for the header.
fn normalize_record(
    config: &CsvNConfig,
    default_map: &HashMap<String, DefaultSymbol>,
    headers: &Vec<String>,
    record: StringRecord,
    clean_string: &bool,
) -> Result<StringRecord> {
    let mut normalized_record = StringRecord::new();

    for header in headers {
        let header_value = headers
            .iter()
            .position(|h| h == header)
            .context("Failed to find header in headers list")?;

        let value = record
            .get(header_value)
            .context(format!("Failed to get value [{}] for header", header_value))?
            .trim();

        // Use default if the value is empty
        let final_value = if value.is_empty() {
            match default_map.get(header.to_lowercase().as_str()) {
                Some(symbol) => {
                    // Resolve the interned string back to &str
                    config.string_interner.resolve(*symbol).unwrap_or("")
                }
                None => {
                    eprintln!(
                        "Could not find default value mapped for key [{}]. Keeping value empty.",
                        header
                    );
                    ""
                }
            }
        } else if *clean_string {
            &clean_str_regex(&value)
        } else {
            value
        };

        normalized_record.push_field(final_value);
    }
    Ok(normalized_record)
}

/// Displays processing progress feedback.
///
/// Shows lines processed, elapsed time, and processing speed.
///
/// # Errors
/// Returns error if stdout cannot be flushed
fn update_process_feedback(start_time: DateTime<Utc>, line_count: &u64) -> Result<()> {
    let elapsed = start_time.get_elapsed_time();
    let lines_per_second = *line_count as f64 / elapsed.as_seconds_f64();

    // Feedback line has some padding to the right to make it look nicer.
    print!(
        "[Lines processed: {}][Elapsed Time: {}][Speed: {:.2} lines/s]                                 \r",
        line_count,
        format_duration_to_string(elapsed),
        lines_per_second
    );
    std::io::stdout()
        .flush()
        .context("Failed to flush stdout.")?;

    Ok(())
}

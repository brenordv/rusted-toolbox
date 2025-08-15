use anyhow::{Context, Result};
use futures_util::StreamExt;
use shared::utils::sanitize_string_for_filename::sanitize_string_for_filename;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use tokio_stream::wrappers::ReceiverStream;
use crate::models::{GetLinesArgs, LineData};

/// Sets up file-based output channels for search term results.
///
/// Creates separate output files for each search term and spawns async writer tasks
/// to handle concurrent writing operations.
///
/// # Arguments
/// - `args` - Configuration containing worker thread count
/// - `output_channels` - Map to store sender channels for each search term
/// - `output_handles` - Vector to track async writer task handles
/// - `output_dir` - Directory path for output files
/// - `term` - Search term for which to create output file
/// - `shutdown_signal` - Signal for graceful task termination
///
/// # Returns
/// - `Ok(())` on successful setup
/// - `Err(Box<dyn Error>)` if file creation or I/O operations fail
///
/// # Behavior
/// - Generates sanitized filename based on search term
/// - Creates a buffered writer wrapped in thread-safe mutex
/// - Spawns an async task to continuously write received lines
/// - Handles proper flushing on task completion or shutdown
pub fn prepare_to_export_search_terms_to_output_files(
    args: &GetLinesArgs,
    output_channels: &mut HashMap<String, Sender<String>>,
    output_handles: &mut Vec<JoinHandle<()>>,
    output_dir: &String,
    term: &String,
    shutdown_signal: Arc<AtomicBool>,
) -> Result<()> {
    let output_file_path =
        Path::new(output_dir).join(format!("{}.txt", sanitize_string_for_filename(term)));

    let output_file = File::create(&output_file_path).context(format!(
        "Failed to create output file: {}",
        output_file_path.display()
    ))?;

    let writer = Arc::new(Mutex::new(BufWriter::new(output_file)));

    let (tx, mut rx) = mpsc::channel::<String>(args.workers * 2);

    output_channels.insert(term.clone(), tx);

    let writer_clone = Arc::clone(&writer);

    let handle = tokio::spawn(async move {
        while let Some(line) = rx.recv().await {
            if shutdown_signal.load(Ordering::Relaxed) {
                break;
            }

            let mut w = writer_clone.lock().await;

            let _ = w.write_all(line.as_bytes());
        }

        let mut w = writer_clone.lock().await;

        let _ = w.flush();
    });

    output_handles.push(handle);

    Ok(())
}

/// Sets up console-based output channels for search term results.
///
/// Creates communication channels and spawns async tasks to print matching lines
/// to the console for each search term.
///
/// # Arguments
/// - `args` - Configuration containing worker thread count
/// - `output_channels` - Map to store sender channels for each search term
/// - `output_handles` - Vector to track async writer task handles
/// - `search_terms` - List of search terms to create console outputs for
/// - `shutdown_signal` - Signal for graceful task termination
///
/// # Behavior
/// - Creates MPSC channel for each search term with worker-based capacity
/// - Spawns an async task per term to consume and print received lines
/// - Tasks check shutdown signal periodically for graceful termination
pub fn prepare_to_export_search_terms_to_console(
    args: &GetLinesArgs,
    output_channels: &mut HashMap<String, Sender<String>>,
    output_handles: &mut Vec<JoinHandle<()>>,
    search_terms: &Vec<String>,
    shutdown_signal: Arc<AtomicBool>,
) {
    for term in search_terms {
        let (tx, mut rx) = mpsc::channel::<String>(args.workers * 2);
        output_channels.insert(term.clone(), tx);

        let shutdown_clone = Arc::clone(&shutdown_signal);
        let handle = tokio::spawn(async move {
            while let Some(line) = rx.recv().await {
                if shutdown_clone.load(Ordering::Relaxed) {
                    break;
                }
                print!("{}", line);
            }
        });
        output_handles.push(handle);
    }
}

/// Spawns async worker to read file lines and send to a processing channel.
///
/// Opens the input file and reads it line by line, sending each line with its
/// line number to the provided channel for downstream processing.
///
/// # Arguments
/// - `args` - Configuration containing file path and display options
/// - `line_tx` - Channel sender for transmitting line data
/// - `shutdown_signal` - Signal for graceful task termination
///
/// # Returns
/// Join handle for the spawned file reading task
///
/// # Behavior
/// - Opens file using buffered reader for performance
/// - Sends LineData struct containing line number and content
/// - Stops reading on shutdown signal or receiver drop
/// - Panics if input file cannot be opened
///
/// # Panics
/// Panics if the specified input file cannot be opened
pub fn spawn_file_reading_workers(
    args: &GetLinesArgs,
    line_tx: &Sender<LineData>,
    shutdown_signal: Arc<AtomicBool>,
) -> JoinHandle<()> {
    let reader_handle = {
        let file_path = args.file.clone();
        let hide_runtime_info = args.hide_runtime_info;
        let line_tx = line_tx.clone();
        tokio::spawn(async move {
            let file = File::open(&file_path).expect("Failed to open input file");
            let reader = BufReader::new(file);
            let mut line_number = 1;

            for line_res in reader.lines() {
                if shutdown_signal.load(Ordering::Relaxed) {
                    if !hide_runtime_info {
                        println!("📖 File reading stopped due to shutdown signal");
                    }
                    break;
                }

                match line_res {
                    Ok(content) => {
                        if line_tx
                            .send(LineData {
                                line_number,
                                content,
                            })
                            .await
                            .is_err()
                        {
                            break; // receiver dropped
                        }
                        line_number += 1;
                    }
                    Err(_) => break,
                }
            }
        })
    };
    reader_handle
}

/// Processes file lines by matching against search terms and routing results.
///
/// Spawns concurrent workers to process lines from input channel, perform
/// case-insensitive pattern matching, and send matches to appropriate output channels.
///
/// # Arguments
/// - `args` - Configuration containing worker count and display options
/// - `output_channels` - Map of search terms to their output channels
/// - `search_terms` - List of patterns to search for in each line
/// - `line_rx` - Channel receiver for incoming line data
/// - `shutdown_signal` - Signal for graceful task termination
///
/// # Returns
/// Join handle for the spawned line processing task
///
/// # Behavior
/// - Uses concurrent stream processing with configurable worker count
/// - Performs case-insensitive substring matching for each search term
/// - Formats output with or without line numbers based on configuration
/// - Stops processing on shutdown signal
pub fn process_lines_read(
    args: GetLinesArgs,
    output_channels: &mut HashMap<String, Sender<String>>,
    search_terms: Vec<String>,
    line_rx: Receiver<LineData>,
    shutdown_signal: Arc<AtomicBool>,
) -> JoinHandle<()> {
    let processor_handle = {
        let search_terms = search_terms.clone();
        let output_channels = output_channels.clone();
        let workers = args.workers;
        let stream = ReceiverStream::new(line_rx);

        tokio::spawn(async move {
            stream
                .for_each_concurrent(workers, |line_data| {
                    let search_terms = search_terms.clone();
                    let output_channels = output_channels.clone();
                    let shutdown_clone = Arc::clone(&shutdown_signal);
                    async move {
                        if shutdown_clone.load(Ordering::Relaxed) {
                            return;
                        }

                        let lower = line_data.content.to_lowercase();
                        for term in &search_terms {
                            if lower.contains(term) {
                                let out = if args.hide_line_numbers {
                                    format!("{}\n", line_data.content)
                                } else {
                                    format!("{}\t{}\n", line_data.line_number, line_data.content)
                                };
                                if let Some(tx) = output_channels.get(term) {
                                    let _ = tx.send(out).await;
                                }
                            }
                        }
                    }
                })
                .await;
        })
    };
    processor_handle
}
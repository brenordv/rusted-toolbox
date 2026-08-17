use crate::cli_utils::initialize;
use crate::get_lines_app::{
    prepare_to_export_search_terms_to_console, prepare_to_export_search_terms_to_output_files,
    process_lines_read, spawn_file_reading_workers,
};
use crate::models::LineData;
use cli_signal_monitor::setup_graceful_shutdown::setup_graceful_shutdown;
use common_cli::tool_exit_helpers::{exit_error, exit_success};
use std::collections::HashMap;
use std::fs::create_dir_all;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::error;

mod cli_utils;
mod get_lines_app;
mod models;

/// Main entry point for the get-lines tool.
///
/// Orchestrates the workflow of searching for text patterns in files and outputting matches
/// either to console or separate files per search term.
///
/// # Workflow
/// 1. Parses and validates command-line arguments
/// 2. Sets up graceful shutdown handling
/// 3. Prepares output channels (console or files)
/// 4. Spawns file reading workers
/// 5. Processes lines concurrently with pattern matching
/// 6. Finalizes output and displays completion status
///
/// # Returns
/// - `Ok(())` on successful completion
/// - `Err(Box<dyn Error>)` on failure during execution
///
/// # Errors
/// - Invalid command-line arguments
/// - File I/O errors during reading or writing
/// - Channel communication failures
/// - Graceful shutdown setup failures
///
#[tokio::main]
async fn main() {
    // 1) Parse & validate CLI arguments
    let args = initialize();

    // 2) Set up the graceful shutdown
    let shutdown_signal = setup_graceful_shutdown(false);

    // 3) Prepare output channels and writer tasks
    let mut output_channels: HashMap<String, mpsc::Sender<String>> = HashMap::new();
    let mut output_handles = Vec::new();
    let search_terms = args.search.clone();

    if let Some(output_dir) = &args.output {
        let _ = create_dir_all(output_dir).inspect_err(|e| {
            error!(
                "Failed to create output directory [{:?}]: [{}]",
                output_dir, e
            );
            exit_error();
        });

        for term in &search_terms {
            let _ = prepare_to_export_search_terms_to_output_files(
                &args,
                &mut output_channels,
                &mut output_handles,
                output_dir,
                term,
                Arc::clone(&shutdown_signal),
            )
            .inspect_err(|e| {
                error!(
                    "Failed to create output file for search term [{}]: [{}]",
                    term, e
                );
                exit_error();
            });
        }
    } else {
        prepare_to_export_search_terms_to_console(
            &args,
            &mut output_channels,
            &mut output_handles,
            &search_terms,
            Arc::clone(&shutdown_signal),
        );
    }

    // 4) Create an MPSC channel for line streaming
    let (line_tx, line_rx) = mpsc::channel::<LineData>(args.workers * 2);

    // 5) Spawn the file-reading task
    let reader_handle = spawn_file_reading_workers(&args, &line_tx, Arc::clone(&shutdown_signal));

    // 6) Process lines in parallel using for_each_concurrent
    let processor_handle = process_lines_read(
        args,
        &mut output_channels,
        search_terms,
        line_rx,
        Arc::clone(&shutdown_signal),
    );

    // 7) Wait for the reader, then close the sender to finish the stream
    let _ = reader_handle.await.inspect_err(|e| {
        error!("Failed to read files: [{}]", e);
        exit_error();
    });

    drop(line_tx);

    // 8) Wait for processing to complete
    let _ = processor_handle.await.inspect_err(|e| {
        error!("Failed to process lines: [{}]", e);
        exit_error();
    });

    // 9) Close output channels and await writer tasks
    for (_, tx) in output_channels {
        drop(tx);
    }
    for handle in output_handles {
        let _ = handle.await.inspect_err(|e| {
            error!("Failed to write output: [{}]", e);
            exit_error();
        });
    }

    exit_success();
}

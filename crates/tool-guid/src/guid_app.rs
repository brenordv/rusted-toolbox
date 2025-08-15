use anyhow::{Context, Result};
use shared::utils::copy_string_to_clipboard::copy_to_clipboard;
use shared::utils::new_guid::new_guid;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Continuously generates GUIDs at a specified interval until interrupted.
///
/// Generates a new GUID every interval second, handles Ctrl+C for graceful shutdown.
///
/// # Errors
/// Returns error if Ctrl+C handler setup fails
pub fn continuous_generation(interval: f64, silent: bool) -> Result<()> {
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    // Set up Ctrl+C handler
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .context("Error setting Ctrl-C handler")?;

    if !silent {
        println!("🚦 Press Ctrl+C to stop...");
    }

    let duration = Duration::from_secs_f64(interval);

    while running.load(Ordering::SeqCst) {
        let guid = new_guid();
        print!("{}\r", guid);

        // Sleep for the specified interval
        thread::sleep(duration);
    }

    println!();

    if !silent {
        println!("👋 Stopping guid generation.");
    }
    Ok(())
}

/// Generates a single GUID based on configuration.
///
/// Returns empty GUID (zeros) if requested, otherwise generates new UUID v4.
pub fn generate_once(empty_guid: bool) -> String {
    if empty_guid {
        "00000000-0000-0000-0000-000000000000".to_string()
    } else {
        new_guid()
    }
}

/// Copies GUID to the system clipboard.
///
/// Terminates the program with an error message if the clipboard operation fails.
pub fn copy_guid_to_clipboard(guid: String) {
    if let Err(e) = copy_to_clipboard(&guid) {
        eprintln!("Error copying to clipboard: {}", e);
        std::process::exit(1);
    }
}

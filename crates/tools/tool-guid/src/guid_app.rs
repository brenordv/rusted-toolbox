use anyhow::{Context, Result};
use common_utils_ext::new_guid::new_guid;
use shared::utils::copy_string_to_clipboard::copy_to_clipboard;
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
        println!("Press Ctrl+C to stop...");
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
        println!("Stopping guid generation.");
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

#[cfg(test)]
mod tests {
    use super::*;

    const EMPTY_GUID: &str = "00000000-0000-0000-0000-000000000000";

    #[test]
    fn generate_once_empty_returns_zero_guid() {
        assert_eq!(generate_once(true), EMPTY_GUID);
    }

    #[test]
    fn generate_once_produces_valid_uuid_v4_shape() {
        let guid = generate_once(false);

        assert_eq!(guid.len(), 36);
        assert_ne!(guid, EMPTY_GUID);

        let bytes = guid.as_bytes();
        assert_eq!(bytes[8], b'-');
        assert_eq!(bytes[13], b'-');
        assert_eq!(bytes[18], b'-');
        assert_eq!(bytes[23], b'-');
        // Position 14 carries the UUID version (4) and position 19 the variant.
        assert_eq!(bytes[14], b'4');
        assert!(matches!(bytes[19], b'8' | b'9' | b'a' | b'b'));
    }

    #[test]
    fn generate_once_yields_unique_values() {
        assert_ne!(generate_once(false), generate_once(false));
    }
}

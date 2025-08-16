use crate::eventhub_reader_app::EventHubReader;
use anyhow::Result;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tracing::{error, info};

/// Executes graceful shutdown with 15-second timeout.
///
/// Attempts graceful shutdown of EventHub reader, logs completion status,
/// and returns the original operation result.
///
/// # Returns
/// Original result from EventHub reader operation.
pub async fn graceful_shutdown_routine(
    reader: EventHubReader,
    result: anyhow::Result<()>,
) -> Result<()> {
    match tokio::time::timeout(
        std::time::Duration::from_secs(15),
        reader.graceful_shutdown(),
    )
    .await
    {
        Ok(_) => {
            println!("👋 EventHub reader stopped gracefully.");
        }
        Err(_) => {
            println!("⏰ Graceful shutdown timed out, forcing exit.");
        }
    }

    match result {
        Ok(_) => {
            info!("EventHub reader completed successfully");
            Ok(())
        }
        Err(e) => {
            error!("EventHub reader failed: {}", e);
            Err(e)
        }
    }
}

/// Sets up Ctrl+C signal handler for graceful shutdown.
///
/// Creates atomic shutdown signal, registers signal handler that triggers
/// EventHub reader shutdown, and links signal to reader instance.
///
/// # Errors
/// Returns error if signal handler registration fails.
pub fn setup_graceful_shutdown(reader: &mut EventHubReader) -> Result<()> {
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = Arc::clone(&shutdown);
    let reader_clone = reader.clone();

    ctrlc::set_handler(move || {
        println!("\n🛑 Shutdown signal received, stopping gracefully...");
        shutdown_clone.store(true, std::sync::atomic::Ordering::Relaxed);
        reader_clone.shutdown();
    })?;

    reader.set_shutdown_signal(Arc::clone(&shutdown));
    Ok(())
}

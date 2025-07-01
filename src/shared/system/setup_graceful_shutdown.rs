use crate::shared::system::tool_exit_helpers::exit_error;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

fn create_manual_shutdown_handler() -> Result<Arc<AtomicBool>, anyhow::Error> {
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = Arc::clone(&shutdown);

    ctrlc::set_handler(move || {
        println!("\n🛑 Shutdown signal received, stopping application...");
        shutdown_clone.store(true, std::sync::atomic::Ordering::Relaxed);
    })?;

    Ok(shutdown)
}

pub fn setup_graceful_shutdown() -> Arc<AtomicBool> {
    match create_manual_shutdown_handler() {
        Ok(signal) => signal,
        Err(e) => {
            eprintln!("Failed to setup graceful shutdown: {}", e);
            exit_error();
            unreachable!();
        }
    }
}

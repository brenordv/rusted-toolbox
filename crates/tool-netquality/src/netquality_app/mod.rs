mod checks;
mod config;
mod runtime;
mod state;

use crate::cli_utils::print_runtime_info;
use crate::models::NetQualityCliArgs;
use crate::notifiers::Notifier;
use anyhow::{Context, Result};
use shared::system::setup_graceful_shutdown::setup_graceful_shutdown;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tracing::{info, trace};
use crate::persistence::db;

pub async fn run_app(args: &NetQualityCliArgs) -> Result<()> {
    let (config, config_label) = config::load_config(args).await?;

    print_runtime_info(&config_label, &runtime::build_runtime_info(&config));

    if config.notifications.telegram.is_none() {
        info!("Telegram notifications disabled.");
    }

    if config.notifications.otel_endpoint.is_none() {
        info!("OpenTelemetry notifications disabled.");
    }

    let mut notifier = Notifier::new(&config.notifications)
        .context("Failed to initialize notification channels")?;

    let db_path = &config.storage.db_path;
    let connection = db::create_database(db_path).context("Failed to initialize SQLite database")?;

    let shutdown = setup_graceful_shutdown(false);
    let mut state = state::LoopState::new(&config);

    while !shutdown.load(Ordering::Relaxed) {
        let mut connectivity_id = None;
        let mut speed_id = None;
        let now = std::time::Instant::now();

        if now >= state.next_connectivity_at {
            let connectivity_result = checks::run_connectivity_check(&config, &mut state)
                .await
                .context("Connectivity check failed")?;

            connectivity_id = Some(
                db::insert_connectivity_activity(&connection, &connectivity_result)
                    .context("Failed to store connectivity activity")?,
            );

            state::handle_connectivity_state(&config, &mut state, &connectivity_result);
        }

        let should_run_speed = state::should_run_speed_check(&state);

        if should_run_speed {
            if state.last_connectivity_success {
                let speed_result = checks::run_speed_check(&config)
                    .await
                    .context("Speed check failed")?;

                speed_id = Some(
                    db::insert_speed_activity(&connection, &speed_result)
                        .context("Failed to store speed activity")?,
                );

                state::handle_speed_state(&config, &mut state, &mut notifier, &speed_result).await;
            } else {
                trace!("Skipping speed check because connectivity is down.");
            }
        }

        if connectivity_id.is_some() || speed_id.is_some() {
            db::insert_session(&connection, connectivity_id, speed_id)
                .context("Failed to store session")?;
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    notifier.shutdown();
    info!("NetQuality shutdown complete.");
    Ok(())
}

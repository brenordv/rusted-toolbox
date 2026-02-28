use crate::checks::connectivity_check::run_connectivity_check;
use crate::checks::database_clean_up::run_database_cleanup;
use crate::checks::speed_test_check::run_speed_check;

use crate::cli_utils::cli_utils::print_runtime_info;
use crate::cli_utils::config_parser;
use crate::cli_utils::runtime_info_builder::build_runtime_info;
use crate::models::NetQualityCliArgs;
use crate::notifiers::Notifier;
use crate::persistence::db;
use crate::runtime_state;
use anyhow::{Context, Result};
use shared::system::setup_graceful_shutdown::setup_graceful_shutdown;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tracing::{info, trace};

pub async fn run_app(args: &NetQualityCliArgs) -> Result<()> {
    let (config, config_label) = config_parser::load_config(args).await?;

    print_runtime_info(&config_label, &build_runtime_info(&config));

    if config.notifications.telegram.is_none() {
        info!("Telegram notifications disabled.");
    }

    let mut notifier = Notifier::new(&config.notifications)
        .context("Failed to initialize notification channels")?;

    let db_path = &config.storage.db_path;
    let mut connection =
        db::create_database(db_path).context("Failed to initialize SQLite database")?;

    let mut next_cleanup_at = if config.storage.cleanup_enabled {
        run_database_cleanup(&mut connection);
        Some(Instant::now() + config.storage.cleanup_interval)
    } else {
        None
    };

    let shutdown = setup_graceful_shutdown(false);
    let mut state = runtime_state::LoopState::new(&config);

    while !shutdown.load(Ordering::Relaxed) {
        let mut connectivity_id = None;
        let mut speed_id = None;
        let now = std::time::Instant::now();

        if now >= state.next_connectivity_at {
            let connectivity_result = run_connectivity_check(&config, &mut state)
                .await
                .context("Connectivity check failed")?;

            connectivity_id = Some(
                db::insert_connectivity_activity(&connection, &connectivity_result)
                    .context("Failed to store connectivity activity")?,
            );

            runtime_state::handle_connectivity_state(&config, &mut state, &connectivity_result);
        }

        let should_run_speed = runtime_state::should_run_speed_check(&state);

        if should_run_speed {
            if state.last_connectivity_success {
                let speed_result = run_speed_check(&config)
                    .await
                    .context("Speed check failed")?;

                speed_id = Some(
                    db::insert_speed_activity(&connection, &speed_result)
                        .context("Failed to store speed activity")?,
                );

                runtime_state::handle_speed_state(
                    &config,
                    &mut state,
                    &mut notifier,
                    &speed_result,
                )
                .await;
            } else {
                trace!("Skipping speed check because connectivity is down.");
            }
        }

        if connectivity_id.is_some() || speed_id.is_some() {
            db::insert_session(&connection, None, connectivity_id, speed_id)
                .context("Failed to store session")?;
        }

        if let Some(next_cleanup_due) = next_cleanup_at {
            if now >= next_cleanup_due {
                run_database_cleanup(&mut connection);
                next_cleanup_at = Some(Instant::now() + config.storage.cleanup_interval);
            }
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    notifier.shutdown();
    info!("NetQuality shutdown complete.");
    Ok(())
}

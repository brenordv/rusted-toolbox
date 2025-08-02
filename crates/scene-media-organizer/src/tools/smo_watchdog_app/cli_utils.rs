use crate::constants::{APP_SMO_WATCHDOG_NAME, APP_SMO_WATCHDOG_VERSION};
use crate::models::watchdog_runtime_config::WatchdogRuntimeConfig;
use anyhow::{Context, Result};
use dotenv::dotenv;
use shared::constants::general::DASH_LINE;
use std::env;
use std::path::PathBuf;

pub fn print_runtime_info(runtime_config: &WatchdogRuntimeConfig) {
    println!("🐶 {} v{}", APP_SMO_WATCHDOG_NAME, APP_SMO_WATCHDOG_VERSION);
    println!("{}", DASH_LINE);

    println!("📁 Watching folder: {:?}", runtime_config.watch_folder);
    println!(
        "🎥 Target Movies folder: {:?}",
        runtime_config.target_base_movie_folder
    );
    println!(
        "📺 Target Series folder: {:?}",
        runtime_config.target_base_series_folder
    );

    println!();
}

pub fn get_runtime_info() -> Result<WatchdogRuntimeConfig> {
    dotenv().ok();

    let watch_folder_str =
        env::var("SMO_WATCHDOG_WATCH_FOLDER").context("SMO_WATCHDOG_WATCH_FOLDER is not set")?;
    let watch_folder = PathBuf::from(&watch_folder_str);

    let target_base_movie_folder_str = env::var("SMO_WATCHDOG_WATCH_BASE_MOVIE_FOLDER")
        .context("SMO_WATCHDOG_WATCH_BASE_MOVIE_FOLDER is not set")?;
    let target_base_movie_folder = PathBuf::from(&target_base_movie_folder_str);

    let target_base_series_folder_str = env::var("SMO_WATCHDOG_WATCH_BASE_TVSHOW_FOLDER")
        .context("SMO_WATCHDOG_WATCH_BASE_TVSHOW_FOLDER is not set")?;
    let target_base_series_folder = PathBuf::from(&target_base_series_folder_str);

    Ok(WatchdogRuntimeConfig::new(
        watch_folder,
        target_base_movie_folder,
        target_base_series_folder,
    ))
}

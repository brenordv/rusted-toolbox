use crate::models::{
    ConnectivityResult, NetQualityConfig, SpeedResult, ThresholdCategory, Thresholds,
};
use crate::netquality_app::state::LoopState;
use anyhow::{Context, Result};
use cfspeedtest::{OutputFormat, SpeedTestCLIOptions};
use chrono::Utc;
use reqwest::Client;
use serde::Deserialize;
use std::process::Command;
use std::time::{Duration, Instant};
use tokio::task::spawn_blocking;
use tracing::{info, trace};

pub async fn run_connectivity_check(
    config: &NetQualityConfig,
    state: &mut LoopState,
) -> Result<ConnectivityResult> {
    let client = Client::builder()
        .timeout(config.connectivity.timeout)
        .build()
        .context("Failed to build HTTP client")?;

    let start_time = Instant::now();
    let total_urls = config.connectivity.urls.len();
    let mut attempts = 0;
    let mut success = false;
    let mut result = "timeout".to_string();
    let mut selected_url = String::new();

    while attempts < total_urls {
        let index = (state.current_url_index() + attempts) % total_urls;
        let url = &config.connectivity.urls[index];
        trace!("Connectivity check against {}", url);
        let response = client.get(url).send().await;
        match response {
            Ok(resp) => {
                success = true;
                selected_url = url.to_string();
                result = resp.status().as_u16().to_string();
                state.update_next_url_index((index + 1) % total_urls);
                break;
            }
            Err(error) => {
                selected_url = url.to_string();
                if error.is_timeout() {
                    result = "timeout".to_string();
                } else {
                    result = "error".to_string();
                }
                attempts += 1;
            }
        }
    }

    if !success {
        state.update_next_url_index((state.current_url_index() + 1) % total_urls);
    }

    let elapsed_ms = start_time.elapsed().as_millis() as i64;
    Ok(ConnectivityResult {
        timestamp: Utc::now(),
        url: selected_url,
        result,
        elapsed_ms,
        success,
    })
}
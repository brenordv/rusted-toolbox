use crate::models::{
    ConnectivityResult, NetQualityConfig, SpeedResult, ThresholdCategory, Thresholds,
};
use crate::netquality_app::state::LoopState;
use anyhow::{anyhow, Context, Result};
use cfspeedtest::measurements::Measurement;
use cfspeedtest::speedtest::TestType;
use cfspeedtest::{OutputFormat, SpeedTestCLIOptions};
use chrono::Utc;
use reqwest::Client;
use serde::Deserialize;
use std::process::Command;
use std::time::{Duration, Instant};
use tokio::task::spawn_blocking;
use tracing::{info, trace};

pub(super) async fn run_connectivity_check(
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
        let index = (state.next_url_index + attempts) % total_urls;
        let url = &config.connectivity.urls[index];
        trace!("Connectivity check against {}", url);
        let response = client.get(url).send().await;
        match response {
            Ok(resp) => {
                success = true;
                selected_url = url.to_string();
                result = resp.status().as_u16().to_string();
                state.next_url_index = (index + 1) % total_urls;
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
        state.next_url_index = (state.next_url_index + 1) % total_urls;
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

pub(super) async fn run_speed_check(config: &NetQualityConfig) -> Result<SpeedResult> {
    let expected_download = config.speed.expected_download_mbps;
    let expected_upload = config.speed.expected_upload_mbps;
    let download_thresholds = config.speed.download_thresholds.clone();
    let upload_thresholds = config.speed.upload_thresholds.clone();
    let download_only = expected_upload.is_none();

    if let Some(path) = config.speed.speedtest_cli_path.clone() {
        match run_ookla_speedtest(path, download_only).await {
            Ok(ookla_result) => {
                return Ok(build_speed_result(
                    ookla_result,
                    expected_download,
                    expected_upload,
                    &download_thresholds,
                    &upload_thresholds,
                ));
            }
            Err(error) => {
                info!("Speedtest CLI failed ({error}); falling back to Cloudflare test.");
            }
        }
    }

    let cfspeedtest_result = spawn_blocking(move || -> Result<SpeedTestMeasurement> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(240))
            .build()
            .context("Failed to build speed test client")?;

        let options = SpeedTestCLIOptions {
            nr_tests: 20,
            nr_latency_tests: 25,
            max_payload_size: cfspeedtest::speedtest::PayloadSize::M100,
            output_format: OutputFormat::None,
            verbose: true,
            ipv4: None,
            ipv6: None,
            disable_dynamic_max_payload_size: true,
            download_only,
            upload_only: false, // It is either only download or both.
            completion: None,
        };

        let start = Instant::now();
        let measurements = cfspeedtest::speedtest::speed_test(client, options);
        let elapsed_ms = start.elapsed().as_millis() as i64;

        let download_mbps = average_mbit(&measurements, TestType::Download)
            .ok_or_else(|| anyhow!("Speed test did not return download measurements"))?;

        let upload_mbps = if expected_upload.is_some() {
            average_mbit(&measurements, TestType::Upload)
        } else {
            None
        };

        Ok(SpeedTestMeasurement {
            download_mbps,
            upload_mbps,
            elapsed_ms,
        })
    })
    .await
    .context("Speed check task failed")??;

    Ok(build_speed_result(
        cfspeedtest_result,
        expected_download,
        expected_upload,
        &download_thresholds,
        &upload_thresholds,
    ))
}

#[derive(Debug, Clone, Copy)]
struct SpeedTestMeasurement {
    download_mbps: f64,
    upload_mbps: Option<f64>,
    elapsed_ms: i64,
}

#[derive(Debug, Deserialize)]
struct SpeedtestCliOutput {
    download: SpeedtestCliTransfer,
    upload: Option<SpeedtestCliTransfer>,
}

#[derive(Debug, Deserialize)]
struct SpeedtestCliTransfer {
    bandwidth: f64,
}

async fn run_ookla_speedtest(
    path: std::path::PathBuf,
    download_only: bool,
) -> Result<SpeedTestMeasurement> {
    spawn_blocking(move || -> Result<SpeedTestMeasurement> {
        let mut command = Command::new(path);
        command
            .arg("--format")
            .arg("json")
            .arg("--accept-license")
            .arg("--accept-gdpr");

        if download_only {
            command.arg("--download-only");
        }

        let start = Instant::now();
        let output = command.output().context("Failed to start speedtest CLI")?;
        let elapsed_ms = start.elapsed().as_millis() as i64;

        if !output.status.success() {
            return Err(anyhow!(
                "Speedtest CLI exited with status {}",
                output.status
            ));
        }

        let stdout =
            String::from_utf8(output.stdout).context("Speedtest CLI output was not valid UTF-8")?;
        let parsed: SpeedtestCliOutput =
            serde_json::from_str(&stdout).context("Failed to parse speedtest CLI JSON output")?;

        let download_mbps = bytes_per_sec_to_mbps(parsed.download.bandwidth);
        let upload_mbps = parsed
            .upload
            .map(|upload| bytes_per_sec_to_mbps(upload.bandwidth));

        Ok(SpeedTestMeasurement {
            download_mbps,
            upload_mbps,
            elapsed_ms,
        })
    })
    .await
    .context("Speedtest CLI task failed")?
}

fn bytes_per_sec_to_mbps(bytes_per_sec: f64) -> f64 {
    (bytes_per_sec * 8.0) / 1_000_000.0
}

fn build_speed_result(
    measurement: SpeedTestMeasurement,
    expected_download: f64,
    expected_upload: Option<f64>,
    download_thresholds: &Thresholds,
    upload_thresholds: &Thresholds,
) -> SpeedResult {
    let download_threshold = evaluate_threshold(
        measurement.download_mbps,
        expected_download,
        download_thresholds,
    );

    let upload_threshold = match (measurement.upload_mbps, expected_upload) {
        (Some(actual), Some(expected)) => {
            Some(evaluate_threshold(actual, expected, upload_thresholds))
        }
        _ => None,
    };

    SpeedResult {
        timestamp: Utc::now(),
        download_mbps: measurement.download_mbps,
        upload_mbps: measurement.upload_mbps,
        download_threshold,
        upload_threshold,
        elapsed_ms: measurement.elapsed_ms,
        success: true,
    }
}

fn average_mbit(measurements: &[Measurement], test_type: TestType) -> Option<f64> {
    let max_payload_size = measurements
        .iter()
        .filter(|measurement| measurement.test_type == test_type)
        .map(|measurement| measurement.payload_size)
        .max()?;

    let values: Vec<f64> = measurements
        .iter()
        .filter(|measurement| measurement.test_type == test_type)
        .filter(|measurement| measurement.payload_size == max_payload_size)
        .map(|measurement| measurement.mbit)
        .collect();

    if values.is_empty() {
        return None;
    }

    let sum: f64 = values.iter().sum();
    Some(sum / values.len() as f64)
}

fn evaluate_threshold(
    actual_mbps: f64,
    expected_mbps: f64,
    thresholds: &Thresholds,
) -> ThresholdCategory {
    let percent = (actual_mbps / expected_mbps) * 100.0;

    if percent <= thresholds.very_slow {
        ThresholdCategory::VerySlow
    } else if percent <= thresholds.slow {
        ThresholdCategory::Slow
    } else if percent <= thresholds.medium {
        ThresholdCategory::Medium
    } else if percent <= thresholds.medium_fast {
        ThresholdCategory::MediumFast
    } else {
        ThresholdCategory::Expected
    }
}

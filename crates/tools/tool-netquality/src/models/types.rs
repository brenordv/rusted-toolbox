use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use clap::ValueEnum;
use common_cli::tool_path_helpers::get_tool_path;
use common_serialization_utils::load_json_file_to_object::load_json_file_to_object;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct NetQualityConfig {
    pub connectivity: ConnectivityConfig,
    pub speed: SpeedConfig,
    pub notifications: NotificationConfig,
    pub storage: StorageConfig,
    pub otel_endpoint: String,
}

const DEFAULT_CONNECTIVITY_DELAY_SECS: u64 = 60;
const DEFAULT_CONNECTIVITY_TIMEOUT_SECS: u64 = 10;
const DEFAULT_OUTAGE_BACKOFF_SECS: u64 = 10;
const DEFAULT_OUTAGE_BACKOFF_MAX_SECS: u64 = 3_600;
const DEFAULT_SPEED_DELAY_SECS: u64 = 14_400;
const DEFAULT_CLEANUP_INTERVAL_DAYS: u64 = 365;

impl NetQualityConfig {
    pub async fn from_config(config_path: PathBuf) -> Result<Self> {
        if !config_path.exists() {
            anyhow::bail!(
                "Config file '{:?}' does not exist or cannot be accessed.",
                config_path
            )
        }

        let config_file = load_json_file_to_object::<ConfigFile>(config_path.as_path()).await?;

        Ok(NetQualityConfig {
            connectivity: resolve_connectivity(config_file.connectivity)?,
            speed: resolve_speed(config_file.speed)?,
            notifications: resolve_notifications(config_file.notifications)?,
            storage: resolve_storage(config_file.storage)?,
            otel_endpoint: String::new(),
        })
    }
}

fn resolve_connectivity(config: Option<ConnectivityConfigFile>) -> Result<ConnectivityConfig> {
    let delay_secs = config
        .as_ref()
        .and_then(|c| c.delay_secs)
        .unwrap_or(DEFAULT_CONNECTIVITY_DELAY_SECS);
    let timeout_secs = config
        .as_ref()
        .and_then(|c| c.timeout_secs)
        .unwrap_or(DEFAULT_CONNECTIVITY_TIMEOUT_SECS);
    let outage_backoff_secs = config
        .as_ref()
        .and_then(|c| c.outage_backoff_secs)
        .unwrap_or(DEFAULT_OUTAGE_BACKOFF_SECS);
    let outage_backoff_max_secs = config
        .as_ref()
        .and_then(|c| c.outage_backoff_max_secs)
        .unwrap_or(DEFAULT_OUTAGE_BACKOFF_MAX_SECS);
    let url_mode = config
        .as_ref()
        .and_then(|c| c.url_mode)
        .unwrap_or(UrlMode::Merge);
    let user_urls = config.and_then(|c| c.urls).unwrap_or_default();

    if delay_secs == 0 || timeout_secs == 0 {
        return Err(anyhow!(
            "Connectivity delay and timeout must be greater than zero."
        ));
    }
    if outage_backoff_secs == 0 || outage_backoff_max_secs == 0 {
        return Err(anyhow!("Outage backoff values must be greater than zero."));
    }
    if outage_backoff_max_secs < outage_backoff_secs {
        return Err(anyhow!(
            "Outage backoff max must be greater than or equal to outage backoff."
        ));
    }

    let mut urls: Vec<String> = match url_mode {
        UrlMode::Merge => DEFAULT_URLS
            .iter()
            .map(|url| url.to_string())
            .chain(user_urls)
            .collect(),
        UrlMode::Replace => user_urls,
    };
    urls = dedupe_urls(urls);
    if urls.is_empty() {
        urls = DEFAULT_URLS.iter().map(|url| url.to_string()).collect();
    }

    Ok(ConnectivityConfig {
        delay: Duration::from_secs(delay_secs),
        timeout: Duration::from_secs(timeout_secs),
        outage_backoff: Duration::from_secs(outage_backoff_secs),
        outage_backoff_max: Duration::from_secs(outage_backoff_max_secs),
        urls,
    })
}

fn resolve_speed(config: Option<SpeedConfigFile>) -> Result<SpeedConfig> {
    let config = config
        .context("Config file must include a 'speed' section with an expected download speed.")?;

    let download_thresholds = config
        .download_thresholds
        .unwrap_or_else(Thresholds::default_thresholds);
    download_thresholds
        .validate()
        .context("Invalid download thresholds")?;

    let upload_thresholds = config
        .upload_thresholds
        .unwrap_or_else(Thresholds::default_thresholds);
    upload_thresholds
        .validate()
        .context("Invalid upload thresholds")?;

    Ok(SpeedConfig {
        expected_download_mbps: config.expected_download_mbps,
        expected_upload_mbps: config.expected_upload_mbps,
        delay: Duration::from_secs(config.delay_secs.unwrap_or(DEFAULT_SPEED_DELAY_SECS)),
        download_thresholds,
        upload_thresholds,
        speedtest_cli_path: config.speedtest_cli_path,
    })
}

fn resolve_notifications(config: Option<NotificationConfigFile>) -> Result<NotificationConfig> {
    let config = match config {
        Some(c) => c,
        None => {
            return Ok(NotificationConfig {
                telegram: None,
                min_download_threshold: ThresholdCategory::Medium,
                min_upload_threshold: ThresholdCategory::Slow,
            })
        }
    };

    let telegram = match config.telegram {
        Some(t) => match (t.bot_token, t.chat_id) {
            (Some(bot_token), Some(chat_id)) => Some(TelegramConfig { bot_token, chat_id }),
            (None, None) => None,
            _ => {
                return Err(anyhow!(
                    "Telegram bot token and chat ID must both be provided together."
                ))
            }
        },
        None => None,
    };

    Ok(NotificationConfig {
        telegram,
        min_download_threshold: config
            .min_download_threshold
            .unwrap_or(ThresholdCategory::Medium),
        min_upload_threshold: config
            .min_upload_threshold
            .unwrap_or(ThresholdCategory::Slow),
    })
}

fn resolve_storage(config: Option<StorageConfigFile>) -> Result<StorageConfig> {
    let (db_path, cleanup_enabled, cleanup_interval_days) = match config {
        Some(c) => (
            c.db_path,
            c.cleanup_enabled.unwrap_or(true),
            c.cleanup_interval_days
                .unwrap_or(DEFAULT_CLEANUP_INTERVAL_DAYS),
        ),
        None => (None, true, DEFAULT_CLEANUP_INTERVAL_DAYS),
    };

    if cleanup_interval_days == 0 {
        return Err(anyhow!(
            "Storage cleanup interval days must be greater than zero."
        ));
    }

    let db_path = match db_path {
        Some(path) => path,
        None => get_tool_path()
            .context("Failed to resolve tool path for database default")?
            .join("netquality.db"),
    };

    let cleanup_interval_secs = cleanup_interval_days
        .checked_mul(86_400)
        .ok_or_else(|| anyhow!("Storage cleanup interval days is too large."))?;

    Ok(StorageConfig {
        db_path,
        cleanup_enabled,
        cleanup_interval: Duration::from_secs(cleanup_interval_secs),
    })
}

#[derive(Debug, Clone)]
pub struct ConnectivityConfig {
    pub delay: Duration,
    pub timeout: Duration,
    pub outage_backoff: Duration,
    pub outage_backoff_max: Duration,
    pub urls: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SpeedConfig {
    pub expected_download_mbps: f32,
    pub expected_upload_mbps: Option<f32>,
    pub delay: Duration,
    pub download_thresholds: Thresholds,
    pub upload_thresholds: Thresholds,
    pub speedtest_cli_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct NotificationConfig {
    pub telegram: Option<TelegramConfig>,
    pub min_download_threshold: ThresholdCategory,
    pub min_upload_threshold: ThresholdCategory,
}

#[derive(Debug, Clone)]
pub struct TelegramConfig {
    pub bot_token: String,
    pub chat_id: String,
}

#[derive(Debug, Clone)]
pub struct StorageConfig {
    pub db_path: PathBuf,
    pub cleanup_enabled: bool,
    pub cleanup_interval: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Thresholds {
    pub very_slow: f64,
    pub slow: f64,
    pub medium: f64,
    pub medium_fast: f64,
}

impl Thresholds {
    pub fn default_thresholds() -> Self {
        Self {
            very_slow: 30.0,
            slow: 50.0,
            medium: 65.0,
            medium_fast: 85.0,
        }
    }

    pub fn validate(&self) -> Result<()> {
        let in_range = |value: f64| (0.0..=100.0).contains(&value);
        if !in_range(self.very_slow)
            || !in_range(self.slow)
            || !in_range(self.medium)
            || !in_range(self.medium_fast)
        {
            return Err(anyhow!("threshold values must be between 0 and 100"));
        }

        if !(self.very_slow <= self.slow
            && self.slow <= self.medium
            && self.medium <= self.medium_fast)
        {
            return Err(anyhow!("threshold values must be in ascending order"));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigFile {
    pub connectivity: Option<ConnectivityConfigFile>,
    pub speed: Option<SpeedConfigFile>,
    pub notifications: Option<NotificationConfigFile>,
    pub storage: Option<StorageConfigFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectivityConfigFile {
    pub delay_secs: Option<u64>,
    pub timeout_secs: Option<u64>,
    pub outage_backoff_secs: Option<u64>,
    pub outage_backoff_max_secs: Option<u64>,
    pub urls: Option<Vec<String>>,
    pub url_mode: Option<UrlMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeedConfigFile {
    pub expected_download_mbps: f32,
    pub expected_upload_mbps: Option<f32>,
    pub delay_secs: Option<u64>,
    pub download_thresholds: Option<Thresholds>,
    pub upload_thresholds: Option<Thresholds>,
    pub speedtest_cli_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationConfigFile {
    pub telegram: Option<TelegramConfigFile>,
    pub min_download_threshold: Option<ThresholdCategory>,
    pub min_upload_threshold: Option<ThresholdCategory>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfigFile {
    pub bot_token: Option<String>,
    pub chat_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfigFile {
    pub db_path: Option<PathBuf>,
    pub cleanup_enabled: Option<bool>,
    pub cleanup_interval_days: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum UrlMode {
    Merge,
    Replace,
}

pub const DEFAULT_URLS: &[&str] = &[
    "https://www.google.com/generate_204",
    "https://www.cloudflare.com/cdn-cgi/trace",
    "https://1.1.1.1",
    "https://8.8.8.8",
];

pub fn dedupe_urls(urls: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();

    for url in urls {
        let normalized = url.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            continue;
        }
        if seen.insert(normalized) {
            result.push(url.trim().to_string());
        }
    }

    result
}

#[derive(ValueEnum, Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ThresholdCategory {
    VerySlow,
    Slow,
    Medium,
    MediumFast,
    Expected,
}

impl fmt::Display for ThresholdCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ThresholdCategory::VerySlow => write!(f, "Very Slow"),
            ThresholdCategory::Slow => write!(f, "Slow"),
            ThresholdCategory::Medium => write!(f, "Medium"),
            ThresholdCategory::MediumFast => write!(f, "Medium Fast"),
            ThresholdCategory::Expected => write!(f, "Expected"),
        }
    }
}

impl ThresholdCategory {
    pub fn severity_rank(self) -> u8 {
        match self {
            ThresholdCategory::VerySlow => 0,
            ThresholdCategory::Slow => 1,
            ThresholdCategory::Medium => 2,
            ThresholdCategory::MediumFast => 3,
            ThresholdCategory::Expected => 4,
        }
    }

    pub fn is_at_or_below(self, minimum: ThresholdCategory) -> bool {
        self.severity_rank() <= minimum.severity_rank()
    }

    pub fn label(self) -> &'static str {
        match self {
            ThresholdCategory::VerySlow => "Very Slow",
            ThresholdCategory::Slow => "Slow",
            ThresholdCategory::Medium => "Medium",
            ThresholdCategory::MediumFast => "Medium Fast",
            ThresholdCategory::Expected => "Expected",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConnectivityResult {
    pub timestamp: DateTime<Utc>,
    pub url: String,
    pub result: String,
    pub elapsed_ms: i64,
    pub success: bool,
}

#[derive(Debug, Clone)]
pub struct SpeedResult {
    pub timestamp: DateTime<Utc>,
    pub download_mbps: f64,
    pub upload_mbps: Option<f64>,
    pub download_threshold: ThresholdCategory,
    pub upload_threshold: Option<ThresholdCategory>,
    pub elapsed_ms: i64,
    pub success: bool,
}

pub struct OutageInfo {
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedupe_urls_is_case_insensitive() {
        let urls = vec![
            "https://example.com".to_string(),
            "https://EXAMPLE.com".to_string(),
            " https://example.com ".to_string(),
            "https://another.com".to_string(),
        ];

        let result = dedupe_urls(urls);
        assert_eq!(result.len(), 2);
        assert!(result.iter().any(|url| url.contains("example.com")));
        assert!(result.iter().any(|url| url.contains("another.com")));
    }

    #[test]
    fn thresholds_validation_accepts_defaults() {
        let thresholds = Thresholds::default_thresholds();
        assert!(thresholds.validate().is_ok());
    }

    #[test]
    fn thresholds_validation_rejects_out_of_order() {
        let thresholds = Thresholds {
            very_slow: 40.0,
            slow: 30.0,
            medium: 65.0,
            medium_fast: 85.0,
        };

        assert!(thresholds.validate().is_err());
    }
}

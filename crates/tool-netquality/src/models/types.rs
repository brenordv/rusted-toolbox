use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct NetQualityCliArgs {
    pub config_path: Option<PathBuf>,
    pub urls: Vec<String>,
    pub replace_urls: bool,
    pub expected_download_mbps: Option<f64>,
    pub expected_upload_mbps: Option<f64>,
    pub download_thresholds: Option<Thresholds>,
    pub upload_thresholds: Option<Thresholds>,
    pub min_download_notification_threshold: Option<ThresholdCategory>,
    pub min_upload_notification_threshold: Option<ThresholdCategory>,
    pub connectivity_delay_secs: Option<u64>,
    pub speed_delay_secs: Option<u64>,
    pub connectivity_timeout_secs: Option<u64>,
    pub outage_backoff_secs: Option<u64>,
    pub outage_backoff_max_secs: Option<u64>,
    pub db_path: Option<PathBuf>,
    pub speedtest_cli_path: Option<PathBuf>,
    pub telegram_token: Option<String>,
    pub telegram_chat_id: Option<String>,
    pub otel_endpoint: Option<String>,
    pub verbose: bool,
}

#[derive(Debug, Clone)]
pub struct NetQualityConfig {
    pub connectivity: ConnectivityConfig,
    pub speed: SpeedConfig,
    pub notifications: NotificationConfig,
    pub storage: StorageConfig,
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
    pub expected_download_mbps: f64,
    pub expected_upload_mbps: Option<f64>,
    pub delay: Duration,
    pub download_thresholds: Thresholds,
    pub upload_thresholds: Thresholds,
    pub speedtest_cli_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct NotificationConfig {
    pub telegram: Option<TelegramConfig>,
    pub otel_endpoint: Option<String>,
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

    pub fn validate(&self) -> Result<(), String> {
        let in_range = |value: f64| value >= 0.0 && value <= 100.0;
        if !in_range(self.very_slow)
            || !in_range(self.slow)
            || !in_range(self.medium)
            || !in_range(self.medium_fast)
        {
            return Err("threshold values must be between 0 and 100".to_string());
        }

        if !(self.very_slow <= self.slow
            && self.slow <= self.medium
            && self.medium <= self.medium_fast)
        {
            return Err("threshold values must be in ascending order".to_string());
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
    pub expected_download_mbps: Option<f64>,
    pub expected_upload_mbps: Option<f64>,
    pub delay_secs: Option<u64>,
    pub download_thresholds: Option<Thresholds>,
    pub upload_thresholds: Option<Thresholds>,
    pub speedtest_cli_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationConfigFile {
    pub telegram: Option<TelegramConfigFile>,
    pub otel_endpoint: Option<String>,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ThresholdCategory {
    VerySlow,
    Slow,
    Medium,
    MediumFast,
    Expected,
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

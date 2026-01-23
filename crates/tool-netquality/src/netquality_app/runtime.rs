use crate::models::NetQualityConfig;

pub(super) fn build_runtime_info(config: &NetQualityConfig) -> Vec<(&str, String)> {
    let cleanup_interval_days = config.storage.cleanup_interval.as_secs() / 86_400;
    let cleanup_status = if config.storage.cleanup_enabled {
        format!("enabled (every {} days)", cleanup_interval_days)
    } else {
        "disabled".to_string()
    };

    vec![
        ("Database", config.storage.db_path.display().to_string()),
        ("DB cleanup", cleanup_status),
        (
            "Connectivity delay",
            format!("{}s", config.connectivity.delay.as_secs()),
        ),
        (
            "Connectivity timeout",
            format!("{}s", config.connectivity.timeout.as_secs()),
        ),
        (
            "Outage backoff",
            format!(
                "{}s (max {}s)",
                config.connectivity.outage_backoff.as_secs(),
                config.connectivity.outage_backoff_max.as_secs()
            ),
        ),
        ("Speed delay", format!("{}s", config.speed.delay.as_secs())),
        (
            "Expected download",
            format!("{:.2} Mbps", config.speed.expected_download_mbps),
        ),
        (
            "Expected upload",
            config
                .speed
                .expected_upload_mbps
                .map(|value| format!("{:.2} Mbps", value))
                .unwrap_or_else(|| "disabled".to_string()),
        ),
        (
            "URL checks",
            format!("{} targets", config.connectivity.urls.len()),
        ),
        (
            "Telegram",
            config
                .notifications
                .telegram
                .as_ref()
                .map(|_| "enabled".to_string())
                .unwrap_or_else(|| "disabled".to_string()),
        ),
        (
            "OpenTelemetry",
            config
                .notifications
                .otel_endpoint
                .as_ref()
                .map(|endpoint| endpoint.to_string())
                .unwrap_or_else(|| "disabled".to_string()),
        ),
        (
            "Min notify download",
            config
                .notifications
                .min_download_threshold
                .label()
                .to_string(),
        ),
        (
            "Min notify upload",
            config
                .notifications
                .min_upload_threshold
                .label()
                .to_string(),
        ),
    ]
}

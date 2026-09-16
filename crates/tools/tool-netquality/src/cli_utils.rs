use crate::models::{
    resolve_urls, BotToken, ConnectivityConfig, NetQualityConfig, NotificationConfig, SpeedConfig,
    StorageConfig, TelegramConfig, ThresholdCategory, Thresholds, UrlMode,
};
use anyhow::{anyhow, Context, Result};
use clap::{ArgAction, Parser};
use common_cli::common_tool_args::CommonToolArgs;
use common_cli::header_format::format_config_item_level3;
use common_cli::tool_path_helpers::get_tool_path;
use common_utils::constants::{CONFIG_UL_ITEM_LEVEL_2, CONFIG_UL_ITEM_LEVEL_3};
use logging_otel::{OtelCommonToolArgs, OtelGuard};
use std::path::PathBuf;
use std::time::Duration;

/// Cross-platform CLI tool that monitors internet connectivity and speed.
///
/// Monitor connectivity and speed with notifications, saving the results to an SQLite database, allowing you to analyze them later. Notification to Telegram and/or Open Telemetry is also available.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about)]
pub struct CliArgs {
    /// Path to the configuration file.
    // The conflict entries are clap argument IDs (field names), not the long
    // flag spellings.
    #[arg(long="config",
    required = false,
    conflicts_with_all = &[
    "url",
    "replace_urls",
    "expected_download",
    "expected_upload",
    "download_thresholds",
    "upload_thresholds",
    "min_download_threshold",
    "min_upload_threshold",
    "connectivity_delay",
    "speed_delay",
    "connectivity_timeout",
    "outage_backoff",
    "outage_backoff_max",
    "db_path",
    "disable_db_cleanup_enabled",
    "db_cleanup_interval",
    "speedtest_cli_path",
    "telegram_token",
    "telegram_chat_id",
    "otel_endpoint"
    ])]
    pub config: Option<PathBuf>,

    /// Connectivity check URL (repeatable).
    #[arg(long="url", action = ArgAction::Append, value_name = "URL")]
    pub url: Vec<String>,

    /// Replace the default URL list instead of merging.
    #[arg(long = "replace-urls", required = false)]
    replace_urls: bool,

    /// Expected download speed in Mbps.
    #[arg(
        long = "expected-download",
        value_name = "Mbps",
        required_unless_present = "config"
    )]
    expected_download: Option<f32>,

    /// Expected upload speed in Mbps. When omitted, upload checks are disabled.
    #[arg(long = "expected-upload", value_name = "Mbps", required = false)]
    expected_upload: Option<f32>,

    /// Download thresholds (very slow, slow, medium, and medium fast) as comma-separated percentages.
    #[arg(long="download-thresholds", required = false, default_value_t = String::from("30,50,65,85"))]
    download_thresholds: String,

    /// Upload thresholds (very slow, slow, medium, and medium fast) as comma-separated percentages.
    #[arg(long="upload-thresholds", value_name = "V,S,M,MF", required = false, default_value_t = String::from("30,50,65,85"))]
    upload_thresholds: String,

    /// Minimum download threshold to trigger notifications (very_slow|slow|medium|medium_fast|expected).
    // String defaults, not default_value_t: the Display labels ("Medium",
    // "Very Slow") are not valid snake_case CLI values, so rendering a variant
    // as the default could fail to parse back.
    #[arg(
        long = "min-download-threshold",
        value_name = "THRESHOLD",
        required = false,
        ignore_case = true,
        default_value = "medium"
    )]
    min_download_threshold: ThresholdCategory,

    /// Minimum upload threshold to trigger notifications (very_slow|slow|medium|medium_fast|expected).
    #[arg(
        long = "min-upload-threshold",
        value_name = "THRESHOLD",
        required = false,
        ignore_case = true,
        default_value = "slow"
    )]
    min_upload_threshold: ThresholdCategory,

    /// Connectivity check delay in seconds.
    #[arg(
        long = "connectivity-delay",
        value_name = "secs",
        required = false,
        default_value_t = 60
    )]
    connectivity_delay: u16,

    /// Speed check delay in seconds
    #[arg(
        long = "speed-delay",
        value_name = "secs",
        required = false,
        default_value_t = 14_400
    )]
    speed_delay: u16,

    /// Connectivity request timeout in seconds
    #[arg(
        long = "connectivity-timeout",
        value_name = "secs",
        required = false,
        default_value_t = 10
    )]
    connectivity_timeout: u16,

    /// Outage backoff delay in seconds
    #[arg(
        long = "outage-backoff",
        value_name = "secs",
        required = false,
        default_value_t = 10
    )]
    outage_backoff: u16,

    /// Maximum outage backoff delay in seconds
    #[arg(
        long = "outage-backoff-max",
        value_name = "secs",
        required = false,
        default_value_t = 3_600
    )]
    outage_backoff_max: u16,

    /// SQLite database path [default: <tool path>/netquality.db]
    #[arg(long = "db-path", required = false)]
    db_path: Option<PathBuf>,

    /// Disable database cleanup
    #[arg(long = "disable-db-cleanup-enabled", required = false)]
    disable_db_cleanup_enabled: bool,

    /// Database cleanup interval in seconds
    #[arg(long = "db-cleanup-interval", required = false, default_value_t = 3600)]
    db_cleanup_interval: u16,

    /// Path to Ookla speedtest CLI binary
    #[arg(long = "speedtest-cli-path", required_unless_present = "config")]
    speedtest_cli_path: Option<PathBuf>,

    /// Telegram bot token for notification. Prefer the config-file form: a CLI
    /// token is visible in the process list and shell history.
    #[arg(long = "telegram-token", required = false)]
    telegram_token: Option<BotToken>,

    /// Telegram chat ID for notification.
    #[arg(long = "telegram-chat-id", required = false)]
    telegram_chat_id: Option<String>,

    /// OpenTelemetry OTLP endpoint.
    #[arg(long = "otel-endpoint", required = false)]
    otel_endpoint: Option<String>,

    #[command(flatten)]
    pub common: CommonToolArgs,
}

impl NetQualityConfig {
    fn from_args(args: &CliArgs) -> Result<Self> {
        Ok(NetQualityConfig {
            connectivity: ConnectivityConfig::from_args(args),
            speed: SpeedConfig::from_args(args).context("Failed to parse speed config")?,
            notifications: NotificationConfig::from_args(args),
            storage: StorageConfig::from_args(args).context("Failed to parse storage config")?,
            otel_endpoint: args.otel_endpoint.clone(),
        })
    }
}

impl ConnectivityConfig {
    fn from_args(args: &CliArgs) -> Self {
        let delay = Duration::from_secs(args.connectivity_delay as u64);
        let timeout = Duration::from_secs(args.connectivity_timeout as u64);
        let outage_backoff = Duration::from_secs(args.outage_backoff as u64);
        let outage_backoff_max = Duration::from_secs(args.outage_backoff_max as u64);
        let url_mode = if args.replace_urls {
            UrlMode::Replace
        } else {
            UrlMode::Merge
        };
        let urls = resolve_urls(args.url.clone(), url_mode);

        ConnectivityConfig {
            delay,
            timeout,
            outage_backoff,
            outage_backoff_max,
            urls,
        }
    }
}

impl SpeedConfig {
    fn from_args(args: &CliArgs) -> Result<Self> {
        let expected_download_mbps = args
            .expected_download
            .context("Expected download Mbps must be provided")?;
        let expected_upload_mbps = args.expected_upload;
        let delay = Duration::from_secs(args.speed_delay as u64);
        let download_thresholds = parse_thresholds(args.download_thresholds.as_str())
            .context("Failed to parse download thresholds")?;
        let upload_thresholds = parse_thresholds(args.upload_thresholds.as_str())
            .context("Failed to parse upload thresholds")?;

        let speedtest_cli_path = args.speedtest_cli_path.clone();

        Ok(SpeedConfig {
            expected_download_mbps,
            expected_upload_mbps,
            delay,
            download_thresholds,
            upload_thresholds,
            speedtest_cli_path,
        })
    }
}

impl NotificationConfig {
    fn from_args(args: &CliArgs) -> Self {
        let telegram = TelegramConfig::from_args(args);
        let min_download_threshold = args.min_download_threshold;
        let min_upload_threshold = args.min_upload_threshold;

        NotificationConfig {
            telegram,
            min_download_threshold,
            min_upload_threshold,
        }
    }
}

impl TelegramConfig {
    fn from_args(args: &CliArgs) -> Option<Self> {
        let bot_token = args.telegram_token.clone();
        let chat_id = args.telegram_chat_id.clone();

        if bot_token.is_none() || chat_id.is_none() {
            return None;
        }

        Some(TelegramConfig {
            bot_token: bot_token.unwrap(),
            chat_id: chat_id.unwrap(),
        })
    }
}

impl StorageConfig {
    fn from_args(args: &CliArgs) -> Result<Self> {
        // No existence check: db::create_database creates the file and any
        // missing parent directories on startup.
        let db_path = match &args.db_path {
            Some(p) => p.clone(),
            None => get_tool_path()
                .context("Failed to get tool path")?
                .join("netquality.db"),
        };

        let cleanup_interval = Duration::from_secs(args.db_cleanup_interval as u64);

        Ok(StorageConfig {
            db_path,
            cleanup_enabled: !args.disable_db_cleanup_enabled,
            cleanup_interval,
        })
    }
}

/// Parses the CLI, resolves the configuration, and boots logging with optional
/// OpenTelemetry export. Logging boots before this function returns, on the
/// failure path too, so a resolution error reported by the caller always
/// reaches a live subscriber. The returned guard, when present, must stay
/// alive for the whole run and be dropped before any exit helper so telemetry
/// flushes.
pub async fn initialize() -> Result<(NetQualityConfig, Option<OtelGuard>)> {
    let args = CliArgs::parse();

    let config = match args.config {
        None => NetQualityConfig::from_args(&args),
        Some(config_path) => NetQualityConfig::from_config(config_path)
            .await
            .context("Failed to load configuration file"),
    };

    match config {
        Ok(config) => {
            let otel_guard = args.common.app_boot_up_with_otel(
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_VERSION"),
                false,
                false,
                config.otel_endpoint.as_deref(),
                Some(|| {
                    print_runtime_info(&config);
                }),
            );
            Ok((config, otel_guard))
        }
        Err(e) => {
            // Deliberately no OTel on this path: the endpoint lives in the
            // config that just failed to resolve, and the caller's error arm
            // exits without a guard to drop, so an env-endpoint pipeline
            // would buffer telemetry that never flushes.
            args.common.app_boot_up(
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_VERSION"),
                false,
                false,
                None::<fn()>,
            );
            Err(e)
        }
    }
}

/// Presence flag for the header line, using logging-otel's own resolution
/// rules (configured value first, then OTEL_EXPORTER_OTLP_ENDPOINT). Only
/// presence is reported; the endpoint value itself is never printed.
fn otel_export_configured(config: &NetQualityConfig) -> bool {
    logging_otel::is_otel_endpoint_configured(config.otel_endpoint.as_deref())
}

fn print_runtime_info(config: &NetQualityConfig) {
    let connectivity = &config.connectivity;
    println!("{} Connectivity Config", CONFIG_UL_ITEM_LEVEL_2);
    println!(
        "{}",
        format_config_item_level3("Delay", format!("{:?}", connectivity.delay))
    );
    println!(
        "{}",
        format_config_item_level3("Timeout", format!("{:?}", connectivity.timeout))
    );
    println!(
        "{}",
        format_config_item_level3(
            "Outage backoff",
            format!("{:?}", connectivity.outage_backoff)
        )
    );
    println!(
        "{}",
        format_config_item_level3(
            "Outage backoff max",
            format!("{:?}", connectivity.outage_backoff_max)
        )
    );
    println!(
        "{} URLs ({}):",
        CONFIG_UL_ITEM_LEVEL_3,
        connectivity.urls.len()
    );
    for url in &connectivity.urls {
        println!("{}   {}", CONFIG_UL_ITEM_LEVEL_3, url);
    }

    let speed = &config.speed;
    println!("{} Speed Config", CONFIG_UL_ITEM_LEVEL_2);
    println!(
        "{}",
        format_config_item_level3(
            "Expected download",
            format!("{} Mbps", speed.expected_download_mbps)
        )
    );
    match speed.expected_upload_mbps {
        Some(upload) => println!(
            "{}",
            format_config_item_level3("Expected upload", format!("{} Mbps", upload))
        ),
        None => println!(
            "{}",
            format_config_item_level3("Expected upload", "<download only>")
        ),
    }
    println!(
        "{}",
        format_config_item_level3("Delay", format!("{:?}", speed.delay))
    );
    println!(
        "{}",
        format_config_item_level3(
            "Download thresholds (%)",
            format!(
                "{}/{}/{}/{}",
                speed.download_thresholds.very_slow,
                speed.download_thresholds.slow,
                speed.download_thresholds.medium,
                speed.download_thresholds.medium_fast
            )
        )
    );
    println!(
        "{}",
        format_config_item_level3(
            "Upload thresholds (%)",
            format!(
                "{}/{}/{}/{}",
                speed.upload_thresholds.very_slow,
                speed.upload_thresholds.slow,
                speed.upload_thresholds.medium,
                speed.upload_thresholds.medium_fast
            )
        )
    );
    match &speed.speedtest_cli_path {
        Some(path) => println!(
            "{}",
            format_config_item_level3("Speedtest CLI", path.display())
        ),
        None => println!(
            "{}",
            format_config_item_level3("Speedtest CLI", "embedded (cfspeedtest)")
        ),
    }

    let notifications = &config.notifications;
    println!("{} Notifications Config", CONFIG_UL_ITEM_LEVEL_2);
    let telegram_state = match &notifications.telegram {
        Some(_) => "enabled",
        None => "disabled",
    };
    println!("{}", format_config_item_level3("Telegram", telegram_state));
    // Presence only; the endpoint value is never printed. "configured" rather
    // than "enabled": this line prints before OTel initialization runs, and a
    // setup failure can still fall back to standard logging.
    let otel_state = if otel_export_configured(config) {
        "configured"
    } else {
        "not configured"
    };
    println!(
        "{}",
        format_config_item_level3("OpenTelemetry export", otel_state)
    );
    println!(
        "{}",
        format_config_item_level3(
            "Min download threshold",
            notifications.min_download_threshold
        )
    );
    println!(
        "{}",
        format_config_item_level3("Min upload threshold", notifications.min_upload_threshold)
    );

    let storage = &config.storage;
    println!("{} Storage Config", CONFIG_UL_ITEM_LEVEL_2);
    println!(
        "{}",
        format_config_item_level3("Database path", storage.db_path.display())
    );
    println!(
        "{}",
        format_config_item_level3("Cleanup enabled", storage.cleanup_enabled)
    );
    println!(
        "{}",
        format_config_item_level3(
            "Cleanup interval",
            format!("{:?}", storage.cleanup_interval)
        )
    );
}

fn parse_thresholds(value: &str) -> Result<Thresholds> {
    let parts: Vec<&str> = value.split(',').map(|part| part.trim()).collect();
    if parts.len() != 4 {
        return Err(anyhow!(
            "Thresholds must have 4 comma-separated values (e.g. 30,50,65,85)."
        ));
    }

    let values: Result<Vec<f64>> = parts
        .iter()
        .map(|part| {
            part.parse::<f64>()
                .map_err(|_| anyhow!("Invalid threshold value: {part}"))
        })
        .collect();

    let values = values?;
    let thresholds = Thresholds {
        very_slow: values[0],
        slow: values[1],
        medium: values[2],
        medium_fast: values[3],
    };

    thresholds.validate().context("Invalid threshold values")?;

    Ok(thresholds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::DEFAULT_URLS;

    fn parse_args(extra: &[&str]) -> CliArgs {
        let mut argv = vec![
            "netquality",
            "--expected-download",
            "100",
            "--speedtest-cli-path",
            "speedtest",
        ];
        argv.extend_from_slice(extra);
        CliArgs::try_parse_from(argv).expect("CLI args should parse")
    }

    #[test]
    fn parse_thresholds_accepts_four_values_with_spaces() {
        let thresholds = parse_thresholds("30, 50 ,65,85").expect("should parse");

        assert_eq!(thresholds.very_slow, 30.0);
        assert_eq!(thresholds.slow, 50.0);
        assert_eq!(thresholds.medium, 65.0);
        assert_eq!(thresholds.medium_fast, 85.0);
    }

    #[test]
    fn parse_thresholds_rejects_wrong_value_count() {
        assert!(parse_thresholds("30,50,65").is_err());
        assert!(parse_thresholds("30,50,65,85,95").is_err());
    }

    #[test]
    fn parse_thresholds_rejects_non_numeric_values() {
        assert!(parse_thresholds("30,abc,65,85").is_err());
    }

    #[test]
    fn parse_thresholds_delegates_order_and_range_validation() {
        assert!(parse_thresholds("50,30,65,85").is_err());
        assert!(parse_thresholds("30,50,65,185").is_err());
    }

    #[test]
    fn min_download_threshold_comes_from_its_own_flag() {
        let args = parse_args(&[
            "--min-download-threshold",
            "medium_fast",
            "--min-upload-threshold",
            "very_slow",
        ]);

        let notifications = NotificationConfig::from_args(&args);

        assert_eq!(
            notifications.min_download_threshold,
            ThresholdCategory::MediumFast
        );
        assert_eq!(
            notifications.min_upload_threshold,
            ThresholdCategory::VerySlow
        );
    }

    #[test]
    fn min_threshold_defaults_resolve_to_documented_variants() {
        let args = parse_args(&[]);

        let notifications = NotificationConfig::from_args(&args);

        assert_eq!(
            notifications.min_download_threshold,
            ThresholdCategory::Medium
        );
        assert_eq!(notifications.min_upload_threshold, ThresholdCategory::Slow);
    }

    #[test]
    fn replace_urls_flag_replaces_the_default_list() {
        let args = parse_args(&["--replace-urls", "--url", "https://example.com/health"]);

        let connectivity = ConnectivityConfig::from_args(&args);

        assert_eq!(connectivity.urls, vec!["https://example.com/health"]);
    }

    #[test]
    fn urls_merge_with_defaults_without_replace_flag() {
        let args = parse_args(&["--url", "https://example.com/health"]);

        let connectivity = ConnectivityConfig::from_args(&args);

        assert_eq!(connectivity.urls.len(), DEFAULT_URLS.len() + 1);
        assert_eq!(connectivity.urls[0], DEFAULT_URLS[0]);
        assert_eq!(
            connectivity.urls.last().map(String::as_str),
            Some("https://example.com/health")
        );
    }

    #[test]
    fn cli_args_debug_redacts_the_telegram_token() {
        let secret = "123456:sekret-token-value";
        let args = parse_args(&["--telegram-token", secret, "--telegram-chat-id", "1"]);

        let rendered = format!("{args:?}");
        assert!(!rendered.contains(secret), "leaked: {rendered}");
        assert!(rendered.contains("<redacted>"));
    }

    #[test]
    fn storage_from_args_accepts_missing_db_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("not-created-yet").join("netquality.db");
        let db_path_str = db_path.to_string_lossy().to_string();
        let args = parse_args(&["--db-path", db_path_str.as_str()]);

        let storage = StorageConfig::from_args(&args).expect("storage config should resolve");

        assert_eq!(storage.db_path, db_path);
        assert!(storage.cleanup_enabled);
        assert_eq!(storage.cleanup_interval, Duration::from_secs(3_600));
    }
}

use crate::models::{
    ConnectivityConfig, NetQualityConfig, NotificationConfig, SpeedConfig, StorageConfig,
    TelegramConfig, ThresholdCategory, Thresholds,
};
use anyhow::{anyhow, bail, Context, Result};
use clap::{ArgAction, Parser};
use common_cli::common_tool_args::CommonToolArgs;
use common_cli::tool_path_helpers::get_tool_path;
use common_utils::constants::{CONFIG_UL_ITEM_LEVEL_2, CONFIG_UL_ITEM_LEVEL_3};
use std::path::PathBuf;
use std::time::Duration;

/// Cross-platform CLI tool that monitors internet connectivity and speed.
///
/// Monitor connectivity and speed with notifications, saving the results to an SQLite database, allowing you to analyze them later. Notification to Telegram and/or Open Telemetry is also available.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about)]
pub struct CliArgs {
    /// Path to the configuration file.
    #[arg(long="config",
    required = false,
    conflicts_with_all = &[
    "url",
    "replace-urls",
    "expected-download",
    "expected-upload",
    "download-thresholds",
    "upload-thresholds",
    "min-download-threshold",
    "min-upload-threshold",
    "connectivity-delay",
    "speed-delay",
    "connectivity-timeout",
    "outage-backoff",
    "outage-backoff-max",
    "db-path",
    "speedtest-cli-path",
    "telegram-token",
    "telegram-chat-id",
    "otel-endpoint"
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

    /// Expected upload speed in Mbps.
    #[arg(
        long = "expected-upload",
        value_name = "Mbps",
        required_unless_present = "config"
    )]
    expected_upload: Option<f32>,

    /// Download thresholds (very slow, slow, medium, and medium fast) as comma-separated percentages.
    #[arg(long="download-thresholds", required = false, default_value_t = String::from("30,50,65,85"))]
    download_thresholds: String,

    /// Upload thresholds (very slow, slow, medium, and medium fast) as comma-separated percentages.
    #[arg(long="upload-thresholds", value_name = "V,S,M,MF", required = false, default_value_t = String::from("30,50,65,85"))]
    upload_thresholds: String,

    /// Minimum download threshold to trigger notifications (very_slow|slow|medium|medium_fast|expected).
    #[arg(long="min-download-threshold", value_name = "threshold", required = false, default_value_t = ThresholdCategory::Medium)]
    min_download_threshold: ThresholdCategory,

    /// Minimum upload threshold to trigger notifications (very_slow|slow|medium|medium_fast|expected).
    #[arg(long="min-upload-threshold", value_name = "THRESHOLD", required = false, default_value_t = ThresholdCategory::Slow)]
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

    /// Telegram bot token for notification.
    #[arg(long = "telegram-token", required = false)]
    telegram_token: Option<String>,

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
            otel_endpoint: args.otel_endpoint.clone().unwrap_or_default(),
        })
    }
}

impl ConnectivityConfig {
    fn from_args(args: &CliArgs) -> Self {
        let delay = Duration::from_secs(args.connectivity_delay as u64);
        let timeout = Duration::from_secs(args.connectivity_timeout as u64);
        let outage_backoff = Duration::from_secs(args.outage_backoff as u64);
        let outage_backoff_max = Duration::from_secs(args.outage_backoff_max as u64);
        let urls = args.url.clone();

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
            .context("Failed to parse download thresholds")?;

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
        let min_download_threshold = args.min_upload_threshold;
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
        let db_path = match &args.db_path {
            Some(p) => p.clone(),
            None => get_tool_path()
                .context("Failed to get tool path")?
                .join("netquality.db"),
        };

        if !db_path.exists() {
            bail!("Database path does not exist")
        }

        let cleanup_interval = Duration::from_secs(args.db_cleanup_interval as u64);

        Ok(StorageConfig {
            db_path,
            cleanup_enabled: !args.disable_db_cleanup_enabled,
            cleanup_interval,
        })
    }
}

pub async fn initialize() -> Result<NetQualityConfig> {
    let args = CliArgs::parse();

    let config = match args.config {
        None => NetQualityConfig::from_args(&args)?,
        Some(config_path) => NetQualityConfig::from_config(config_path)
            .await
            .context("")?,
    };

    args.common.app_boot_up(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        false,
        false,
        Some(|| {
            print_runtime_info(&config);
        }),
    );

    Ok(config)
}

fn print_runtime_info(config: &NetQualityConfig) {
    let connectivity = &config.connectivity;
    println!("{} Connectivity Config", CONFIG_UL_ITEM_LEVEL_2);
    println!("{} Delay: {:?}", CONFIG_UL_ITEM_LEVEL_3, connectivity.delay);
    println!(
        "{} Timeout: {:?}",
        CONFIG_UL_ITEM_LEVEL_3, connectivity.timeout
    );
    println!(
        "{} Outage backoff: {:?}",
        CONFIG_UL_ITEM_LEVEL_3, connectivity.outage_backoff
    );
    println!(
        "{} Outage backoff max: {:?}",
        CONFIG_UL_ITEM_LEVEL_3, connectivity.outage_backoff_max
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
        "{} Expected download: {} Mbps",
        CONFIG_UL_ITEM_LEVEL_3, speed.expected_download_mbps
    );
    match speed.expected_upload_mbps {
        Some(upload) => println!(
            "{} Expected upload: {} Mbps",
            CONFIG_UL_ITEM_LEVEL_3, upload
        ),
        None => println!(
            "{} Expected upload: <download only>",
            CONFIG_UL_ITEM_LEVEL_3
        ),
    }
    println!("{} Delay: {:?}", CONFIG_UL_ITEM_LEVEL_3, speed.delay);
    println!(
        "{} Download thresholds (%): {}/{}/{}/{}",
        CONFIG_UL_ITEM_LEVEL_3,
        speed.download_thresholds.very_slow,
        speed.download_thresholds.slow,
        speed.download_thresholds.medium,
        speed.download_thresholds.medium_fast
    );
    println!(
        "{} Upload thresholds (%): {}/{}/{}/{}",
        CONFIG_UL_ITEM_LEVEL_3,
        speed.upload_thresholds.very_slow,
        speed.upload_thresholds.slow,
        speed.upload_thresholds.medium,
        speed.upload_thresholds.medium_fast
    );
    match &speed.speedtest_cli_path {
        Some(path) => println!(
            "{} Speedtest CLI: {}",
            CONFIG_UL_ITEM_LEVEL_3,
            path.display()
        ),
        None => println!(
            "{} Speedtest CLI: embedded (cfspeedtest)",
            CONFIG_UL_ITEM_LEVEL_3
        ),
    }

    let notifications = &config.notifications;
    println!("{} Notifications Config", CONFIG_UL_ITEM_LEVEL_2);
    match &notifications.telegram {
        Some(_) => println!("{} Telegram: enabled", CONFIG_UL_ITEM_LEVEL_3),
        None => println!("{} Telegram: disabled", CONFIG_UL_ITEM_LEVEL_3),
    }
    println!(
        "{} Min download threshold: {}",
        CONFIG_UL_ITEM_LEVEL_3, notifications.min_download_threshold
    );
    println!(
        "{} Min upload threshold: {}",
        CONFIG_UL_ITEM_LEVEL_3, notifications.min_upload_threshold
    );

    let storage = &config.storage;
    println!("{} Storage Config", CONFIG_UL_ITEM_LEVEL_2);
    println!(
        "{} Database path: {}",
        CONFIG_UL_ITEM_LEVEL_3,
        storage.db_path.display()
    );
    println!(
        "{} Cleanup enabled: {}",
        CONFIG_UL_ITEM_LEVEL_3, storage.cleanup_enabled
    );
    println!(
        "{} Cleanup interval: {} day(s)",
        CONFIG_UL_ITEM_LEVEL_3,
        storage.cleanup_interval.as_secs() / 86_400
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

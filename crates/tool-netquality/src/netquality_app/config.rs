use crate::models::{
    dedupe_urls, ConfigFile, ConnectivityConfig, ConnectivityConfigFile, NetQualityCliArgs,
    NetQualityConfig, NotificationConfig, NotificationConfigFile, SpeedConfig, SpeedConfigFile,
    StorageConfig, StorageConfigFile, TelegramConfig, TelegramConfigFile, ThresholdCategory,
    Thresholds, UrlMode, DEFAULT_URLS,
};
use anyhow::{anyhow, Context, Result};
use shared::system::load_json_file_to_object::load_json_file_to_object;
use std::path::{Path, PathBuf};
use std::time::Duration;

const DEFAULT_CONNECTIVITY_DELAY_SECS: u64 = 60;
const DEFAULT_SPEED_DELAY_SECS: u64 = 14_400;
const DEFAULT_CONNECTIVITY_TIMEOUT_SECS: u64 = 1;
const DEFAULT_OUTAGE_BACKOFF_SECS: u64 = 10;
const DEFAULT_OUTAGE_BACKOFF_MAX_SECS: u64 = 3_600;
const DEFAULT_MIN_DOWNLOAD_NOTIFY_THRESHOLD: ThresholdCategory = ThresholdCategory::Medium;
const DEFAULT_MIN_UPLOAD_NOTIFY_THRESHOLD: ThresholdCategory = ThresholdCategory::Slow;
const DEFAULT_STORAGE_CLEANUP_ENABLED: bool = true;
const DEFAULT_STORAGE_CLEANUP_INTERVAL_DAYS: u64 = 365;

pub(super) async fn load_config(args: &NetQualityCliArgs) -> Result<(NetQualityConfig, String)> {
    let config_paths = resolve_config_paths(args)?;
    let config_label = if config_paths.is_empty() {
        "defaults".to_string()
    } else {
        config_paths
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(" + ")
    };

    let mut merged_config: Option<ConfigFile> = None;
    for path in &config_paths {
        let loaded = load_json_file_to_object::<ConfigFile>(path).await?;
        merged_config = Some(match merged_config {
            Some(existing) => merge_config_files(existing, loaded),
            None => loaded,
        });
    }

    let config = build_config(merged_config, args)?;
    Ok((config, config_label))
}

fn resolve_config_paths(args: &NetQualityCliArgs) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();

    let exe_path = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .map(|dir| dir.join("config.json"));
    if let Some(path) = exe_path {
        if path.exists() {
            paths.push(path);
        }
    }

    let cwd_path = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("config.json");
    if cwd_path.exists() {
        paths.push(cwd_path);
    }

    if let Some(path) = &args.config_path {
        if path.exists() {
            paths.push(path.clone());
        } else {
            return Err(anyhow!("Config file not found: {}", path.display()));
        }
    }

    Ok(paths)
}

fn merge_config_files(base: ConfigFile, overlay: ConfigFile) -> ConfigFile {
    ConfigFile {
        connectivity: merge_connectivity_config(base.connectivity, overlay.connectivity),
        speed: merge_speed_config(base.speed, overlay.speed),
        notifications: merge_notification_config(base.notifications, overlay.notifications),
        storage: merge_storage_config(base.storage, overlay.storage),
    }
}

fn merge_connectivity_config(
    base: Option<ConnectivityConfigFile>,
    overlay: Option<ConnectivityConfigFile>,
) -> Option<ConnectivityConfigFile> {
    match (base, overlay) {
        (None, None) => None,
        (Some(value), None) | (None, Some(value)) => Some(value),
        (Some(base), Some(overlay)) => Some(ConnectivityConfigFile {
            delay_secs: overlay.delay_secs.or(base.delay_secs),
            timeout_secs: overlay.timeout_secs.or(base.timeout_secs),
            outage_backoff_secs: overlay.outage_backoff_secs.or(base.outage_backoff_secs),
            outage_backoff_max_secs: overlay
                .outage_backoff_max_secs
                .or(base.outage_backoff_max_secs),
            urls: overlay.urls.or(base.urls),
            url_mode: overlay.url_mode.or(base.url_mode),
        }),
    }
}

fn merge_speed_config(
    base: Option<SpeedConfigFile>,
    overlay: Option<SpeedConfigFile>,
) -> Option<SpeedConfigFile> {
    match (base, overlay) {
        (None, None) => None,
        (Some(value), None) | (None, Some(value)) => Some(value),
        (Some(base), Some(overlay)) => Some(SpeedConfigFile {
            expected_download_mbps: overlay
                .expected_download_mbps
                .or(base.expected_download_mbps),
            expected_upload_mbps: overlay.expected_upload_mbps.or(base.expected_upload_mbps),
            delay_secs: overlay.delay_secs.or(base.delay_secs),
            download_thresholds: overlay.download_thresholds.or(base.download_thresholds),
            upload_thresholds: overlay.upload_thresholds.or(base.upload_thresholds),
            speedtest_cli_path: overlay.speedtest_cli_path.or(base.speedtest_cli_path),
        }),
    }
}

fn merge_notification_config(
    base: Option<NotificationConfigFile>,
    overlay: Option<NotificationConfigFile>,
) -> Option<NotificationConfigFile> {
    match (base, overlay) {
        (None, None) => None,
        (Some(value), None) | (None, Some(value)) => Some(value),
        (Some(base), Some(overlay)) => Some(NotificationConfigFile {
            telegram: merge_telegram_config(base.telegram, overlay.telegram),
            otel_endpoint: overlay.otel_endpoint.or(base.otel_endpoint),
            min_download_threshold: overlay
                .min_download_threshold
                .or(base.min_download_threshold),
            min_upload_threshold: overlay.min_upload_threshold.or(base.min_upload_threshold),
        }),
    }
}

fn merge_telegram_config(
    base: Option<TelegramConfigFile>,
    overlay: Option<TelegramConfigFile>,
) -> Option<TelegramConfigFile> {
    match (base, overlay) {
        (None, None) => None,
        (Some(value), None) | (None, Some(value)) => Some(value),
        (Some(base), Some(overlay)) => Some(TelegramConfigFile {
            bot_token: overlay.bot_token.or(base.bot_token),
            chat_id: overlay.chat_id.or(base.chat_id),
        }),
    }
}

fn merge_storage_config(
    base: Option<StorageConfigFile>,
    overlay: Option<StorageConfigFile>,
) -> Option<StorageConfigFile> {
    match (base, overlay) {
        (None, None) => None,
        (Some(value), None) | (None, Some(value)) => Some(value),
        (Some(base), Some(overlay)) => Some(StorageConfigFile {
            db_path: overlay.db_path.or(base.db_path),
            cleanup_enabled: overlay.cleanup_enabled.or(base.cleanup_enabled),
            cleanup_interval_days: overlay.cleanup_interval_days.or(base.cleanup_interval_days),
        }),
    }
}

fn build_config(
    config_file: Option<ConfigFile>,
    args: &NetQualityCliArgs,
) -> Result<NetQualityConfig> {
    let mut connectivity = build_connectivity_config(
        config_file
            .as_ref()
            .and_then(|cfg| cfg.connectivity.clone()),
        args,
    )?;
    let speed = build_speed_config(config_file.as_ref().and_then(|cfg| cfg.speed.clone()), args)?;
    let notifications = build_notification_config(
        config_file
            .as_ref()
            .and_then(|cfg| cfg.notifications.clone()),
        args,
    )?;
    let storage = build_storage_config(
        config_file.as_ref().and_then(|cfg| cfg.storage.clone()),
        args,
    )?;

    connectivity.urls = dedupe_urls(connectivity.urls);
    if connectivity.urls.is_empty() {
        return Err(anyhow!("Connectivity URL list cannot be empty."));
    }

    Ok(NetQualityConfig {
        connectivity,
        speed,
        notifications,
        storage,
    })
}

fn build_connectivity_config(
    config_file: Option<ConnectivityConfigFile>,
    args: &NetQualityCliArgs,
) -> Result<ConnectivityConfig> {
    let delay_secs = args
        .connectivity_delay_secs
        .or_else(|| config_file.as_ref().and_then(|cfg| cfg.delay_secs))
        .unwrap_or(DEFAULT_CONNECTIVITY_DELAY_SECS);
    let timeout_secs = args
        .connectivity_timeout_secs
        .or_else(|| config_file.as_ref().and_then(|cfg| cfg.timeout_secs))
        .unwrap_or(DEFAULT_CONNECTIVITY_TIMEOUT_SECS);
    let outage_backoff_secs = args
        .outage_backoff_secs
        .or_else(|| config_file.as_ref().and_then(|cfg| cfg.outage_backoff_secs))
        .unwrap_or(DEFAULT_OUTAGE_BACKOFF_SECS);
    let outage_backoff_max_secs = args
        .outage_backoff_max_secs
        .or_else(|| {
            config_file
                .as_ref()
                .and_then(|cfg| cfg.outage_backoff_max_secs)
        })
        .unwrap_or(DEFAULT_OUTAGE_BACKOFF_MAX_SECS);

    if delay_secs == 0 || timeout_secs == 0 {
        return Err(anyhow!(
            "Connectivity delay/timeout must be greater than zero."
        ));
    }

    if outage_backoff_secs == 0 || outage_backoff_max_secs == 0 {
        return Err(anyhow!("Outage backoff values must be greater than zero."));
    }

    if outage_backoff_max_secs < outage_backoff_secs {
        return Err(anyhow!("Outage backoff max must be >= outage backoff."));
    }

    let url_mode = if args.replace_urls {
        UrlMode::Replace
    } else {
        config_file
            .as_ref()
            .and_then(|cfg| cfg.url_mode)
            .unwrap_or(UrlMode::Merge)
    };

    let user_urls = if !args.urls.is_empty() {
        args.urls.clone()
    } else {
        config_file
            .as_ref()
            .and_then(|cfg| cfg.urls.clone())
            .unwrap_or_default()
    };

    let mut urls: Vec<String> = match url_mode {
        UrlMode::Merge => DEFAULT_URLS
            .iter()
            .map(|url| url.to_string())
            .chain(user_urls)
            .collect(),
        UrlMode::Replace => user_urls,
    };

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

fn build_speed_config(
    config_file: Option<SpeedConfigFile>,
    args: &NetQualityCliArgs,
) -> Result<SpeedConfig> {
    let expected_download = args
        .expected_download_mbps
        .or_else(|| {
            config_file
                .as_ref()
                .and_then(|cfg| cfg.expected_download_mbps)
        })
        .ok_or_else(|| anyhow!("Expected download speed is required."))?;

    if expected_download <= 0.0 {
        return Err(anyhow!(
            "Expected download speed must be greater than zero."
        ));
    }

    let expected_upload = args.expected_upload_mbps.or_else(|| {
        config_file
            .as_ref()
            .and_then(|cfg| cfg.expected_upload_mbps)
    });

    if let Some(upload) = expected_upload {
        if upload <= 0.0 {
            return Err(anyhow!("Expected upload speed must be greater than zero."));
        }
    }

    let delay_secs = args
        .speed_delay_secs
        .or_else(|| config_file.as_ref().and_then(|cfg| cfg.delay_secs))
        .unwrap_or(DEFAULT_SPEED_DELAY_SECS);

    if delay_secs == 0 {
        return Err(anyhow!("Speed delay must be greater than zero."));
    }

    let download_thresholds = args
        .download_thresholds
        .clone()
        .or_else(|| {
            config_file
                .as_ref()
                .and_then(|cfg| cfg.download_thresholds.clone())
        })
        .unwrap_or_else(Thresholds::default_thresholds);

    download_thresholds
        .validate()
        .context("Invalid download thresholds")?;

    let upload_thresholds = args
        .upload_thresholds
        .clone()
        .or_else(|| {
            config_file
                .as_ref()
                .and_then(|cfg| cfg.upload_thresholds.clone())
        })
        .unwrap_or_else(Thresholds::default_thresholds);

    upload_thresholds
        .validate()
        .context("Invalid upload thresholds")?;

    let speedtest_cli_path = args.speedtest_cli_path.clone().or_else(|| {
        config_file
            .as_ref()
            .and_then(|cfg| cfg.speedtest_cli_path.clone())
    });

    if let Some(path) = &speedtest_cli_path {
        if !path.exists() {
            return Err(anyhow!(
                "Speedtest CLI binary not found: {}",
                path.display()
            ));
        }
    }

    Ok(SpeedConfig {
        expected_download_mbps: expected_download,
        expected_upload_mbps: expected_upload,
        delay: Duration::from_secs(delay_secs),
        download_thresholds,
        upload_thresholds,
        speedtest_cli_path,
    })
}

fn build_notification_config(
    config_file: Option<NotificationConfigFile>,
    args: &NetQualityCliArgs,
) -> Result<NotificationConfig> {
    let telegram = resolve_telegram_config(
        config_file.as_ref().and_then(|cfg| cfg.telegram.clone()),
        args,
    )?;

    let otel_endpoint = args.otel_endpoint.clone().or_else(|| {
        config_file
            .as_ref()
            .and_then(|cfg| cfg.otel_endpoint.clone())
    });

    let min_download_threshold = args
        .min_download_notification_threshold
        .or_else(|| {
            config_file
                .as_ref()
                .and_then(|cfg| cfg.min_download_threshold)
        })
        .unwrap_or(DEFAULT_MIN_DOWNLOAD_NOTIFY_THRESHOLD);

    let min_upload_threshold = args
        .min_upload_notification_threshold
        .or_else(|| {
            config_file
                .as_ref()
                .and_then(|cfg| cfg.min_upload_threshold)
        })
        .unwrap_or(DEFAULT_MIN_UPLOAD_NOTIFY_THRESHOLD);

    Ok(NotificationConfig {
        telegram,
        otel_endpoint,
        min_download_threshold,
        min_upload_threshold,
    })
}

fn resolve_telegram_config(
    config_file: Option<TelegramConfigFile>,
    args: &NetQualityCliArgs,
) -> Result<Option<TelegramConfig>> {
    let token = args
        .telegram_token
        .clone()
        .or_else(|| config_file.as_ref().and_then(|cfg| cfg.bot_token.clone()));
    let chat_id = args
        .telegram_chat_id
        .clone()
        .or_else(|| config_file.as_ref().and_then(|cfg| cfg.chat_id.clone()));

    match (token, chat_id) {
        (Some(token), Some(chat_id)) => Ok(Some(TelegramConfig {
            bot_token: token,
            chat_id,
        })),
        (None, None) => Ok(None),
        _ => Err(anyhow!(
            "Telegram configuration requires both bot_token and chat_id."
        )),
    }
}

fn build_storage_config(
    config_file: Option<StorageConfigFile>,
    args: &NetQualityCliArgs,
) -> Result<StorageConfig> {
    let config_file = config_file.as_ref();
    let db_path = match args
        .db_path
        .clone()
        .or_else(|| config_file.and_then(|cfg| cfg.db_path.clone()))
    {
        Some(path) => path,
        None => default_db_path()?,
    };

    let cleanup_enabled = config_file
        .and_then(|cfg| cfg.cleanup_enabled)
        .unwrap_or(DEFAULT_STORAGE_CLEANUP_ENABLED);
    let cleanup_interval_days = config_file
        .and_then(|cfg| cfg.cleanup_interval_days)
        .unwrap_or(DEFAULT_STORAGE_CLEANUP_INTERVAL_DAYS);

    if cleanup_interval_days == 0 {
        return Err(anyhow!(
            "Storage cleanup interval days must be greater than zero."
        ));
    }

    let cleanup_interval_secs = cleanup_interval_days
        .checked_mul(86_400)
        .ok_or_else(|| anyhow!("Storage cleanup interval days is too large."))?;

    Ok(StorageConfig {
        db_path,
        cleanup_enabled,
        cleanup_interval: Duration::from_secs(cleanup_interval_secs),
    })
}

fn default_db_path() -> Result<PathBuf> {
    let exe_path = std::env::current_exe()
        .context("Failed to resolve executable path for database default")?;
    let exe_dir = exe_path
        .parent()
        .context("Executable path has no parent directory")?;
    Ok(exe_dir.join("netquality.db"))
}

use crate::models::{
    dedupe_urls, ConfigFile, ConnectivityConfig, ConnectivityConfigFile, NetQualityCliArgs,
    NetQualityConfig, NotificationConfig, NotificationConfigFile, SpeedConfig, SpeedConfigFile,
    StorageConfig, StorageConfigFile, TelegramConfig, TelegramConfigFile, Thresholds, UrlMode,
    DEFAULT_URLS,
};
use anyhow::{anyhow, Context, Result};
use shared::system::load_json_file_to_object::load_json_file_to_object;
use std::path::{Path, PathBuf};
use std::time::Duration;

const DEFAULT_CONNECTIVITY_DELAY_SECS: u64 = 10;
const DEFAULT_SPEED_DELAY_SECS: u64 = 14_400;
const DEFAULT_CONNECTIVITY_TIMEOUT_SECS: u64 = 1;
const DEFAULT_OUTAGE_BACKOFF_SECS: u64 = 10;
const DEFAULT_OUTAGE_BACKOFF_MAX_SECS: u64 = 3_600;

pub(super) async fn load_config(args: &NetQualityCliArgs) -> Result<(NetQualityConfig, String)> {
    let config_path = resolve_config_path(args)?;
    let mut config_label = "defaults".to_string();
    let config_file = if let Some(path) = config_path {
        config_label = path.display().to_string();
        Some(load_json_file_to_object::<ConfigFile>(&path).await?)
    } else {
        None
    };

    let config = build_config(config_file, args)?;
    Ok((config, config_label))
}

fn resolve_config_path(args: &NetQualityCliArgs) -> Result<Option<PathBuf>> {
    if let Some(path) = &args.config_path {
        if path.exists() {
            return Ok(Some(path.clone()));
        }
        return Err(anyhow!("Config file not found: {}", path.display()));
    }

    let cwd_path = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("config.json");
    if cwd_path.exists() {
        return Ok(Some(cwd_path));
    }

    let exe_path = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .map(|dir| dir.join("config.json"));
    if let Some(path) = exe_path {
        if path.exists() {
            return Ok(Some(path));
        }
    }

    Ok(None)
}

fn build_config(config_file: Option<ConfigFile>, args: &NetQualityCliArgs) -> Result<NetQualityConfig> {
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
        .map_err(|error| anyhow!("Invalid download thresholds: {error}"))?;

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
        .map_err(|error| anyhow!("Invalid upload thresholds: {error}"))?;

    Ok(SpeedConfig {
        expected_download_mbps: expected_download,
        expected_upload_mbps: expected_upload,
        delay: Duration::from_secs(delay_secs),
        download_thresholds,
        upload_thresholds,
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

    Ok(NotificationConfig {
        telegram,
        otel_endpoint,
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
    let db_path = match args
        .db_path
        .clone()
        .or_else(|| config_file.and_then(|cfg| cfg.db_path))
    {
        Some(path) => path,
        None => default_db_path()?,
    };

    Ok(StorageConfig { db_path })
}

fn default_db_path() -> Result<PathBuf> {
    let exe_path = std::env::current_exe()
        .context("Failed to resolve executable path for database default")?;
    let exe_dir = exe_path
        .parent()
        .context("Executable path has no parent directory")?;
    Ok(exe_dir.join("netquality.db"))
}

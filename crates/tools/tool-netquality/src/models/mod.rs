mod types;

pub use types::{
    dedupe_urls, ConfigFile, ConnectivityConfig, ConnectivityConfigFile, ConnectivityResult,
    NetQualityCliArgs, NetQualityConfig, NotificationConfig, NotificationConfigFile, OutageInfo,
    SpeedConfig, SpeedConfigFile, SpeedResult, StorageConfig, StorageConfigFile, TelegramConfig,
    TelegramConfigFile, ThresholdCategory, Thresholds, UrlMode, DEFAULT_URLS,
};

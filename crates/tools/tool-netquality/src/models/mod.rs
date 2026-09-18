mod types;

pub use types::{
    BotToken, ConnectivityConfig, ConnectivityResult, NetQualityConfig, NotificationConfig,
    OutageInfo, SpeedConfig, SpeedResult, StorageConfig, TelegramConfig, ThresholdCategory,
    Thresholds, UrlMode, resolve_urls,
};

// Only test code outside this module reads the default URL list directly.
#[cfg(test)]
pub use types::DEFAULT_URLS;

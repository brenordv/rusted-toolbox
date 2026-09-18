pub(crate) mod telegram_notifier;

use crate::models::{NetQualityConfig, NotificationConfig, OutageInfo, SpeedResult};
use anyhow::Result;
use chrono::Duration as ChronoDuration;
use tracing::{Instrument, info_span, trace, warn};

use self::telegram_notifier::TelegramNotifier;

pub(crate) struct Notifier {
    telegram: Option<TelegramNotifier>,
}

impl Notifier {
    pub(crate) fn new(config: &NotificationConfig) -> Result<Self> {
        let telegram = match &config.telegram {
            Some(telegram) => Some(TelegramNotifier::new(telegram)?),
            None => None,
        };

        Ok(Self { telegram })
    }

    pub(crate) async fn send_outage_end(
        &mut self,
        config: &NetQualityConfig,
        speed: &SpeedResult,
        outage: &OutageInfo,
    ) {
        let duration = outage.ended_at.signed_duration_since(outage.started_at);
        let duration_str = format_duration(duration);
        let upload = speed
            .upload_mbps
            .map(|value| format!("{:.2} Mbps", value))
            .unwrap_or_else(|| "n/a".to_string());

        let message = format!(
            "Outage ended.\nStart: {}\nEnd: {}\nDuration: {}\nDownload: {:.2} Mbps\nUpload: {}",
            outage.started_at.to_rfc3339(),
            outage.ended_at.to_rfc3339(),
            duration_str,
            speed.download_mbps,
            upload
        );

        self.send_message(config, &message).await;
    }

    pub(crate) async fn send_speed_change(
        &mut self,
        config: &NetQualityConfig,
        speed: &SpeedResult,
    ) {
        let mut message = format!(
            "Speed change detected.\nDownload: {:.2} Mbps ({})",
            speed.download_mbps,
            speed.download_threshold.label()
        );

        if let Some(upload) = speed.upload_mbps {
            let label = speed
                .upload_threshold
                .map(|threshold| threshold.label())
                .unwrap_or("Unknown");
            message.push_str(&format!("\nUpload: {:.2} Mbps ({})", upload, label));
        }

        self.send_message(config, &message).await;
    }

    /// Delivers one notification inside the `netquality.notification` span,
    /// which raccoon_otel's subscriber exports to the OTel collector. The
    /// span wraps the Telegram send and every diagnostic about it, attached
    /// with `Instrument` because a guard held across an `.await` would detach
    /// the async body from the span. The span is info-level: at the default
    /// `warn` log level it is disabled and the events export uncorrelated.
    pub(crate) async fn send_message(&mut self, config: &NetQualityConfig, message: &str) {
        let span = info_span!("netquality.notification", "notification.message" = message,);
        async {
            if let Some(telegram) = &self.telegram
                && let Err(error) = telegram.send(message).await
            {
                warn!("Failed to send Telegram notification: {error:#}");
            }

            trace!("Notification sent: {}", message);
            if config.notifications.telegram.is_none() {
                warn!("No notification channels configured; message only sent via tracing.");
            }
        }
        .instrument(span)
        .await;
    }

    pub(crate) fn shutdown(&self) {
        // OTel shutdown is handled by the OtelGuard in main.rs.
    }
}

fn format_duration(duration: ChronoDuration) -> String {
    let seconds = duration.num_seconds().max(0);
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let seconds = seconds % 60;
    format!("{hours}h {minutes}m {seconds}s")
}

use crate::models::{BotToken, TelegramConfig};
use anyhow::{Context, Result, anyhow};
use reqwest::Client;

pub(crate) struct TelegramNotifier {
    client: Client,
    bot_token: BotToken,
    chat_id: String,
}

impl TelegramNotifier {
    pub(crate) fn new(config: &TelegramConfig) -> Result<Self> {
        Ok(Self {
            client: Client::new(),
            bot_token: config.bot_token.clone(),
            chat_id: config.chat_id.clone(),
        })
    }

    /// Sends `message` to the configured chat.
    ///
    /// The request URL embeds the bot token, so transport errors must never
    /// carry the URL into the anyhow chain: the chain reaches `warn!` and the
    /// OTel exporter in `notifiers/mod.rs`. `without_url` strips it.
    ///
    /// # Errors
    /// Returns a token-free error when the request fails or Telegram answers
    /// with a non-success status.
    pub(crate) async fn send(&self, message: &str) -> Result<()> {
        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.bot_token.expose()
        );

        let payload = serde_json::json!({
            "chat_id": self.chat_id,
            "text": message
        });

        let response = self
            .client
            .post(url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| anyhow::Error::new(e.without_url()))
            .context("Telegram sendMessage request failed")?;
        if !response.status().is_success() {
            return Err(anyhow!("Telegram response status {}", response.status()));
        }

        Ok(())
    }
}

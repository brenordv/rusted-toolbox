use crate::models::TelegramConfig;
use anyhow::{anyhow, Result};
use reqwest::Client;

pub(crate) struct TelegramNotifier {
    client: Client,
    bot_token: String,
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

    pub(crate) async fn send(&self, message: &str) -> Result<()> {
        let url = format!("https://api.telegram.org/bot{}/sendMessage", self.bot_token);

        let payload = serde_json::json!({
            "chat_id": self.chat_id,
            "text": message
        });

        let response = self.client.post(url).json(&payload).send().await?;
        if !response.status().is_success() {
            return Err(anyhow!("Telegram response status {}", response.status()));
        }

        Ok(())
    }
}

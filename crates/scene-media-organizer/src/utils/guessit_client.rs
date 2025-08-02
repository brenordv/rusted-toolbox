use crate::models::guessit_response::GuessItResponse;
use anyhow::Result;
use reqwest::Client;
use std::sync::OnceLock;

// Global HTTP client for connection pooling
static HTTP_CLIENT: OnceLock<Client> = OnceLock::new();

fn get_http_client() -> &'static Client {
    HTTP_CLIENT.get_or_init(|| {
        Client::builder()
            .pool_max_idle_per_host(10)
            .pool_idle_timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client")
    })
}

pub struct GuessItClient {
    base_url: String,
}

impl GuessItClient {
    pub fn new(base_url: String) -> Self {
        Self { base_url }
    }

    pub async fn it(&self, filename: String) -> Result<GuessItResponse> {
        let url = format!("{}?it={}", self.base_url, filename);

        let client = get_http_client();

        let response = client.get(&url).send().await?;

        let guessit_response: GuessItResponse = response.json().await?;

        Ok(guessit_response)
    }
}

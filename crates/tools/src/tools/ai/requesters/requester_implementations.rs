use crate::tools::ai::models::models::AiResponse;
use crate::tools::ai::models::open_ai::{ApiResponse, ChatCompletion, Message};
use crate::tools::ai::requesters::requester_traits::{AiRequesterTraits, MessageVecExt};
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::Client;

pub struct AiRequester {
    system_message: Option<Message>,
    message_history: Vec<Message>,
    current_payload: Vec<Message>,
    current_model: String,
    api_url: String,
    api_key: String,
    api_org: Option<String>,
    headers: HeaderMap,
    user_role: String,
}

impl AiRequester {
    pub fn new(api_url: String, api_key: String, api_org: Option<String>) -> Self {
        Self {
            system_message: None,
            message_history: vec![],
            current_payload: vec![],
            current_model: "".to_string(),
            api_url,
            api_key,
            api_org,
            headers: Default::default(),
            user_role: "user".to_string(),
        }
    }
}

#[async_trait]
impl AiRequesterTraits for AiRequester {
    fn change_model(&mut self, model: &str) -> Result<&mut Self> {
        if model.is_empty() {
            anyhow::bail!("Model cannot be empty.");
        }

        self.current_model = model.to_string();

        Ok(self)
    }

    fn set_user_role(&mut self, role: &str) -> Result<&mut Self> {
        if role.is_empty() {
            anyhow::bail!("Role cannot be empty.");
        }

        self.user_role = role.to_string();

        Ok(self)
    }

    fn build_headers(&mut self) -> Result<&mut Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_str(&format!("Bearer {}", self.api_key))
                .context("Failed to build authorization header")?,
        );

        if let Some(org) = &self.api_org {
            headers.insert(
                "OpenAI-Organization",
                HeaderValue::from_str(org).context("Failed to build organization header")?,
            );
        }

        self.headers = headers;
        Ok(self)
    }

    fn build_system_message(&mut self, system_message: String) -> Result<&mut Self> {
        if system_message.is_empty() {
            anyhow::bail!("System message cannot be empty.");
        }

        self.system_message = Some(Message {
            role: "system".to_string(),
            content: system_message,
        });

        Ok(self)
    }

    fn build_request_payload(&mut self, new_message: String) -> &mut Self {
        if !self.current_payload.first_is_system() {
            if let Some(system_message) = &self.system_message {
                self.current_payload.insert(0, system_message.clone());
            }
        }
        // Add the new message at the end (if that's your logic)
        let message = Message {
            role: self.user_role.clone(),
            content: new_message,
        };

        self.current_payload.push(message.clone());

        self.current_payload.push(message);

        self
    }

    async fn send_request(&mut self) -> Result<AiResponse> {
        let headers = self.headers.clone();

        let client = Client::builder().default_headers(headers).build()?;

        let chat_completion = ChatCompletion {
            model: self.current_model.clone(),
            messages: self.current_payload.clone(),
            temperature: 0.1,
        };

        let api_response = client
            .post(&self.api_url)
            .json(&chat_completion)
            .send()
            .await
            .context("Failed to send request")?;

        let status_code = (&api_response).status();

        let success = status_code == 200;

        if !success {
            let error_text = api_response
                .text()
                .await
                .context("Failed to parse error response")?;

            anyhow::bail!("Error [{}]: {}", status_code, error_text);
        }

        let raw_text = api_response
            .text()
            .await
            .context("Failed to parse error response")?;

        let api_response_obj: ApiResponse = serde_json::from_str(&raw_text)
            .context("Failed to parse response")?;

        let ai_response = api_response_obj
            .choices
            .get(0)
            .map(|choice| choice.message.content.clone())
            .context("No response returned")?;

        let ai_response_message = Message {
            role: "assistant".to_string(),
            content: ai_response.clone(),
        };

        self.current_payload.push(ai_response_message.clone());

        self.message_history.push(ai_response_message);

        Ok(AiResponse {
            success,
            message: ai_response,
        })
    }
}

impl MessageVecExt for [Message] {
    fn first_is_system(&self) -> bool {
        self.get(0).map(|m| m.role == "system").unwrap_or(false)
    }
}
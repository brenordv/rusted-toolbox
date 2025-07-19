use crate::tools::ai::models::models::AiResponse;
use crate::tools::ai::models::open_ai::{ApiResponse, ChatCompletion, Message};
use crate::tools::ai::requesters::requester_traits::{MessageVecExt, OpenAiRequesterTraits};
use crate::tools::ai::utils::request_logger::RequestLogger;
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Client, Response, StatusCode};

pub struct OpenAiRequester {
    api_url: String,
    api_key: String,
    api_org: Option<String>,
    headers: HeaderMap,
    current_model: String,
    system_message: Option<Message>,
    message_history: Vec<Message>,
    temperature: f32,
    request_logger: RequestLogger,
    api_client: Option<Client>,
}

impl OpenAiRequester {
    const USER_ROLE: &'static str = "user";
    const SYSTEM_ROLE: &'static str = "system";
    const ASSISTANT_ROLE: &'static str = "assistant";

    pub fn new(
        api_url: String,
        api_key: String,
        api_org: Option<String>,
        temperature: Option<f32>,
        request_history_path: Option<String>,
    ) -> Result<Self> {
        let resolved_request_history_path =
            request_history_path.unwrap_or_else(|| ".request_history".to_string());

        Ok(Self {
            api_url,
            api_key,
            api_org,
            headers: Default::default(),
            current_model: "".to_string(),
            system_message: None,
            message_history: vec![],
            temperature: temperature.unwrap_or(1.0),
            request_logger: RequestLogger::new(resolved_request_history_path)?,
            api_client: None,
        })
    }

    fn build_request_payload(&mut self, new_message: String, use_history: bool) -> Vec<Message> {
        let new_user_message = Message {
            role: Self::USER_ROLE.to_string(),
            content: new_message,
        };

        if use_history {
            self.ensure_first_message_is_from_system_if_available();

            let mut full_history = self.message_history.clone();

            full_history.push(new_user_message);

            return full_history;
        };

        match &self.system_message {
            Some(system_message) => {
                vec![system_message.clone(), new_user_message]
            }
            None => {
                // Adding the system message with instructions is always a good idea.
                vec![new_user_message]
            }
        }
    }

    fn ensure_first_message_is_from_system_if_available(&mut self) {
        if self.message_history.first_is_system() {
            return;
        }

        if let Some(system_message) = &self.system_message {
            self.message_history.insert(0, system_message.clone());
        }
    }

    fn save_user_request_to_message_history(&mut self, payload: &Vec<Message>) -> Result<()> {
        if payload.is_empty() {
            anyhow::bail!("Payload cannot be empty.");
        }

        self.message_history.push(payload.last().unwrap().clone());

        Ok(())
    }

    fn build_openai_request(&mut self, payload: Vec<Message>) -> ChatCompletion {
        ChatCompletion {
            model: self.current_model.clone(),
            messages: payload,
            temperature: format!("{:.1}", self.temperature),
        }
    }

    fn build_api_client(&mut self) -> Result<()> {
        let api_client = Client::builder()
            .default_headers(self.headers.clone())
            .build()
            .context("Failed to build API client")?;

        self.api_client = Some(api_client);

        Ok(())
    }

    async fn send_api_request(
        &mut self,
        chat_completion_request: &ChatCompletion,
    ) -> Result<(Response, StatusCode, bool)> {
        let api_client = match &self.api_client {
            Some(api_client) => api_client,
            None => {
                anyhow::bail!("API client is not built. Please build it first.");
            }
        };

        let api_response = api_client
            .post(&self.api_url)
            .json(&chat_completion_request)
            .send()
            .await
            .context("Failed to send request")?;

        let status_code = (&api_response).status();
        Ok((api_response, status_code, status_code == 200))
    }

    async fn extract_response_text(api_response: Response) -> Result<String> {
        api_response
            .text()
            .await
            .context("Failed to parse error response")
    }

    fn extract_ai_response_from_text(raw_text_response: &String) -> Result<Message> {
        let api_response_obj: ApiResponse =
            serde_json::from_str(&raw_text_response).context("Failed to parse response")?;

        let ai_response = api_response_obj
            .choices
            .get(0)
            .map(|choice| choice.message.content.clone())
            .context("No response returned")?;

        Ok(Message {
            role: Self::ASSISTANT_ROLE.to_string(),
            content: ai_response.clone(),
        })
    }

    fn save_ai_response_to_message_history(&mut self, ai_response: &Message) {
        self.message_history.push(ai_response.clone());
    }

    fn build_headers(&mut self) -> Result<()> {
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

        Ok(())
    }
}

#[async_trait]
impl OpenAiRequesterTraits for OpenAiRequester {
    fn set_model(&mut self, model: &str) -> Result<&mut Self> {
        if model.is_empty() {
            anyhow::bail!("Model cannot be empty.");
        }

        self.current_model = model.to_string();

        Ok(self)
    }

    fn set_temperature(&mut self, temperature: &f32) -> Result<&mut Self> {
        self.temperature = *temperature;

        Ok(self)
    }

    fn initialize_api_client(&mut self) -> Result<&mut Self> {
        self.build_headers()?;

        self.build_api_client()?;

        Ok(self)
    }

    fn set_system_message(&mut self, system_message: String) -> Result<&mut Self> {
        if system_message.is_empty() {
            anyhow::bail!("System message cannot be empty.");
        }

        self.system_message = Some(Message {
            role: Self::SYSTEM_ROLE.to_string(),
            content: system_message,
        });

        Ok(self)
    }

    async fn send_request(&mut self, new_message: String, use_history: bool) -> Result<AiResponse> {
        let payload = self.build_request_payload(new_message, use_history);

        self.save_user_request_to_message_history(&payload)?;

        let chat_completion_request = self.build_openai_request(payload);

        self.request_logger.set_request_timestamp_local();

        self.request_logger.save_request(&chat_completion_request)?;

        let (api_response, status_code, success) =
            self.send_api_request(&chat_completion_request).await?;

        // Instead of immediately parsing the response, I'm getting the text so we know what went
        // wrong in case of failure.
        let raw_text_response = Self::extract_response_text(api_response).await?;

        self.request_logger
            .save_response(&raw_text_response, status_code.as_u16())?;

        if !success {
            anyhow::bail!("Error [{}]: {}", status_code, raw_text_response);
        }

        let ai_response = Self::extract_ai_response_from_text(&raw_text_response)?;

        self.save_ai_response_to_message_history(&ai_response);

        Ok(AiResponse {
            success,
            message: ai_response.content,
        })
    }
}

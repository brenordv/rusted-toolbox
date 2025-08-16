use anyhow::Result;
use async_trait::async_trait;
use crate::models::{AiResponse, Message};

pub trait MessageVecExt {
    fn first_is_system(&self) -> bool;
}

impl MessageVecExt for [Message] {
    fn first_is_system(&self) -> bool {
        self.get(0).map(|m| m.role == "system").unwrap_or(false)
    }
}

#[async_trait]
pub trait OpenAiRequesterTraits {
    fn set_model(&mut self, model: &str) -> Result<&mut Self>;
    fn set_temperature(&mut self, temperature: &f32) -> Result<&mut Self>;
    fn initialize_api_client(&mut self) -> Result<&mut Self>;
    fn set_system_message(&mut self, system_message: String) -> Result<&mut Self>;
    async fn send_request(&mut self, new_message: String, use_history: bool) -> Result<AiResponse>;
}

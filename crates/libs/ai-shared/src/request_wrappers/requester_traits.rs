use crate::models::{AiResponse, Message};
use anyhow::Result;
use async_trait::async_trait;

pub trait MessageVecExt {
    fn first_is_system(&self) -> bool;
}

impl MessageVecExt for [Message] {
    fn first_is_system(&self) -> bool {
        self.first().map(|m| m.role == "system").unwrap_or(false)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn message(role: &str) -> Message {
        Message {
            role: role.to_string(),
            content: "x".to_string(),
        }
    }

    #[test]
    fn first_is_system_true_when_first_role_is_system() {
        let messages = [message("system"), message("user")];

        assert!(messages.first_is_system());
    }

    #[test]
    fn first_is_system_false_when_first_role_is_user() {
        let messages = [message("user"), message("system")];

        assert!(!messages.first_is_system());
    }

    #[test]
    fn first_is_system_false_when_empty() {
        let messages: [Message; 0] = [];

        assert!(!messages.first_is_system());
    }
}

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Serialize, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct ChatCompletion {
    pub model: String,
    pub messages: Vec<Message>,
    pub temperature: f32,
}

#[derive(Debug, Deserialize)]
pub struct ApiMessage {
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct ApiChoice {
    pub message: ApiMessage,
}

#[derive(Debug, Deserialize)]
pub struct ApiResponse {
    pub choices: Vec<ApiChoice>,
    pub usage: Option<HashMap<String, Value>>,
}

#[derive(Debug, Deserialize)]
pub struct AiResponse {
    pub success: bool,
    pub message: String,
}

impl AiResponse {
    pub fn new_empty(success: bool) -> AiResponse {
        AiResponse {
            success,
            message: String::new(),
        }
    }

    pub fn new(success: bool, message: String) -> AiResponse {
        AiResponse { success, message }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ai_response_new_empty_has_empty_message() {
        let response = AiResponse::new_empty(true);

        assert!(response.success);
        assert!(response.message.is_empty());
    }

    #[test]
    fn ai_response_new_sets_fields() {
        let response = AiResponse::new(false, "boom".to_string());

        assert!(!response.success);
        assert_eq!(response.message, "boom");
    }

    #[test]
    fn message_serializes_role_and_content() {
        let message = Message {
            role: "user".to_string(),
            content: "hello".to_string(),
        };

        let json = serde_json::to_string(&message).unwrap();

        assert!(json.contains("\"role\":\"user\""));
        assert!(json.contains("\"content\":\"hello\""));
    }

    #[test]
    fn chat_completion_serializes_expected_fields() {
        let completion = ChatCompletion {
            model: "gpt-x".to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: "hi".to_string(),
            }],
            temperature: 0.5,
        };

        let json = serde_json::to_string(&completion).unwrap();

        assert!(json.contains("\"model\":\"gpt-x\""));
        assert!(json.contains("\"temperature\":0.5"));
        assert!(json.contains("\"messages\""));
    }

    #[test]
    fn api_response_deserializes_choice_content() {
        let raw = r#"{"choices":[{"message":{"content":"answer"}}],"usage":{"total_tokens":5}}"#;

        let parsed: ApiResponse = serde_json::from_str(raw).unwrap();

        assert_eq!(parsed.choices.len(), 1);
        assert_eq!(parsed.choices[0].message.content, "answer");
        assert!(parsed.usage.is_some());
    }

    #[test]
    fn api_response_allows_missing_usage() {
        let raw = r#"{"choices":[]}"#;

        let parsed: ApiResponse = serde_json::from_str(raw).unwrap();

        assert!(parsed.choices.is_empty());
        assert!(parsed.usage.is_none());
    }
}

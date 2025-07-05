use crate::tools::ai::models::models::AiResponse;
use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait AiRequesterTraits {
    fn change_model(&mut self, model: &str) -> Result<&mut Self>;
    fn set_user_role(&mut self, role: &str) -> Result<&mut Self>;
    fn build_headers(&mut self) -> Result<&mut Self>;
    fn build_system_message(&mut self, system_message: String) -> Result<&mut Self>;
    fn build_request_payload(&mut self, new_message: String) -> &mut Self;
    async fn send_request(&mut self) -> Result<AiResponse>;
}

pub trait MessageVecExt {
    fn first_is_system(&self) -> bool;
}

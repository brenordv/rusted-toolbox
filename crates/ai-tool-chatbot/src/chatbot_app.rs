use crate::cli_utils::get_user_response;
use crate::models::ChatBotAgent;
use ai_shared::models::AiResponse;
use ai_shared::request_wrappers::requester_builders::build_requester_for_ai;
use ai_shared::request_wrappers::requester_traits::OpenAiRequesterTraits;
use anyhow::{Context, Result};
use tracing::error;

pub async fn start_chatbot(options: ChatBotAgent) -> Result<()> {
    let mut requester = build_requester_for_ai()?;

    requester
        .initialize_api_client()
        .context("Failed to initialize API client")?
        .set_system_message(options.ai_personality.clone())
        .context("Failed to set system message")?;

    let mut ai_response = AiResponse::new_empty(true);

    let user_printer = options.user_printer;
    let ai_printer = options.agent_printer;

    if let Some(first_message_to_ai) = options.first_message_to_ai {
        ai_response = requester.send_request(first_message_to_ai, true).await?;
        let _ = &ai_printer.print(ai_response.message.to_string());
    }

    while ai_response.success {
        user_printer.print_tag();
        let user_request = get_user_response(true);

        ai_response = requester
            .send_request(format!("The user replied: {}", user_request), true)
            .await?;

        if !ai_response.success {
            error!("Request to AI failed! {}", ai_response.message);
            break;
        }

        ai_printer.print(ai_response.message.to_string());
    }

    Ok(())
}

use anyhow::{Context, Result};
use dotenv::dotenv;
use rusted_toolbox::tools::ai::models::models::Role;
use rusted_toolbox::tools::ai::requesters::requester_implementations::OpenAiRequester;
use rusted_toolbox::tools::ai::requesters::requester_traits::OpenAiRequesterTraits;
use rusted_toolbox::tools::ai::utils::get_user_response::get_user_response;
use rusted_toolbox::tools::ai::utils::load_chat_personality_prompt::load_chat_personality_prompt;
use rusted_toolbox::tools::ai::utils::printer::RolePrinter;
use std::env;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();

    println!("Rusted Agents: Chat");
    let user_name = env::var("AI_CHAT_USER_NAME").unwrap_or_else(|_| {
        println!("What is your name?");
        get_user_response(true)
    });

    let request_history_path = match env::var("AI_CHAT_REQUEST_HISTORY_PATH") {
        Ok(path) => Some(path),
        Err(_) => None,
    };

    let personality_path = env::var("AI_CHAT_PERSONALITIES_FOLDER")
        .context("AI_CHAT_PERSONALITIES_FOLDER must be set")?;

    let (personality, personality_name) = load_chat_personality_prompt(personality_path.as_str())?;

    let ai_name = if personality_name.is_empty() {
        println!("What is the AI's name?");
        get_user_response(true)
    } else {
        personality_name.to_string()
    };

    let tag_padding_size = if user_name.len() > ai_name.len() {
        user_name.len()
    } else {
        ai_name.len()
    };

    let agent_printer = RolePrinter::new(Role::Agent, ai_name, Some(tag_padding_size));

    let user_printer = RolePrinter::new(Role::User, user_name, Some(tag_padding_size));

    let api_key = env::var("API_KEY").context("API_KEY must be set")?;

    let ai_model = env::var("AI_MODEL").context("AI_MODEL must be set")?;

    let api_url = env::var("API_URL").context("API_URL must be set")?;

    let mut requester = OpenAiRequester::new(api_url, api_key, None, None, request_history_path)?;

    requester
        .set_model(ai_model.as_str())?
        .set_temperature(&1.0)?
        .initialize_api_client()?
        .set_system_message(personality)?;

    let mut ai_response = requester
        .send_request("Its been a while since you talked to the user, but he just connected to the chat. You should say something. Remember: You are chatting with the user. Dive deep into your your role playing.".to_string(), true)
        .await?;

    agent_printer.print(ai_response.message.to_string());

    while ai_response.success {
        user_printer.print_tag();
        let user_request = get_user_response(true);

        ai_response = requester
            .send_request(format!("The user replied: {}", user_request), true)
            .await?;

        if !ai_response.success {
            eprintln!("{}", ai_response.message);
            break;
        }

        agent_printer.print(ai_response.message.to_string());
    }

    Ok(())
}

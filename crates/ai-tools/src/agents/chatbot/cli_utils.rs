use crate::agents::chatbot::models::ChatBotAgent;
use crate::models::models::Role;
use crate::utils::get_user_response::get_user_response;
use crate::utils::load_chat_personality_prompt::load_chat_personality_prompt;
use crate::utils::printer::RolePrinter;
use anyhow::{Context, Result};

use std::env;
use shared::constants::general::DASH_LINE;
use crate::constants::{AI_CHATBOT_VERSION};

/// Displays runtime configuration information.
///
/// Shows input file, headers, cleaning options, and default mappings.
pub fn print_runtime_info(args: &ChatBotAgent) {
    println!("💬 ChatBot v{}", AI_CHATBOT_VERSION);
    println!("{}", DASH_LINE);

    println!("🧍 User: {}", &args.user_name);
    println!("🤖 Ai: {}", &args.ai_name);

    println!();
}

pub fn get_runtime_config() -> Result<ChatBotAgent> {
    let user_name = env::var("AI_CHAT_USER_NAME").unwrap_or_else(|_| {
        println!("What is your name?");
        get_user_response(true)
    });

    let personality_path = env::var("AI_CHAT_PERSONALITIES_FOLDER")
        .context("AI_CHAT_PERSONALITIES_FOLDER must be set")?;

    let (personality, personality_name) = load_chat_personality_prompt(personality_path.as_str())?;

    let ai_name = if personality_name.is_empty() {
        println!("What is the AI's name?");
        get_user_response(true)
    } else {
        personality_name.to_string()
    };

    let tag_padding_size = if &user_name.len() > &ai_name.len() {
        &user_name.len()
    } else {
        &ai_name.len()
    };

    let agent_printer = RolePrinter::new(Role::Agent, ai_name.clone(), Some(*tag_padding_size));

    let user_printer = RolePrinter::new(Role::User, user_name.clone(), Some(*tag_padding_size));

    let first_message_to_ai = env::var("AI_CHAT_INITIAL_MSG_TO_AI").ok();

    Ok(ChatBotAgent {
        user_name,
        ai_name,
        ai_personality: personality,
        agent_printer,
        user_printer,
        first_message_to_ai,
    })
}

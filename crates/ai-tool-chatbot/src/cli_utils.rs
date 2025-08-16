
use anyhow::{Context, Result};


use shared::constants::general::DASH_LINE;
use std::env;
use std::io::stdin;
use std::path::PathBuf;
use shared::system::select_file_from_path::select_file_from_path;
use shared::utils::role_printer::{Role, RolePrinter};
use crate::models::ChatBotAgent;

/// Displays runtime configuration information.
///
/// Shows input file, headers, cleaning options, and default mappings.
pub fn print_runtime_info(args: &ChatBotAgent) {
    println!("💬 ChatBot v{}", env!("CARGO_PKG_VERSION"));
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

pub fn get_user_response(required: bool) -> String {
    let mut user_response = String::new();

    while user_response.is_empty() {
        stdin()
            .read_line(&mut user_response)
            .expect("Failed to read line");

        if !required && user_response.is_empty() {
            return String::new();
        }
    }

    user_response.trim().to_string()
}

pub fn load_chat_personality_prompt(personalities_path: &str) -> Result<(String, String)> {
    let selected_personality_file =
        select_file_from_path(personalities_path, "Select a personality")?;

    let personality_name = selected_personality_file
        .split('_')
        .last()
        .unwrap_or("")
        .split(".")
        .next()
        .unwrap_or("")
        .to_string();

    let personality_file_fullpath =
        PathBuf::from(personalities_path).join(selected_personality_file);

    Ok((
        std::fs::read_to_string(personality_file_fullpath)?,
        personality_name,
    ))
}
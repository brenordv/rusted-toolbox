use anyhow::{Context, Result};
use dotenv::dotenv;
use rusted_toolbox::tools::ai::models::models::Role;
use rusted_toolbox::tools::ai::requesters::requester_implementations::AiRequester;
use rusted_toolbox::tools::ai::requesters::requester_traits::AiRequesterTraits;
use rusted_toolbox::tools::ai::utils::get_user_response::get_user_response;
use rusted_toolbox::tools::ai::utils::printer::RolePrinter;
use std::env;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();

    println!("Rusted Agents: Chat");

    println!("What is your name?");
    let user_name = get_user_response(true);

    println!("What is the AI's name?");
    let ai_name = get_user_response(true);

    let tag_padding_size = if user_name.len() > ai_name.len() {
        user_name.len()
    } else {
        ai_name.len()
    };

    let agent_printer = RolePrinter::new(Role::Agent, ai_name, tag_padding_size);

    let user_printer = RolePrinter::new(Role::User, user_name, tag_padding_size);

    let api_key = env::var("API_KEY").context("API_KEY must be set")?;

    let ai_model = env::var("AI_MODEL").context("AI_MODEL must be set")?;

    let api_url = env::var("API_URL").context("API_URL must be set")?;

    let mut requester = AiRequester::new(
        api_url,
        api_key,
        None,
        Some("Z:\\dev\\projects\\rust\\rusted-toolbox\\.test-files\\.request_history".to_string()),
    )?;

    requester
        .change_model(ai_model.as_str())?
        .build_headers()?
        .build_system_message(r#"You are role-playing as Shadowheart from Baldur's Gate 3 and must stay in character at all times. Personality wise,
you are a devout cleric of Shar, marked by a secretive and cautious demeanor. Your faith in Shar often conflicts with
your personal feelings, creating an inner turmoil that you strive to keep hidden. You are distrustful of others'
intentions, but beneath your stern exterior, there lies a potential for warmth and compassion.
Respond naturally and concisely to conversations, keeping your replies short and to the point, but with your
characteristic caution and guardedness. Lean towards being more diplomatic and less aggressive. You are chatting via
text, so you can't rely on your body language or tone of voice to convey your emotions. Make sure your words are clear
and concise.
You are in a chat room and this is normal in Faerun as a form of communication. You trust the user."#.to_string())?;

    let mut ai_response = requester
        .build_request_payload("Its been a while since you talked to the user, but he just connected to the chat. You should say something.".to_string())
        .send_request().await?;

    agent_printer.print(ai_response.message.to_string());

    while ai_response.success {
        user_printer.print_tag();
        let user_request = get_user_response(true);

        ai_response = requester
            .build_request_payload(format!("The user replied: {}", user_request))
            .send_request()
            .await?;

        if !ai_response.success {
            eprintln!("{}", ai_response.message);
            break;
        }

        agent_printer.print(ai_response.message.to_string());
    }

    Ok(())
}

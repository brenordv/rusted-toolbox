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
    let agent_printer = RolePrinter::new(Role::Agent, "Luna".to_string());
    let user_printer = RolePrinter::new(Role::User, "User".to_string());

    let api_key = env::var("OPEN_ROUTER_AI_KEY").context("OPENAI_API_KEY must be set")?;

    let mut requester = AiRequester::new("https://openrouter.ai/api/v1/chat/completions".to_string(), api_key, None);

    requester
        .change_model("openrouter/cypher-alpha:free")?
        .build_headers()?
        .build_system_message(r#""#.to_string())?;

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
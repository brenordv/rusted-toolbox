use crate::ai_functions::{fix_cli_command_as_string, suggest_cli_command_as_string};
use crate::models::{HowMode, HowRuntimeConfig};
use ai_shared::message_builders::system_message_builders::{
    build_rust_ai_function_system_message, build_rust_ai_function_user_message,
};
use ai_shared::request_wrappers::requester_builders::build_requester_for_ai;
use ai_shared::request_wrappers::requester_traits::OpenAiRequesterTraits;
use anyhow::{Context, Result};
use shared::utils::copy_string_to_clipboard::copy_to_clipboard;

/// Starts the How application with the given configuration.
///
/// This function handles both command fixing and command suggestion modes,
/// optionally copying results to clipboard.
///
/// # Arguments
/// * `config` - Runtime configuration containing mode, OS info, and options
///
/// # Errors
/// Returns error if AI request fails, clipboard operation fails, or other system errors
pub async fn start_how_app(config: HowRuntimeConfig) -> Result<()> {
    let result = match &config.mode {
        HowMode::FixCommand(command) => fix_command(command, &config.os, &config.shell).await?,
        HowMode::SuggestCommand(request) => {
            suggest_command(request, &config.os, &config.shell).await?
        }
    };

    // Print the result
    println!("{}", result);

    // Copy to clipboard if requested
    if config.copy_to_clipboard {
        copy_to_clipboard(&result)
            .map_err(|e| anyhow::anyhow!("Failed to copy result to clipboard: {}", e))?;
        eprintln!("✅ Copied to clipboard");
    }

    Ok(())
}

/// Fixes a potentially broken command using AI.
///
/// # Arguments
/// * `command` - The command to fix
/// * `os` - The target operating system
/// * `shell` - Optional shell information
///
/// # Errors
/// Returns error if AI request fails or returns invalid response
async fn fix_command(command: &str, os: &str, shell: &Option<String>) -> Result<String> {
    let os_info = build_os_info(os, shell);
    let input = format!("command: {}\n{}", command, os_info);

    let mut requester = build_requester_for_ai().context("Failed to build AI requester")?;

    let system_message = build_rust_ai_function_system_message();
    let user_message = build_rust_ai_function_user_message(fix_cli_command_as_string, &input);

    requester
        .set_system_message(system_message)?
        .initialize_api_client()?;

    let response = requester
        .send_request(user_message, false)
        .await
        .context("Failed to get AI response for command fix")?;

    if !response.success || response.message.trim().is_empty() {
        return Err(anyhow::anyhow!(
            "AI returned empty response. Please try again or report this issue."
        ));
    }

    Ok(response.message.trim().to_string())
}

/// Suggests a command based on natural language request using AI.
///
/// # Arguments
/// * `request` - The natural language request
/// * `os` - The target operating system
/// * `shell` - Optional shell information
///
/// # Errors
/// Returns error if AI request fails or returns invalid response
async fn suggest_command(request: &str, os: &str, shell: &Option<String>) -> Result<String> {
    let os_info = build_os_info(os, shell);
    let input = format!("request: {}\n: {}", request, os_info);

    let mut requester = build_requester_for_ai().context("Failed to build AI requester")?;

    let system_message = build_rust_ai_function_system_message();
    let user_message = build_rust_ai_function_user_message(suggest_cli_command_as_string, &input);

    requester
        .set_system_message(system_message)?
        .initialize_api_client()?;

    let response = requester
        .send_request(user_message, false)
        .await
        .context("Failed to get AI response for command suggestion")?;

    if !response.success || response.message.trim().is_empty() {
        return Err(anyhow::anyhow!(
            "AI returned empty response. Please try again or report this issue."
        ));
    }

    Ok(response.message.trim().to_string())
}

/// Builds OS information string for AI functions.
///
/// Combines OS name with shell information if available.
fn build_os_info(os: &str, shell: &Option<String>) -> String {
    match shell {
        Some(shell_name) => format!("OS: {} (shell: {})", os, shell_name),
        None => format!("OS: {}", os).to_string(),
    }
}

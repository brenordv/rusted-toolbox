use crate::ai_functions::convert_distro_command_as_string;
use crate::cli_utils::print_runtime_info;
use crate::command_parser::parse_command_parts;
use crate::distro_detect::{detect_target_distro, normalize_distro_name};
use crate::distro_map::convert_parts_with_map;
use crate::models::{DistroCcRuntimeConfig, DistroFamily};
use ai_shared::message_builders::system_message_builders::{
    build_rust_ai_function_system_message, build_rust_ai_function_user_message,
};
use ai_shared::request_wrappers::requester_builders::build_requester_for_ai;
use ai_shared::request_wrappers::requester_traits::OpenAiRequesterTraits;
use anyhow::{Context, Result};

pub async fn start_distro_cc_app(config: DistroCcRuntimeConfig) -> Result<()> {
    let from_family = normalize_distro_name(&config.from)
        .context("Invalid --from distro name. Please specify a known distro family.")?;

    let to_family = match config.to.as_ref() {
        Some(value) => normalize_distro_name(value)
            .context("Invalid --to distro name. Please specify a known distro family.")?,
        None => detect_target_distro()?,
    };

    if !config.no_header {
        print_runtime_info(
            from_family.as_str(),
            to_family.as_str(),
            &config.command,
            config.no_header,
            config.verbose,
        );
    }

    if config.verbose {
        eprintln!("Attempting conversion using internal map...");
    }

    let mapped_result = try_convert_with_map(from_family, to_family, &config.command, config.verbose)?;

    let final_result = if let Some(mapped) = mapped_result {
        mapped
    } else {
        if config.verbose {
            eprintln!("Internal map miss. Falling back to AI.");
        }
        convert_with_ai(from_family, to_family, &config.command).await?
    };

    if config.no_header {
        println!("{}", final_result);
    } else {
        println!("Result:");
        println!("{}", final_result);
    }

    Ok(())
}

fn try_convert_with_map(
    from: DistroFamily,
    to: DistroFamily,
    command: &str,
    verbose: bool,
) -> Result<Option<String>> {
    let parts = match parse_command_parts(command) {
        Ok(parts) => parts,
        Err(err) => {
            if verbose {
                eprintln!("Failed to parse command for map conversion: {}", err);
            }
            return Ok(None);
        }
    };

    Ok(convert_parts_with_map(from, to, &parts))
}

async fn convert_with_ai(
    from: DistroFamily,
    to: DistroFamily,
    command: &str,
) -> Result<String> {
    eprintln!("Warning: using AI fallback; the converted command may be imperfect.");
    let input = format!(
        "command: {}\nfrom_distro: {}\nto_distro: {}",
        command,
        from.as_str(),
        to.as_str()
    );

    let mut requester = build_requester_for_ai().context("Failed to build AI requester")?;

    let system_message = build_rust_ai_function_system_message();
    let user_message =
        build_rust_ai_function_user_message(convert_distro_command_as_string, &input);

    requester
        .set_system_message(system_message)?
        .initialize_api_client()?;

    let response = requester
        .send_request(user_message, false)
        .await
        .context("Failed to get AI response for distro conversion")?;

    if !response.success || response.message.trim().is_empty() {
        return Err(anyhow::anyhow!(
            "AI returned empty response. Please try again or report this issue."
        ));
    }

    Ok(response.message.trim().to_string())
}

use crate::models::{HowMode, HowRuntimeConfig};
use anyhow::Result;
use clap::{Arg, Command};
use shared::command_line::cli_builder::CommandExt;
use shared::constants::general::DASH_LINE;
use std::env;

/// Displays runtime configuration information.
pub fn print_runtime_info(config: &HowRuntimeConfig) {
    println!("🔧 How v{}", env!("CARGO_PKG_VERSION"));
    println!("{}", DASH_LINE);

    match &config.mode {
        HowMode::FixCommand(cmd) => {
            println!("🚨 Mode: Fix Command");
            println!("📝 Command: {}", cmd);
        }
        HowMode::SuggestCommand(request) => {
            println!("💡 Mode: Suggest Command");
            println!("📝 Request: {}", request);
        }
    }

    println!("💻 OS: {}", config.os);
    if let Some(shell) = &config.shell {
        println!("🐚 Shell: {}", shell);
    }
    println!("📋 Copy to clipboard: {}", config.copy_to_clipboard);
    println!();
}

/// Parses command-line arguments into How configuration.
///
/// # Errors
/// Returns error if arguments are invalid or conflicting
pub fn get_cli_arguments() -> Result<HowRuntimeConfig> {
    let matches = Command::new(env!("CARGO_PKG_NAME"))
        .add_basic_metadata(
            env!("CARGO_PKG_VERSION"),
            "A CLI tool that helps with command-line syntax",
            "This tool can fix broken commands or suggest commands from natural language requests. It automatically detects your OS and shell for accurate suggestions."
        )
        .arg(Arg::new("ask")
            .long("ask")
            .short('a')
            .value_name("REQUEST")
            .help("Natural language request for command suggestion (e.g., \"How to find files with bacon in the name\")"))
        .arg(Arg::new("copy")
            .long("copy")
            .short('c')
            .action(clap::ArgAction::SetTrue)
            .help("Copy the result to clipboard (Default: false)"))
        .arg(Arg::new("command")
            .help("Command to fix (if not using --ask)")
            .num_args(0..)
            .trailing_var_arg(true)
            .action(clap::ArgAction::Append))
        .get_matches();

    // Detect OS and shell
    let os = detect_os();
    let shell = detect_shell();

    // Determine mode based on arguments
    let mode = if let Some(request) = matches.get_one::<String>("ask") {
        HowMode::SuggestCommand(request.clone())
    } else if let Some(command_parts) = matches.get_many::<String>("command") {
        let command: Vec<String> = command_parts.cloned().collect();
        if command.is_empty() {
            return Err(anyhow::anyhow!(
                "No command provided. Use 'how <command>' to fix a command or 'how --ask \"request\"' to get suggestions."
            ));
        }
        HowMode::FixCommand(command.join(" "))
    } else {
        return Err(anyhow::anyhow!(
            "No command or request provided. Use 'how --help' for usage information."
        ));
    };

    let copy_to_clipboard = matches.get_flag("copy");

    Ok(HowRuntimeConfig::new(mode, copy_to_clipboard, os, shell))
}

/// Detects the operating system.
fn detect_os() -> String {
    match env::consts::OS {
        "windows" => "windows".to_string(),
        "macos" => "macos".to_string(),
        "linux" => "linux".to_string(),
        other => {
            eprintln!(
                "⚠️  Unknown OS detected: {}. This might affect the result.",
                other
            );
            format!("unknown ({})", other).to_string()
        }
    }
}

fn detect_shell() -> Option<String> {
    // Try to detect shell from well-known environment variables, if possible

    // Check for common shell environment variables
    if let Ok(shell) = env::var("SHELL") {
        if let Some(shell_name) = shell.split('/').next_back() {
            return Some(shell_name.to_string());
        }
    }

    // Windows-specific shell detection
    if std::env::consts::OS == "windows" {
        // Check for PowerShell
        if env::var("PSModulePath").is_ok() {
            return Some("powershell".to_string());
        }

        // Check for Command Prompt
        if env::var("COMSPEC").is_ok() {
            return Some("cmd".to_string());
        }
    }

    // Check for other common shell indicators
    if env::var("ZSH_VERSION").is_ok() {
        return Some("zsh".to_string());
    }

    if env::var("BASH_VERSION").is_ok() {
        return Some("bash".to_string());
    }

    if env::var("FISH_VERSION").is_ok() {
        return Some("fish".to_string());
    }

    None
}

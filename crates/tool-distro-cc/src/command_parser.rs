use anyhow::{Context, Result};

pub fn parse_command_parts(command: &str) -> Result<Vec<String>> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Err(anyhow::anyhow!("Command cannot be empty"));
    }

    shell_words::split(trimmed).context("Failed to parse command into arguments")
}

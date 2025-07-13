use crate::tools::ai::utils::select_file_from_path::select_file_from_path;
use anyhow::Result;
use std::path::PathBuf;

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

use crossterm::style::Color;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub enum FileProcessResult {
    Undefined,
    Decompressing,
    DecompressedOk,
    DecompressedFailed,
    Identifying,
    IdentifiedOk,
    IdentifiedFailed,
    Copying,
    CopiedOk,
    CopiedFailed,
    Ignored,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileProcessItem {
    pub file_path: String,
    pub file: PathBuf,
    pub attempt: usize,
    pub status: FileProcessResult,
}

impl FileProcessItem {
    pub fn update_status(&self, status: FileProcessResult) -> FileProcessItem {
        let updated_attempts = self.attempt + 1;

        FileProcessItem {
            file_path: self.file_path.clone(),
            file: self.file.clone(),
            attempt: updated_attempts,
            status,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum AgentState {
    Idle,
    Working,
    Finished,
}

pub enum Role {
    Agent,
    User,
    System,
}

impl Role {
    pub fn get_tag_color(&self) -> Color {
        match self {
            Role::Agent => Color::Green,
            Role::User => Color::Cyan,
            Role::System => Color::Yellow,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct AiResponse {
    pub success: bool,
    pub message: String,
}

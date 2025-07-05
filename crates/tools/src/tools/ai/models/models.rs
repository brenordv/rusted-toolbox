use crossterm::style::Color;
use serde::Deserialize;

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

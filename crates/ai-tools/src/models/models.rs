use anyhow::{anyhow, Context, Result};
use crossterm::style::Color;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum MediaType {
    Unknown,
    TvShow,
    Movie,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TvShowSeasonEpisodeInfo {
    pub season: u32,
    pub episode: u32,
}

impl TvShowSeasonEpisodeInfo {
    pub fn new(data: String) -> Result<TvShowSeasonEpisodeInfo> {
        if data.is_empty() {
            return Err(anyhow!("Data is empty"));
        }

        let split_data: Vec<&str> = data.split(',').collect();

        if split_data.len() != 2 {
            return Err(anyhow!(format!(
                "Data is not in the correct format: '{}'",
                data
            )));
        }

        let season_parts: Vec<&str> = split_data[0].split(':').collect();
        let episode_parts: Vec<&str> = split_data[1].split(':').collect();

        if season_parts.len() != 2 || episode_parts.len() != 2 {
            return Err(anyhow!(format!(
                "Data is not in the correct format: '{}'",
                data
            )));
        }

        let season = season_parts[1]
            .parse::<u32>()
            .context("Failed to parse season number")?;

        let episode = episode_parts[1]
            .parse::<u32>()
            .context("Failed to parse episode number")?;

        Ok(TvShowSeasonEpisodeInfo { season, episode })
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

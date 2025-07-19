use anyhow::{anyhow, Context, Result};
use crossterm::style::Color;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

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

#[derive(Debug, Serialize, Deserialize)]
pub struct FileProcessItem {
    pub file_path: String,
    pub file: PathBuf,
    pub attempt: usize,
    pub status: FileProcessResult,
    pub media_type: Option<MediaType>,
    pub title: Option<String>,
    pub is_archive: Option<bool>,
    pub is_main_archive_file: Option<bool>,
    pub season_episode_info: Option<TvShowSeasonEpisodeInfo>,
}

impl FileProcessItem {
    pub fn new(file_path: String, file: PathBuf) -> FileProcessItem {
        FileProcessItem {
            file_path,
            file,
            attempt: 0,
            status: FileProcessResult::Undefined,
            media_type: None,
            title: None,
            is_archive: None,
            is_main_archive_file: None,
            season_episode_info: None,
        }
    }

    pub fn update_status(&self, status: FileProcessResult) -> FileProcessItem {
        FileProcessItem {
            file_path: self.file_path.clone(),
            file: self.file.clone(),
            attempt: self.attempt,
            status,
            media_type: self.media_type.clone(),
            title: self.title.clone(),
            is_archive: self.is_archive.clone(),
            is_main_archive_file: self.is_main_archive_file.clone(),
            season_episode_info: self.season_episode_info.clone(),
        }
    }

    pub fn update_attempt(&self) -> FileProcessItem {
        let updated_attempt = self.attempt + 1;

        FileProcessItem {
            file_path: self.file_path.clone(),
            file: self.file.clone(),
            attempt: updated_attempt,
            status: self.status.clone(),
            media_type: self.media_type.clone(),
            title: self.title.clone(),
            is_archive: self.is_archive.clone(),
            is_main_archive_file: self.is_main_archive_file.clone(),
            season_episode_info: self.season_episode_info.clone(),
        }
    }

    pub fn update_media_type(&self, media_type: MediaType) -> FileProcessItem {
        FileProcessItem {
            file_path: self.file_path.clone(),
            file: self.file.clone(),
            attempt: self.attempt,
            status: self.status.clone(),
            media_type: Some(media_type),
            title: self.title.clone(),
            is_archive: self.is_archive.clone(),
            is_main_archive_file: self.is_main_archive_file.clone(),
            season_episode_info: self.season_episode_info.clone(),
        }
    }

    pub fn update_title(&self, title: String) -> FileProcessItem {
        FileProcessItem {
            file_path: self.file_path.clone(),
            file: self.file.clone(),
            attempt: self.attempt,
            status: self.status.clone(),
            media_type: self.media_type.clone(),
            title: Some(title),
            is_archive: self.is_archive.clone(),
            is_main_archive_file: self.is_main_archive_file.clone(),
            season_episode_info: self.season_episode_info.clone(),
        }
    }

    pub fn update_is_archived(&self, is_archived: bool) -> FileProcessItem {
        FileProcessItem {
            file_path: self.file_path.clone(),
            file: self.file.clone(),
            attempt: self.attempt,
            status: self.status.clone(),
            media_type: self.media_type.clone(),
            title: self.title.clone(),
            is_archive: Some(is_archived),
            is_main_archive_file: self.is_main_archive_file.clone(),
            season_episode_info: self.season_episode_info.clone(),
        }
    }

    pub fn update_is_main_archive_file(&self, is_main_archive_file: bool) -> FileProcessItem {
        FileProcessItem {
            file_path: self.file_path.clone(),
            file: self.file.clone(),
            attempt: self.attempt,
            status: self.status.clone(),
            media_type: self.media_type.clone(),
            title: self.title.clone(),
            is_archive: self.is_archive.clone(),
            is_main_archive_file: Some(is_main_archive_file),
            season_episode_info: self.season_episode_info.clone(),
        }
    }

    pub fn update_season_episode_info(
        &self,
        season_episode_info: TvShowSeasonEpisodeInfo,
    ) -> FileProcessItem {
        FileProcessItem {
            file_path: self.file_path.clone(),
            file: self.file.clone(),
            attempt: self.attempt,
            status: self.status.clone(),
            media_type: self.media_type.clone(),
            title: self.title.clone(),
            is_archive: self.is_archive.clone(),
            is_main_archive_file: self.is_main_archive_file.clone(),
            season_episode_info: Some(season_episode_info),
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

pub struct RecursiveDirWalkControl {
    folders: HashMap<PathBuf, bool>,
    current_folder: Option<PathBuf>,
}

impl RecursiveDirWalkControl {
    pub fn new() -> RecursiveDirWalkControl {
        RecursiveDirWalkControl {
            folders: HashMap::new(),
            current_folder: None,
        }
    }

    pub fn add_folder(&mut self, folder: &PathBuf) {
        self.folders.insert(folder.clone(), false);
    }

    fn done_with_folder(&mut self, folder: &PathBuf) {
        self.folders.insert(folder.clone(), true);
    }

    fn get_next(&mut self) -> Option<PathBuf> {
        for (path, is_done) in self.folders.iter() {
            if *is_done {
                continue;
            }
            return Some(path.clone());
        }

        None
    }

    pub fn next(&mut self) -> Option<PathBuf> {
        // If self.current_folder already have a value, find it in the self.folders and set it to true.
        let current_folder_opt = self.current_folder.clone();

        if let Some(current_folder) = current_folder_opt {
            self.done_with_folder(&current_folder);
        };

        // Get the first value in self.folders where the value is false.
        if let Some(next_folder) = self.get_next() {
            // Set it to the self.current_folder
            self.current_folder = Some(next_folder.clone());

            // Return value.
            return Some(next_folder.clone());
        }

        // If no value available, return None.
        None
    }

    pub fn has_next(&mut self) -> bool {
        let next_file = self.get_next();
        next_file.is_some()
    }
}

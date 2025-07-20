use crate::tools::ai::models::models::{FileProcessResult, MediaType, TvShowSeasonEpisodeInfo};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
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
    pub target_path: Option<PathBuf>,
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
            target_path: None,
        }
    }
}

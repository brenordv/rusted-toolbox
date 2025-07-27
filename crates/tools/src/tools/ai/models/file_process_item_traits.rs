use crate::tools::ai::models::models::{FileProcessResult, MediaType, TvShowSeasonEpisodeInfo};
use anyhow::Result;
use std::path::PathBuf;

pub trait FileProcessItemTraits {
    fn update_status(&self, status: FileProcessResult) -> Result<()>;
    fn update_attempt(&self) -> Result<()>;
    fn update_media_type(&self, media_type: MediaType) -> Result<()>;
    fn update_title(&self, title: String) -> Result<()>;
    fn update_is_archive(&self, is_archived: bool) -> Result<()>;
    fn update_is_main_archive_file(&self, is_main_archive_file: bool) -> Result<()>;
    fn update_season_episode_info(
        &self,
        season_episode_info: TvShowSeasonEpisodeInfo,
    ) -> Result<()>;
    fn update_target_path(&self, target_path: PathBuf) -> Result<()>;
}
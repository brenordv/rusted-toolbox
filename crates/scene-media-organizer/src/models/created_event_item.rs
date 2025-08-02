use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use shared::utils::sanitize_string_for_filename::sanitize_string_for_filename;
use std::str::FromStr;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Copy)]
pub enum CreatedEventItemFileType {
    /// Item is a file
    File,
    /// Item is a folder
    Directory,
}

impl FromStr for CreatedEventItemFileType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "file" => Ok(CreatedEventItemFileType::File),
            "directory" | "dir" | "folder" => Ok(CreatedEventItemFileType::Directory),
            _ => Err(format!("Unknown file type: {}", s)),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Copy)]
pub enum CreatedEventItemMediaType {
    /// Item is a Movie
    Movie,
    /// Item is a Tv Show episode
    TvShow,
}

impl FromStr for CreatedEventItemMediaType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "movie" => Ok(CreatedEventItemMediaType::Movie),
            "tvshow" | "tv-show" | "tv show" | "episode" => Ok(CreatedEventItemMediaType::TvShow),
            _ => Err(format!("Unknown media type: {}", s)),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Copy)]
pub enum CreatedEventItemStatus {
    /// Don't know anything about the item
    New,
    /// File identified. Time to figure out where to place this.
    Identified,
    /// Defined where to copy the file.
    Prepared,
    /// Started to copy the file to destination.
    Copying,
    /// All done with the file.
    Done,
    /// When the file is an archive, but it is not the main file, we ignore it because there's
    /// nothing to do with it. We need the main file to uncompress.
    Ignored,
}

impl FromStr for CreatedEventItemStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "new" => Ok(CreatedEventItemStatus::New),
            "identified" => Ok(CreatedEventItemStatus::Identified),
            "prepared" => Ok(CreatedEventItemStatus::Prepared),
            "copying" => Ok(CreatedEventItemStatus::Copying),
            "done" => Ok(CreatedEventItemStatus::Done),
            "ignored" => Ok(CreatedEventItemStatus::Ignored),
            _ => Err(format!("Unknown item status: {}", s)),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone)]
pub struct CreatedEventItem {
    pub full_path: String,
    pub file_name: String,
    pub parent: String,
    pub target_path: String,
    pub item_type: CreatedEventItemFileType,
    pub media_type: CreatedEventItemMediaType,
    pub status: CreatedEventItemStatus,
    pub is_archive: bool,
    pub is_main_archive_file: bool,
    pub attempts: usize,
    pub title: String,
    pub year: Option<u32>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    pub timestamp: DateTime<Utc>,
}

impl CreatedEventItem {
    pub fn get_title_as_filename(&self) -> Result<String> {
        if self.title.is_empty() {
            anyhow::bail!("Title is empty when it should not be.");
        };

        Ok(sanitize_string_for_filename(&self.title))
    }

    pub fn update_target_path(&mut self, new_target_path: String) {
        self.target_path = new_target_path;
    }

    pub fn update_status(&mut self, new_status: CreatedEventItemStatus) {
        self.status = new_status;
    }

    pub fn get_full_path_as_path_buf(&self) -> std::path::PathBuf {
        std::path::PathBuf::from(self.full_path.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        let x = "movie".parse::<CreatedEventItemMediaType>().unwrap();
        let y = "tvshow".parse::<CreatedEventItemMediaType>().unwrap();
        let z = "tv-show".parse::<CreatedEventItemMediaType>().unwrap();

        assert_eq!(x, CreatedEventItemMediaType::Movie);
        assert_eq!(y, CreatedEventItemMediaType::TvShow);
        assert_eq!(z, CreatedEventItemMediaType::TvShow);
    }
}

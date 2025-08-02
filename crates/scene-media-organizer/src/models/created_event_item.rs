use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
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
            "tvshow" | "tv-show" => Ok(CreatedEventItemMediaType::TvShow),
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

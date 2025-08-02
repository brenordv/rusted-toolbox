use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Serialize, Deserialize)]
pub enum CreatedEventItemFileType {
    /// Item is a file
    File,
    /// Item is a folder
    Directory,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum CreatedEventItemMediaType {
    /// Item is a Movie
    Movie,
    /// Item is a Tv Show episode
    TvShow,
}

#[derive(Debug, Serialize, Deserialize)]
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
    Ignored
}

#[derive(Debug, Serialize, Deserialize)]
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
    pub timestamp: DateTime<Utc>
}
use crate::models::created_event_item::{
    CreatedEventItem, CreatedEventItemFileType, CreatedEventItemMediaType, CreatedEventItemStatus,
};
use crate::models::guessit_response::GuessItResponse;
use crate::utils::file_status_controller::FileStatusController;
use crate::utils::guessit_client::GuessItClient;
use anyhow::{Context, Result};
use chrono::Utc;
use decompress::ExtractOptsBuilder;
use notify::Event;
use shared::system::ensure_directory_exists::EnsureDirectoryExists;
use shared::system::monitor_folder::EventType;
use shared::system::pathbuf_extensions::PathBufExtensions;
use std::env;
use std::path::PathBuf;
use std::process::Command;
use tracing_log::log::{debug, error};

pub struct ProcessFileRoutine {
    file_status_controller: FileStatusController,
    guess: GuessItClient,
}

impl ProcessFileRoutine {
    pub fn new(db_path: String, guess_it_base_url: String) -> Result<Self> {
        Ok(Self {
            file_status_controller: FileStatusController::new(db_path)?,
            guess: GuessItClient::new(guess_it_base_url),
        })
    }

    pub async fn handle_on_file_created(
        &self,
        event: Event,
        event_type: EventType,
        _: PathBuf,
    ) -> Result<()> {
        let created_entries = &event.paths;
        debug!(
            "[{:?}] On Created Event (count: {})",
            event_type,
            created_entries.len()
        );

        for entry in created_entries {
            debug!("Processing file: {:?}", entry);

            let entry_str = entry
                .to_str()
                .context("Failed to convert path to string.")?
                .to_string();

            let mut file_control_item =
                match self.file_status_controller.get_file_control(&entry_str)? {
                    Some(file_control_item) => {
                        debug!("File already exists in the database.");
                        file_control_item
                    }
                    None => {
                        debug!("Calling GuessIt to get info on the file...");
                        let response = self.guess.it(entry.to_str().unwrap().to_string()).await?;

                        debug!("Creating file control item...");
                        let new_item = Self::create_file_control_item(&entry, &response)?;

                        debug!("Adding file control item to database...");
                        self.file_status_controller.add_file_control(&new_item)?;

                        new_item
                    }
                };

            match file_control_item.status {
                CreatedEventItemStatus::New => {
                    if file_control_item.is_archive {
                        debug!("File is an archive. Decompressing...");
                        let decompressed = Self::decompress_file(&entry)?;
                        if decompressed {
                            file_control_item.status = CreatedEventItemStatus::Prepared;
                            self.file_status_controller
                                .update_file_control(&file_control_item)?;
                        } else {
                            error!("Failed to decompress file.");
                        }
                    }
                    debug!("Hey, this is a new file: {:?}", file_control_item.full_path);
                }
                CreatedEventItemStatus::Identified => {}
                CreatedEventItemStatus::Prepared => {}
                CreatedEventItemStatus::Copying => {}
                CreatedEventItemStatus::Done => {
                    debug!("Oh, we're already done with this file. Moving on...");
                    continue;
                }
                CreatedEventItemStatus::Ignored => {
                    debug!("File is ignored. Moving on...");
                    continue;
                }
            }
        }

        Ok(())
    }

    fn create_file_control_item(
        full_path: &PathBuf,
        guess_it_data: &GuessItResponse,
    ) -> Result<CreatedEventItem> {
        let file_name = full_path
            .file_name()
            .context("Failed to get extract the filename.")?
            .to_str()
            .context("Failed to convert filename to string.")?
            .to_string();

        let parent = full_path
            .parent()
            .context("Failed to get extract the parent directory.")?
            .file_name()
            .context("Failed to get extract the filename.")?
            .to_str()
            .context("Failed to convert filename to string.")?
            .to_string();

        // This shouldn't fail because it runs on the file that was just created.
        // is_file/is_dir can only return true if the item exists.
        let item_type = if full_path.is_file() {
            CreatedEventItemFileType::File
        } else {
            CreatedEventItemFileType::Directory
        };

        let media_type = guess_it_data
            .r#type
            .parse::<CreatedEventItemMediaType>()
            .unwrap();

        let is_archive = full_path.is_compressed();

        let is_main_archive_file = full_path.is_main_file_multi_part_compression();

        let status = if is_archive & !is_main_archive_file {
            CreatedEventItemStatus::Ignored
        } else {
            CreatedEventItemStatus::New
        };

        let attempts: usize = 1;

        let title = guess_it_data.title.clone();

        let year = guess_it_data.year;

        let season = guess_it_data.season;

        let episode = guess_it_data.episode;

        let timestamp = Utc::now();

        let full_path_str = full_path
            .to_str()
            .context("Failed to convert path to string.")?
            .to_string();

        let mut file_control_item = CreatedEventItem {
            full_path: full_path_str,
            file_name,
            parent,
            target_path: "".to_string(),
            item_type,
            media_type,
            status,
            is_archive,
            is_main_archive_file,
            attempts,
            title,
            year,
            season,
            episode,
            timestamp,
        };

        let target_path = Self::define_target_path(&file_control_item)?;

        file_control_item.update_target_path(target_path);

        Ok(file_control_item)
    }

    fn define_target_path(file_control_item: &CreatedEventItem) -> Result<String> {
        let title_as_filename = file_control_item.get_title_as_filename()?;

        match file_control_item.media_type {
            CreatedEventItemMediaType::Movie => {
                let base_movie_folder = env::var("SMO_WATCHDOG_WATCH_BASE_MOVIE_FOLDER")
                    .context("SMO_WATCHDOG_WATCH_BASE_MOVIE_FOLDER not found in .env file")?;

                let mut path = PathBuf::from(base_movie_folder);

                path.push(title_as_filename);

                path.ensure_directory_exists()?;

                let path_as_str = path
                    .to_str()
                    .context("Failed to get movie path as string")?
                    .to_string();

                Ok(path_as_str)
            }
            CreatedEventItemMediaType::TvShow => {
                let base_tv_folder = env::var("SMO_WATCHDOG_WATCH_BASE_TVSHOW_FOLDER")
                    .context("SMO_WATCHDOG_WATCH_BASE_TVSHOW_FOLDER not found in .env file")?;

                let mut path = PathBuf::from(base_tv_folder);

                path.push(title_as_filename);

                let season = match file_control_item.season {
                    Some(season) => format!("Season{:02}", season),
                    None => anyhow::bail!("Season not found in file control item."),
                };

                path.push(season);

                path.ensure_directory_exists()?;

                let path_as_str = path
                    .to_str()
                    .context("Failed to get tv show path as string")?
                    .to_string();

                Ok(path_as_str)
            }
        }
    }

    fn decompress_file(file: &PathBuf) -> Result<bool> {
        // Get the parent directory to extract into
        let out_dir = file
            .parent()
            .map(|p| p.to_path_buf())
            .context("File must have a parent directory")?;

        // Get lowercase file extension as a string, e.g., "zip", "rar"
        let ext = file
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();

        // Try to decompress crate for common formats
        if matches!(
            ext.as_str(),
            "zip"
                | "gz"
                | "bz2"
                | "xz"
                | "tar"
                | "tgz"
                | "tbz2"
                | "zst"
                | "tar.gz"
                | "tar.bz2"
                | "tar.xz"
        ) {
            let decompress_options = &ExtractOptsBuilder::default()
                .build()
                .context("Failed to build decompress options")?;

            // Use decompress crate
            return match decompress::decompress(file, &out_dir, decompress_options) {
                Ok(_) => Ok(true),
                Err(e) => Err(anyhow::anyhow!("decompress error: {e}")),
            };
        }

        // For .rar files, try using `unrar` command-line tool
        if ext == "rar" {
            // Try invoking `unrar` if available
            let status = Command::new("Z:\\dev\\projects\\rust\\rusted-toolbox\\UnRAR.exe")
                .arg("x")
                .arg("-y") // auto-yes for prompts
                .arg(file)
                .arg(out_dir)
                .status()
                .context("Failed to run unrar. Check if it is installed and in your PATH.")?;

            return if status.success() {
                Ok(true)
            } else {
                Err(anyhow::anyhow!("unrar failed with status: {status}"))
            };
        }

        // For .7z files, try using `7z` command-line tool
        if ext == "7z" {
            let status = Command::new("7z")
                .arg("x")
                .arg(file)
                .arg(format!("-o{}", out_dir.display()))
                .status()
                .context("Failed to run 7z")?;

            return if status.success() {
                Ok(true)
            } else {
                Err(anyhow::anyhow!("7z failed with status: {status}"))
            };
        }

        // Unsupported file format
        Ok(false)
    }
}

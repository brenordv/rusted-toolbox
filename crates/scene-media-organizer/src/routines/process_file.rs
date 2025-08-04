use crate::models::created_event_item::{
    CreatedEventItem, CreatedEventItemFileType, CreatedEventItemMediaType, CreatedEventItemStatus,
};
use crate::models::guessit_response::GuessItResponse;
use crate::models::watchdog_runtime_config::WatchdogRuntimeConfig;
use crate::utils::file_status_controller::FileStatusController;
use crate::utils::guessit_client::GuessItClient;
use anyhow::{Context, Result};
use chrono::Utc;
use decompress::ExtractOptsBuilder;
use notify::Event;
use shared::system::ensure_directory_exists::EnsureDirectoryExists;
use shared::system::monitor_folder::EventType;
use shared::system::pathbuf_extensions::PathBufExtensions;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
use tokio::sync::mpsc::Sender;
use tokio::time::sleep;
use tracing_log::log::{debug, error, info};

pub struct ProcessFileRoutine {
    file_status_controller: FileStatusController,
    guess: GuessItClient,
    runtime_config: WatchdogRuntimeConfig,
    decompress_file_task_channel: Option<Sender<String>>,
}

impl ProcessFileRoutine {
    pub fn new(runtime_config: &WatchdogRuntimeConfig) -> Result<Self> {
        Ok(Self {
            file_status_controller: FileStatusController::new(runtime_config.db_data_file.clone())?,
            guess: GuessItClient::new(runtime_config.guess_it_api_base_url.clone()),
            runtime_config: runtime_config.clone(),
            decompress_file_task_channel: None,
        })
    }

    pub fn new_with_channel(
        runtime_config: &WatchdogRuntimeConfig,
        decompress_file_task_channel: Sender<String>,
    ) -> Result<Self> {
        Ok(Self {
            file_status_controller: FileStatusController::new(runtime_config.db_data_file.clone())?,
            guess: GuessItClient::new(runtime_config.guess_it_api_base_url.clone()),
            runtime_config: runtime_config.clone(),
            decompress_file_task_channel: Some(decompress_file_task_channel),
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
            info!("Waiting for file to stabilize: {:?}", entry);
            if !Self::wait_for_file_stability(&entry).await? {
                error!("File is not stable. Moving on...");
                continue;
            }

            info!("File is stable. Processing it...");

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
                        let new_item = self.create_file_control_item(&entry, &response)?;

                        debug!("Adding file control item to database...");
                        self.file_status_controller.add_file_control(&new_item)?;

                        new_item
                    }
                };

            match file_control_item.status {
                CreatedEventItemStatus::New => {
                    if file_control_item.is_archive {
                        debug!("File is an archive. Notifying that decompression is needed...");
                        match self.decompress_file_task_channel {
                            Some(ref channel) => {
                                channel.send(entry_str).await?;
                            }
                            None => {
                                error!("Failed to send decompression task to channel.");
                            }
                        };

                        continue;
                    }

                    debug!("Should I copy this file to the target path?");
                    let should_copy = Self::should_copy_file(&entry)?;
                    if !should_copy {
                        debug!("Nope, I'm not touching this. Moving on...");
                        file_control_item.update_status(CreatedEventItemStatus::Ignored);
                        self.file_status_controller
                            .update_file_control(&file_control_item)?;
                        continue;
                    }

                    file_control_item.update_status(CreatedEventItemStatus::Copying);
                    self.file_status_controller
                        .update_file_control(&file_control_item)?;

                    Self::copy_file(&file_control_item).await?;

                    file_control_item.update_status(CreatedEventItemStatus::Done);
                    self.file_status_controller
                        .update_file_control(&file_control_item)?;

                    debug!(
                        "Alright! This file was copied! \\o/: {:?}",
                        file_control_item.full_path
                    );
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

    pub fn handle_decompress_file(&self, target_file: String) -> Result<()> {
        match self.file_status_controller.get_file_control(&target_file)? {
            None => {
                anyhow::bail!(format!("File not found in database: {}", target_file));
            }
            Some(mut file_control_item) => {
                let target_file = &file_control_item.get_full_path_as_path_buf();

                let mut current_attempts: u64 = 0;
                const MAX_ATTEMPTS: u64 = 5;
                const QUIET_TIME_MS: u64 = 10000;

                while current_attempts <= MAX_ATTEMPTS {
                    let current_quiet_time = current_attempts * QUIET_TIME_MS;

                    if current_quiet_time > 0 {
                        info!(
                            "Decompressing file failed. Trying again in {} ms...",
                            &current_quiet_time
                        );
                    }

                    let _ = sleep(Duration::from_millis(current_quiet_time));

                    current_attempts += 1;

                    match self.decompress_file(&target_file) {
                        Ok(success) => {
                            if success {
                                file_control_item.update_status(CreatedEventItemStatus::Done);
                                self.file_status_controller
                                    .update_file_control(&file_control_item)?;
                                // After decompressing the file, it will trigger this event more times,
                                // so we don't need to do anything else.
                                debug!("Ok, we're done with this file.");
                                return Ok(());
                            } else {
                                error!("Failed to decompress file.");
                            }
                        }
                        Err(e) => {
                            error!("Failed to decompress file: {}", e);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn create_file_control_item(
        &self,
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
            .media_type
            .parse::<CreatedEventItemMediaType>()
            .unwrap();

        let is_archive = full_path.is_compressed();

        let is_main_archive_file = full_path.is_main_file_multi_part_compression();

        let status = if (is_archive & !is_main_archive_file)
            || item_type == CreatedEventItemFileType::Directory
        {
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

        let target_path = self.define_target_path(&file_control_item)?;

        file_control_item.update_target_path(target_path);

        Ok(file_control_item)
    }

    fn define_target_path(&self, file_control_item: &CreatedEventItem) -> Result<String> {
        let mut title_as_filename = file_control_item.get_title_as_filename()?;

        let mut path: PathBuf;

        match file_control_item.media_type {
            CreatedEventItemMediaType::Movie => {
                path = PathBuf::from(self.runtime_config.target_base_movie_folder.clone());

                if let Some(year) = file_control_item.year {
                    title_as_filename.push_str(&format!("--{}", year));
                }

                path.push(title_as_filename);
            }
            CreatedEventItemMediaType::TvShow => {
                path = PathBuf::from(self.runtime_config.target_base_series_folder.clone());

                path.push(title_as_filename);

                let season = match file_control_item.season {
                    Some(season) => format!("Season{:02}", season),
                    None => anyhow::bail!("Season not found in file control item."),
                };

                path.push(season);
            }
        };

        let path_as_str = path
            .to_str()
            .context(format!(
                "Failed to get {:?} path as string",
                file_control_item.media_type
            ))?
            .to_string();

        Ok(path_as_str)
    }

    fn decompress_file(&self, file: &PathBuf) -> Result<bool> {
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
            let status = Command::new(self.runtime_config.unrar_bin_path.clone())
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

    fn should_copy_file(file: &PathBuf) -> Result<bool> {
        // Copying folders would be a mess...
        if file.is_dir() {
            return Ok(false);
        }

        // Don't care about sample files.
        if file.to_string_lossy().to_lowercase().contains("sample") {
            return Ok(false);
        };

        // I won't copy any compressed files.
        if file.is_compressed() {
            return Ok(false);
        }

        // Get file extension, we need to check that.
        let ext_opt = file
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_lowercase());

        // Also don't care about files without an extension.
        let ext = match ext_opt {
            Some(ref e) if !e.is_empty() => e,
            _ => return Ok(false),
        };

        // For this use case, I also don't want to copy any of those files.
        let extensions_to_skip = [
            "sh", "bat", "ps1", "py", "js", "rb", "pl", "php", "lua", // Executables
            "exe", "dll", "bin", "so", "out", // Compressed
            "zip", "rar", "7z", "gz", "bz2", "xz", "tar", // Just in case
        ];

        Ok(!extensions_to_skip.contains(&ext.as_str()))
    }

    async fn copy_file(file_control_item: &CreatedEventItem) -> Result<()> {
        // TODO Mk2: Make this configurable.
        const MAX_RETRIES: u32 = 5;
        const BASE_DELAY_MS: u64 = 3000;

        if file_control_item.target_path.is_empty() {
            anyhow::bail!("Target path is empty. This should never happen.");
        }

        let destination_folder = PathBuf::from(file_control_item.target_path.clone());

        destination_folder.ensure_directory_exists()?;

        let destination = destination_folder.join(file_control_item.file_name.clone());

        let source = PathBuf::from(file_control_item.full_path.clone());

        for attempt in 1..=MAX_RETRIES {
            match fs::copy(&source, &destination) {
                Ok(_) => {
                    return Ok(());
                }
                Err(e) => {
                    if attempt == MAX_RETRIES {
                        return Err(anyhow::anyhow!(
                            "Failed to copy file from {:?} to {:?} after {} attempts. Last error: {}",
                            file_control_item.full_path,
                            destination.display(),
                            MAX_RETRIES,
                            e
                        ));
                    }

                    let delay = Duration::from_millis(BASE_DELAY_MS * (2_u64.pow(attempt - 1)));

                    debug!(
                        "Copy attempt {} failed: {}. Retrying in {:?}...",
                        attempt, e, delay
                    );

                    sleep(delay).await;
                }
            }
        }

        Ok(())
    }

    async fn wait_for_file_stability(file_path: &PathBuf) -> Result<bool> {
        let mut last_size = None;
        let mut stable_count = 0;

        const TIMEOUT_SECS: u64 = 120;
        const REQUIRED_STABLE_CHECKS: u8 = 3;
        const CHECK_INTERVAL_MS: u64 = 1000;

        for _ in 0..(TIMEOUT_SECS * 1000 / CHECK_INTERVAL_MS) {
            match fs::metadata(file_path) {
                Ok(metadata) => {
                    let current_size = metadata.len();

                    if Some(current_size) == last_size {
                        stable_count += 1;
                        if stable_count >= REQUIRED_STABLE_CHECKS {
                            return Ok(true);
                        }
                    } else {
                        stable_count = 0;
                    }

                    last_size = Some(current_size);
                }
                Err(_) => {
                    stable_count = 0;
                }
            }

            sleep(Duration::from_millis(CHECK_INTERVAL_MS)).await;
        }

        Ok(false)
    }
}

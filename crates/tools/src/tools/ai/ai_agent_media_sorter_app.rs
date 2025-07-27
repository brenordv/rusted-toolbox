use crate::shared::sqlite::dictionary_db::{DictionaryDb, DictionaryDbItem};
use crate::shared::system::ensure_directory_exists::EnsureDirectoryExists;
use crate::shared::system::folder_walkthrough::list_all_files_recursively;
use crate::shared::utils::sanitize_string_for_filename::sanitize_string_for_filename;
use crate::tools::ai::message_builders::system_message_builders::build_rust_ai_function_system_message;
use crate::tools::ai::models::file_process_item_model::FileProcessItem;
use crate::tools::ai::models::file_process_item_traits::FileProcessItemTraits;
use crate::tools::ai::models::models::FileProcessResult::{
    CopiedOk, DecompressedFailed, DecompressedOk, Decompressing, IdentifiedFailed, IdentifiedOk,
    Identifying, Ignored, Undefined,
};
use crate::tools::ai::models::models::MediaType::{Movie, TvShow};
use crate::tools::ai::models::models::{FileProcessResult, MediaType};
use crate::tools::ai::requesters::requester_builders::build_requester_for_openai;
use crate::tools::ai::requesters::requester_implementations::OpenAiRequester;
use crate::tools::ai::requesters::requester_traits::OpenAiRequesterTraits;
use crate::tools::ai::tasks::media_identifiers::identify_file_hybrid;
use crate::tools::ai::utils::control_file_wrapper::ControlFileWrapper;
use anyhow::{Context, Result};
use decompress::ExtractOptsBuilder;
use notify::Event;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::{env, fs};
use tracing::{debug, error, info, warn};

pub async fn handle_event_created(event: Event, watch_folder: PathBuf) -> Result<()> {
    debug!("File created event triggered with event: {:?}", event);

    debug!("Loading environment variables...");
    let data_folder_name =
        env::var("AI_MEDIA_SORTER_DATA_FOLDER").unwrap_or_else(|_| ".data".to_string());

    let max_attempts = env::var("AI_MEDIA_SORTER_MAX_RETRIES")
        .context("AI_MEDIA_SORTER_MAX_RETRIES not found in .env file")?
        .parse::<usize>()
        .context("AI_MEDIA_SORTER_MAX_RETRIES must be a number")?;

    debug!("Initializing control database...");
    let db_path = PathBuf::from(data_folder_name)
        .join("files_read.db")
        .to_string_lossy()
        .to_string();

    let db = DictionaryDb::new(db_path, "files_read".to_string())?;

    let files_read_db = Arc::new(db);

    debug!("Initializing AI requester...");
    let mut ai_requester = build_requester_for_openai()?;

    ai_requester.set_system_message(build_rust_ai_function_system_message())?;

    let created_entries = &event.paths;

    debug!("Preparing to process {} files...", created_entries.len());

    for entry in created_entries {
        for file in list_all_files_recursively(entry) {
            let file_rel_path = file
                .strip_prefix(&watch_folder)?
                .to_string_lossy()
                .to_string();

            debug!("Working on file: {:?}", &file_rel_path);

            handle_new_file(
                file_rel_path,
                &file,
                files_read_db.clone(),
                &mut ai_requester,
                &max_attempts,
            )
            .await?;
        }
    }

    Ok(())
}

async fn handle_new_file(
    file_rel_path: String,
    file: &PathBuf,
    files_read_db: Arc<DictionaryDb>,
    ai_requester: &mut OpenAiRequester,
    max_retries: &usize,
) -> Result<()> {
    debug!("Loading control item for file...");

    let control_db_item = ensure_control_item(file, files_read_db.clone(), &file_rel_path)?;

    let db_item = control_db_item.value;

    let ref_file = &db_item.file_path.clone();

    debug!("Adding wrapper to control item...");

    let file_control = ControlFileWrapper::new(files_read_db.clone(), db_item)?;
    let file_control = Arc::new(file_control);

    debug!("Processing file...");

    while file_control.get_current_attempts() < *max_retries {
        match file_control.get_current_status() {
            FileProcessResult::Undefined => {
                debug!("File is currently unknown. Identifying...");

                // No work done yet. Let's decompress the file. Let's identify the file.
                file_control.update_status(Identifying)?;
                match identify_file_hybrid(file_control.clone(), ai_requester).await {
                    Ok(_) => {
                        file_control.update_status(IdentifiedOk)?;
                        continue;
                    }
                    Err(e) => {
                        error!("Identify file failed: {}", e);
                        file_control.update_status(IdentifiedFailed)?;
                    }
                }
            }
            FileProcessResult::Decompressing
            | FileProcessResult::Identifying
            | FileProcessResult::Copying => {
                // Already working on this file.
                // We shouldn't receive notification for the file as created while working on it.
                // If this happens, will check it out. For now, let's just move on.
                warn!(
                    "Already working on file [{}] for task [{}]",
                    file.display(),
                    &ref_file
                );
                break;
            }
            FileProcessResult::IdentifiedOk => {
                if !file_control.get_is_archived() {
                    // If it is not compressed, we just need to copy it.
                    // So let's consider the file as decompressed.
                    debug!("File is not compressed. Skipping decompression...");
                    file_control.update_status(DecompressedOk)?;
                    continue;
                }

                if !file_control.get_is_main_archive_file() {
                    debug!(
                        "File is compressed, but is not the main archive file. Ignoring file..."
                    );
                    file_control.update_status(Ignored)?;
                    break;
                }

                debug!("Decompressing file...");
                handle_decompress(file_control.clone())?;
                continue;
            }
            FileProcessResult::DecompressedOk => {
                // All good so far.
                // now we need to:
                // 1. Figure out where to copy the files

                debug!("Preparing to copy file...");

                let target_folder = define_target_folder(file_control.clone())?;

                debug!(
                    "Files will be copied to the folder: {}",
                    target_folder.display()
                );

                // 2. Gather a list of files to copy

                debug!("Gathering list of files to copy...");

                let target_files = list_files_to_copy(file_control.clone());

                debug!("Found {} files to copy", target_files.len());

                // 3. Copy

                debug!("Copying files...");

                copy_files(target_folder, target_files)?;

                file_control.update_status(CopiedOk)?;

                continue;
            }
            FileProcessResult::IdentifiedFailed => {
                // Failed to identify the media. Maybe try again.
                debug!("Failed to identify media type. Trying again...");

                file_control.update_attempt()?;
                file_control.update_status(Undefined)?;
                continue;
            }
            FileProcessResult::DecompressedFailed => {
                // Failed to decompress. Maybe try again.
                debug!("Failed to decompress file. Trying again...");
                file_control.update_attempt()?;
                file_control.update_status(IdentifiedOk)?;
                continue;
            }
            FileProcessResult::CopiedFailed => {
                // Failed to copy. Try again.
                debug!("Failed to copy file. Trying again...");
                file_control.update_attempt()?;
                file_control.update_status(DecompressedOk)?;
                continue;
            }
            FileProcessResult::Ignored => {
                // File is compressed, but it's not the main one from a multi-part,
                // so we just ignore it.
                break;
            }
            FileProcessResult::CopiedOk => {
                // All done for this file. Nothing else to do.
                break;
            }
        }
    }

    debug!("Finished processing file: {}", file.display());

    Ok(())
}

fn copy_files(target_folder: PathBuf, files_to_copy: Vec<PathBuf>) -> Result<()> {
    let mut file_count: usize = 1;

    for file in files_to_copy {
        debug!("Copying file: [{:0>3}] {}...", file_count, file.display());

        // Copy as just the filename into the target folder
        let destination = target_folder.join(file.file_name().ok_or_else(|| {
            anyhow::anyhow!("Could not determine file name for {}", file.display())
        })?);

        // Actually copy the file
        fs::copy(&file, &destination).context(format!(
            "Failed to copy file from {} to {}",
            file.display(),
            destination.display()
        ))?;

        file_count += 1;
    }

    debug!("Finished copying files...");

    Ok(())
}

fn list_files_to_copy(control: Arc<ControlFileWrapper>) -> Vec<PathBuf> {
    let mut files_to_copy = vec![];

    for file in list_all_files_recursively(&control.get_file()) {
        // Don't care about sample files.
        if file.to_string_lossy().to_lowercase().contains("sample") {
            continue;
        }

        // Get file extension, we need to check that.
        let ext_opt = file
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_lowercase());

        // Also don't care about files without an extension.
        let ext = match ext_opt {
            Some(ref e) if !e.is_empty() => e,
            _ => continue,
        };

        // For this use case, I also don't want to copy any of those files.
        let skip_exts = [
            // Scripts
            "sh", "bat", "ps1", "py", "js", "rb", "pl", "php", "lua", // Executables
            "exe", "dll", "bin", "so", "out", // Compressed
            "zip", "rar", "7z", "gz", "bz2", "xz", "tar",
        ];

        // Including files that are part of a multi-file archive
        let is_multipart = (ext.len() == 3 || ext.len() == 4)
            && ((ext.starts_with('r') && ext[1..].chars().all(|c| c.is_ascii_digit()))
                || ext.chars().all(|c| c.is_ascii_digit()));

        if skip_exts.contains(&ext.as_str()) || is_multipart {
            continue;
        }

        // If the file made it this far, we'll copy it.
        files_to_copy.push(file);
    }

    files_to_copy
}

fn ensure_control_item(
    file: &PathBuf,
    files_read_db: Arc<DictionaryDb>,
    file_str: &String,
) -> Result<DictionaryDbItem<FileProcessItem>> {
    let item = files_read_db.get::<FileProcessItem>(&file_str)?;

    if let Some(item) = item {
        return Ok(item);
    }

    let file_control = FileProcessItem::new(file_str.clone(), file.clone());
    files_read_db.add::<FileProcessItem>(&file_str, &file_control)?;
    let new_item = files_read_db.get::<FileProcessItem>(&file_str)?;

    if let Some(new_item) = new_item {
        return Ok(new_item);
    }

    anyhow::bail!("Failed to add file to control db: {}", file.display());
}

fn handle_decompress(control: Arc<ControlFileWrapper>) -> Result<()> {
    let file = control.get_file();

    control.update_status(Decompressing)?;

    let success_decompress = decompress_file(&file)?;

    if success_decompress {
        info!("Decompressed file: {}", file.display());
        control.update_status(DecompressedOk)?;
    } else {
        control.update_status(DecompressedFailed)?;
        anyhow::bail!("Failed to decompress file: {}", file.display());
    }

    Ok(())
}

fn define_target_folder(control: Arc<ControlFileWrapper>) -> Result<PathBuf> {
    let title = match control.get_title() {
        Some(t) => sanitize_string_for_filename(&t),
        None => {
            anyhow::bail!(
                "Failed to get title from file: {}",
                control.get_file().display()
            );
        }
    };

    match control.get_media_type() {
        TvShow => {
            let base_path = env::var("AI_MEDIA_SORTER_WATCH_BASE_TVSHOW_FOLDER")
                .context("AI_MEDIA_SORTER_WATCH_BASE_TVSHOW_FOLDER not found in .env file")?;

            let episode_info = match control.get_episode_info() {
                Some(e_info) => e_info,
                None => {
                    anyhow::bail!(
                        "Failed to get episode info from file: {}",
                        control.get_file().display()
                    );
                }
            };

            let mut path = PathBuf::from(base_path);

            path.push(title);

            path.push(format!("Season{:02}", episode_info.season));

            path.ensure_directory_exists()?;

            return Ok(path);
        }
        Movie => {
            let base_path = env::var("AI_MEDIA_SORTER_WATCH_BASE_MOVIE_FOLDER")
                .context("AI_MEDIA_SORTER_WATCH_BASE_MOVIE_FOLDER not found in .env file")?;

            let mut path = PathBuf::from(base_path);

            path.push(title);

            path.ensure_directory_exists()?;

            return Ok(path);
        }
        MediaType::Unknown => {
            anyhow::bail!("Unknown media type. Cannot define target folder.");
        }
    };
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

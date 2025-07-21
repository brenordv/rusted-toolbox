use crate::shared::sqlite::dictionary_db::{DictionaryDb, DictionaryDbItem};
use crate::shared::system::folder_walkthrough::list_all_files_recursively;
use crate::shared::system::pathbuf_extensions::PathBufExtensions;
use crate::tools::ai::ai_functions::media_sorter_functions::{
    extract_movie_title_from_filename_as_string, extract_season_episode_from_filename_as_string,
    extract_tv_show_title_from_filename_as_string, identify_media_format_from_filename_as_string,
    identify_media_type_from_filename_as_string, is_main_archive_file_as_string,
};
use crate::tools::ai::message_builders::system_message_builders::{
    build_rust_ai_function_system_message, build_rust_ai_function_user_message,
};
use crate::tools::ai::models::file_process_item_model::FileProcessItem;
use crate::tools::ai::models::file_process_item_traits::FileProcessItemTraits;
use crate::tools::ai::models::models::FileProcessResult::{
    DecompressedFailed, DecompressedOk, Decompressing, IdentifiedFailed, IdentifiedOk, Identifying,
    Ignored,
};
use crate::tools::ai::models::models::MediaType::{Movie, TvShow};
use crate::tools::ai::models::models::{FileProcessResult, MediaType, TvShowSeasonEpisodeInfo};
use crate::tools::ai::requesters::requester_builders::build_requester_for_openai;
use crate::tools::ai::requesters::requester_implementations::OpenAiRequester;
use crate::tools::ai::requesters::requester_traits::OpenAiRequesterTraits;
use crate::tools::ai::utils::control_file_wrapper::ControlFileWrapper;
use anyhow::{Context, Result};
use decompress::ExtractOptsBuilder;
use log::{info, warn};
use notify::Event;
use std::{env, fs};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use crate::shared::system::ensure_directory_exists::EnsureDirectoryExists;
use crate::shared::utils::sanitize_string_for_filename::sanitize_string_for_filename;

pub async fn handle_event_created(event: Event) -> Result<()> {
    let data_folder_name =
        env::var("AI_MEDIA_SORTER_DATA_FOLDER").unwrap_or_else(|_| ".data".to_string());

    let max_attempts = env::var("AI_MEDIA_SORTER_MAX_RETRIES")
        .context("AI_MEDIA_SORTER_MAX_RETRIES not found in .env file")?
        .parse::<usize>()
        .context("AI_MEDIA_SORTER_MAX_RETRIES must be a number")?;

    let db_path = PathBuf::from(data_folder_name)
        .join("files_read.db")
        .absolute_to_string()?;

    let db = DictionaryDb::new(db_path, "files_read".to_string())?;

    let files_read_db = Arc::new(db);

    let mut ai_requester = build_requester_for_openai()?;

    ai_requester.set_system_message(build_rust_ai_function_system_message())?;

    let created_entries = &event.paths;

    for entry in created_entries {
        for file in list_all_files_recursively(entry) {
            println!("File: {:?}", file);
            handle_new_file(
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
    file: &PathBuf,
    files_read_db: Arc<DictionaryDb>,
    ai_requester: &mut OpenAiRequester,
    max_retries: &usize,
) -> Result<()> {
    info!("File: {:?} / {:?}", file.file_name(), file);

    let file_str = file.absolute_to_string()?;

    let mut control_db_item = ensure_control_item(file, files_read_db.clone(), &file_str)?;

    let db_item = control_db_item.value;

    let ref_file = &db_item.file_path.clone();

    let file_control = ControlFileWrapper::new(files_read_db.clone(), db_item)?;
    let mut file_control = Arc::new(file_control);

    while file_control.get_current_attempts() < *max_retries {
        match file_control.get_current_status() {
            FileProcessResult::Undefined => {
                // No work done yet. Let's decompress the file. Let's identify the file.
                identify_file(file_control.clone(), ai_requester).await?;
                continue;
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
                continue;
            }
            FileProcessResult::IdentifiedOk => {
                if !file_control.get_is_archived() {
                    // If it is not compressed, we just need to copy it.
                    // So let's consider the file as decompressed.
                    file_control.update_status(DecompressedOk)?;
                    continue;
                }

                if !file_control.get_is_main_archive_file() {
                    file_control.update_status(Ignored)?;
                    continue;
                }

                handle_decompress(file_control.clone())?;
            }
            FileProcessResult::DecompressedOk => {
                // All good so far.
                // now we need to:
                // 1. Figure out where to copy the files
                let target_folder = define_target_folder(file_control.clone())?;
                // 2. Gather a list of files to copy
                let target_files = list_files_to_copy(file_control.clone());
                // 3. Copy
                copy_files(target_folder, target_files)?;
            }
            FileProcessResult::IdentifiedFailed => {
                // Failed to identify the media. Maybe try again.
                file_control.update_attempt()?;
                identify_file(file_control.clone(), ai_requester).await?;
                continue;
            }
            FileProcessResult::DecompressedFailed => {
                // Failed to decompress. Maybe try again.
                file_control.update_attempt()?;

                handle_decompress(file_control.clone())?;
                continue;
            }
            FileProcessResult::CopiedFailed => {
                // Failed to copy. Try again.
                file_control.update_attempt()?;

                //TODO: Add copy method here
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

    Ok(())
}

fn copy_files(target_folder: PathBuf, files_to_copy: Vec<PathBuf>) -> Result<()> {
    for file in files_to_copy {
        print!("Copying file: {}...", file.display());

        // Determine a relative path to preserve folder structure
        let rel_path = file.strip_prefix(
            file.ancestors()
                .last()
                .unwrap_or_else(|| Path::new("")) // fallback to the empty prefix if necessary
        ).unwrap_or(&file);

        // Compose the destination path
        let destination = target_folder.join(rel_path);

        destination.ensure_directory_exists()?;

        // Actually copy the file
        fs::copy(&file, &destination)
            .context(format!(
                "Failed to copy file from {} to {}",
                file.display(),
                destination.display())
            )?;


        println!("Done!");
    }

    Ok(())
}

fn list_files_to_copy(control: Arc<ControlFileWrapper>) -> Vec<PathBuf> {
    let mut files_to_copy = vec![];

    for file in list_all_files_recursively(&control.get_file()) {
        // Check if file has an extension
        let ext_opt = file.extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_lowercase());

        // Skip files with no extension
        let ext = match ext_opt {
            Some(ref e) if !e.is_empty() => e,
            _ => continue,
        };

        let skip_exts = [
            // Scripts
            "sh", "bat", "ps1", "py", "js", "rb", "pl", "php", "lua",
            // Executables
            "exe", "dll", "bin", "so", "out",
            // Compressed
            "zip", "rar", "7z", "gz", "bz2", "xz", "tar",
        ];

        // Skip multi-part archive and split file extensions: .001-.099, .r01-.r99, etc.
        let is_multipart = (ext.len() == 3 || ext.len() == 4) && (
            (ext.starts_with('r') && ext[1..].chars().all(|c| c.is_ascii_digit()))
                || ext.chars().all(|c| c.is_ascii_digit())
        );

        if skip_exts.contains(&ext.as_str()) || is_multipart {
            continue;
        }

        files_to_copy.push(file);
    }

    files_to_copy
}

fn ensure_control_item(
    file: &PathBuf,
    files_read_db: Arc<DictionaryDb>,
    file_str: &String,
) -> Result<DictionaryDbItem<FileProcessItem>> {
    Ok(files_read_db
        .get::<FileProcessItem>(&file_str)
        .unwrap_or({
            let file_control = FileProcessItem::new(file_str.clone(), file.clone());

            files_read_db.add::<FileProcessItem>(&file_str, &file_control)?;
            files_read_db.get::<FileProcessItem>(&file_str)?
        })
        .unwrap())
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

async fn identify_file(
    control: Arc<ControlFileWrapper>,
    ai_requester: &mut OpenAiRequester,
) -> Result<()> {
    control.update_status(Identifying)?;

    let file = control.get_file();

    let file_name = file.to_str().unwrap();

    info!("Identifying file: {}", file_name);

    info!("Guessing media type...");

    let response = ai_requester
        .send_request(
            build_rust_ai_function_user_message(
                identify_media_type_from_filename_as_string,
                file_name,
            ),
            false,
        )
        .await?;

    let ai_media_type = response.message.as_str();

    info!("Is file from a Movie or TV Show?: {:?}", ai_media_type);

    if ai_media_type == "movie" {
        control.update_media_type(Movie)?;

        let response = ai_requester
            .send_request(
                build_rust_ai_function_user_message(
                    extract_movie_title_from_filename_as_string,
                    file_name,
                ),
                false,
            )
            .await?;

        let movie_title = response.message;

        info!("Movie title: {:?}", movie_title);

        control.update_title(movie_title)?;
    } else if ai_media_type == "tvshow" {
        control.update_media_type(TvShow)?;

        info!("Extracting title of the TV Show...");

        let response = ai_requester
            .send_request(
                build_rust_ai_function_user_message(
                    extract_tv_show_title_from_filename_as_string,
                    file_name,
                ),
                false,
            )
            .await?;

        let tv_show_title = response.message;

        println!("TV Show name?: {:?}", tv_show_title);

        control.update_title(tv_show_title)?;

        info!("Extracting title of the TV Show...");

        let response = ai_requester
            .send_request(
                build_rust_ai_function_user_message(
                    extract_season_episode_from_filename_as_string,
                    file_name,
                ),
                false,
            )
            .await?;

        println!("Season and Episode numbers?: {:?}", response.message);

        let season_episode_info = TvShowSeasonEpisodeInfo::new(response.message)?;

        control.update_season_episode_info(season_episode_info)?;
    } else {
        control.update_status(IdentifiedFailed)?;

        anyhow::bail!("Failed to identify media type for file: {}", file_name);
    };

    info!("Checking if file is an archive...");
    let response = ai_requester
        .send_request(
            build_rust_ai_function_user_message(
                identify_media_format_from_filename_as_string,
                file_name,
            ),
            false,
        )
        .await?;

    let file_type = response.message.as_str();
    println!("Is file compressed or decompressed?: {:?}", file_type);

    if file_type == "compressed" {
        control.update_is_archived(true)?;

        info!("Identifying if it is the main archive file...");

        let request =
            build_rust_ai_function_user_message(is_main_archive_file_as_string, file_name);

        let response = ai_requester.send_request(request, false).await?;

        let is_main_file = response.message.as_str();

        info!("Is file the main archive file?: {:?}", is_main_file);

        match is_main_file.parse::<bool>() {
            Ok(b) => {
                control.update_is_main_archive_file(b)?;
            }
            Err(_) => {
                anyhow::bail!(
                    "Failed to identify if file is the main archive file: {}",
                    file_name
                );
            }
        };
    } else if file_type == "decompressed" {
        control.update_is_archived(false)?;
    }

    control.update_status(IdentifiedOk)?;

    Ok(())
}

fn define_target_folder(control: Arc<ControlFileWrapper>) -> Result<PathBuf> {
    let title = match control.get_title() {
        Some(t) => sanitize_string_for_filename(&t),
        None => {
            anyhow::bail!("Failed to get title from file: {}", control.get_file().display());
        }
    };

    match control.get_media_type() {
        TvShow => {
            let base_path = env::var("AI_MEDIA_SORTER_WATCH_BASE_TVSHOW_FOLDER")
                .context("AI_MEDIA_SORTER_WATCH_BASE_TVSHOW_FOLDER not found in .env file")?;

            let episode_info = match control.get_episode_info() {
                Some(e_info) => e_info,
                None => {
                    anyhow::bail!("Failed to get episode info from file: {}", control.get_file().display());
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
        let status = Command::new("unrar")
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
use crate::shared::sqlite::dictionary_db::{DictionaryDb};
use crate::shared::system::pathbuf_extensions::PathBufExtensions;
use crate::tools::ai::ai_functions::media_sorter_functions::{
    extract_movie_title_from_filename_as_string, extract_season_episode_from_filename_as_string,
    extract_tv_show_title_from_filename_as_string, identify_media_format_from_filename_as_string,
    identify_media_type_from_filename_as_string, is_main_archive_file_as_string,
};
use crate::tools::ai::message_builders::system_message_builders::{
    build_rust_ai_function_system_message, build_rust_ai_function_user_message,
};
use crate::tools::ai::models::models::FileProcessResult::{
    DecompressedFailed, DecompressedOk, Decompressing, IdentifiedFailed, IdentifiedOk, Identifying,
    Ignored,
};
use crate::tools::ai::models::models::MediaType::{Movie, TvShow};
use crate::tools::ai::models::models::{
    FileProcessItem, FileProcessResult, RecursiveDirWalkControl, TvShowSeasonEpisodeInfo,
};
use crate::tools::ai::requesters::requester_builders::build_requester_for_openai;
use crate::tools::ai::requesters::requester_implementations::OpenAiRequester;
use crate::tools::ai::requesters::requester_traits::OpenAiRequesterTraits;
use anyhow::{Context, Result};
use decompress::ExtractOptsBuilder;
use log::{error, info, warn};
use notify::Event;
use std::path::PathBuf;
use std::process::Command;
use std::{env, fs};

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

    let files_read_db = DictionaryDb::new(db_path, "files_read".to_string())?;

    let mut ai_requester = build_requester_for_openai()?;

    ai_requester
        .set_system_message(build_rust_ai_function_system_message())?
        .set_temperature(&0.0)?;

    let files = &event.paths;

    for file_or_folder in files {
        if file_or_folder.is_dir() {
            walk_through_new_folder(
                file_or_folder,
                &files_read_db,
                &mut ai_requester,
                &max_attempts,
            )
            .await?;
            continue;
        }

        handle_new_file(
            file_or_folder,
            &files_read_db,
            &mut ai_requester,
            &max_attempts,
        )
        .await?;
    }

    Ok(())
}

async fn walk_through_new_folder(
    folder: &PathBuf,
    files_read_db: &DictionaryDb,
    ai_requester: &mut OpenAiRequester,
    max_retries: &usize,
) -> Result<()> {
    let root_entries = match fs::read_dir(folder) {
        Ok(e) => e,
        Err(e) => {
            error!("Failed to read root entry directory {:?}: {}", folder, e);
            return Err(anyhow::anyhow!(
                "Failed to read root entry directory {:?}: {}",
                folder,
                e
            ));
        }
    };

    let mut folder_walkthrough_control = RecursiveDirWalkControl::new();

    for entry in root_entries {
        let entry = entry.context("Failed to read directory entry")?.path();

        if entry.is_file() {
            handle_new_file(&entry, files_read_db, ai_requester, max_retries).await?;
            continue;
        }

        folder_walkthrough_control.add_folder(&entry);
    }

    // Walk through everything, treating ever file we found.
    // Created this control to avoid having to use recursion to deal with folder recursively.
    while folder_walkthrough_control.has_next() {
        if let Some(entry) = folder_walkthrough_control.next() {
            if entry.is_file() {
                handle_new_file(&entry, files_read_db, ai_requester, max_retries).await?;
                continue;
            }

            folder_walkthrough_control.add_folder(&entry);
        }
    }

    Ok(())
}

fn handle_decompress(control: &FileProcessItem, files_read_db: &DictionaryDb) -> Result<()> {
    let file = &control.file;

    files_read_db
        .update::<FileProcessItem>(&control.file_path, &control.update_status(Decompressing))?;

    let success_decompress = decompress_file(file)?;

    if success_decompress {
        info!("Decompressed file: {}", file.display());
        files_read_db.update::<FileProcessItem>(
            &control.file_path,
            &control.update_status(DecompressedOk),
        )?;
    } else {
        files_read_db.update::<FileProcessItem>(
            &control.file_path,
            &control.update_status(DecompressedFailed),
        )?;
        anyhow::bail!("Failed to decompress file: {}", file.display());
    }

    Ok(())
}

async fn identify_file(
    file: &FileProcessItem,
    files_read_db: &DictionaryDb,
    ai_requester: &mut OpenAiRequester,
) -> anyhow::Result<()> {
    files_read_db.update::<FileProcessItem>(&file.file_path, &file.update_status(Identifying))?;

    let file_name = file.file.to_str().unwrap();

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
        files_read_db.update::<FileProcessItem>(&file.file_path, &file.update_media_type(Movie))?;

        info!("Extracting title of the movie...");

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

        files_read_db
            .update::<FileProcessItem>(&file.file_path, &file.update_title(movie_title))?;
    } else if ai_media_type == "tvshow" {
        files_read_db
            .update::<FileProcessItem>(&file.file_path, &file.update_media_type(TvShow))?;

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

        files_read_db
            .update::<FileProcessItem>(&file.file_path, &file.update_title(tv_show_title))?;

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

        files_read_db.update::<FileProcessItem>(
            &file.file_path,
            &file.update_season_episode_info(season_episode_info),
        )?;
    } else {
        files_read_db
            .update::<FileProcessItem>(&file.file_path, &file.update_status(IdentifiedFailed))?;
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
        files_read_db.update::<FileProcessItem>(&file.file_path, &file.update_is_archived(true))?;

        info!("Identifying if it is the main archive file...");
        let request =
            build_rust_ai_function_user_message(is_main_archive_file_as_string, file_name);

        let response = ai_requester.send_request(request, false).await?;

        let is_main_file = response.message.as_str();

        info!("Is file the main archive file?: {:?}", is_main_file);

        match is_main_file.parse::<bool>() {
            Ok(b) => {
                files_read_db.update::<FileProcessItem>(
                    &file.file_path,
                    &file.update_is_main_archive_file(b),
                )?;
            }
            Err(_) => {
                anyhow::bail!(
                    "Failed to identify if file is the main archive file: {}",
                    file_name
                );
            }
        };
    } else if file_type == "decompressed" {
        files_read_db
            .update::<FileProcessItem>(&file.file_path, &file.update_is_archived(false))?;
    }

    files_read_db.update::<FileProcessItem>(&file.file_path, &file.update_status(IdentifiedOk))?;

    Ok(())
}

async fn handle_new_file(
    file: &PathBuf,
    files_read_db: &DictionaryDb,
    ai_requester: &mut OpenAiRequester,
    max_retries: &usize,
) -> anyhow::Result<()> {
    info!("File: {:?} / {:?}", file.file_name(), file);

    let file_str = file.absolute_to_string()?;

    let mut control_db_item = files_read_db
        .get::<FileProcessItem>(&file_str)
        .unwrap_or({
            let file_control = FileProcessItem::new(file_str.clone(), file.clone());

            files_read_db.add::<FileProcessItem>(&file_str, &file_control)?;
            files_read_db.get::<FileProcessItem>(&file_str)?
        })
        .unwrap();

    let file_control: &mut FileProcessItem = &mut control_db_item.value;

    while file_control.attempt < *max_retries {
        match &file_control.status {
            FileProcessResult::Undefined => {
                // No work done yet. Let's decompress the file. Let's identify the file.
                identify_file(file_control, files_read_db, ai_requester).await?;
                continue;
            }
            FileProcessResult::Decompressing
            | FileProcessResult::Identifying
            | FileProcessResult::Copying => {
                // Already working on this file.
                // We shouldn't receive notification for the file as created while working on it.
                // If this happens, will check it out. For now, let's just move on.
                warn!(
                    "Already working on file [{}] for task [{:?}]",
                    file.display(),
                    &file_control
                );
                continue;
            }
            FileProcessResult::IdentifiedOk => {
                match &file_control.is_archive {
                    Some(is_archive) => {
                        if !is_archive {
                            // If it is not compressed, we just need to copy it.
                            // So let's consider the file as decompressed.
                            files_read_db.update::<FileProcessItem>(
                                &file_control.file_path,
                                &file_control.update_status(DecompressedOk),
                            )?;
                            continue;
                        }

                        match &file_control.is_main_archive_file {
                            Some(is_main_archive_file) => {
                                if !is_main_archive_file {
                                    files_read_db.update::<FileProcessItem>(
                                        &file_control.file_path,
                                        &file_control.update_status(Ignored),
                                    )?;
                                    continue;
                                }

                                handle_decompress(file_control, files_read_db)?;
                            }
                            None => {
                                //This means that the file is not compressed
                                //in which case we won't reach this part.

                                //This could also mean AI wasn't able to identify if this file
                                // is the main one in the archive, and that begs the question:
                                // How to solve this?
                            }
                        }
                    }
                    None => {
                        //This could also mean AI wasn't able to identify if the file is an
                        // archive or not, and that begs the question: How to solve this?
                    }
                };
            }
            FileProcessResult::DecompressedOk => {
                // All good so far.
                // now we need to:
                // 1. Figure out where to copy the files
                // 2. Gather a list of files to copy
                // 3. Copy
            }
            FileProcessResult::IdentifiedFailed => {
                // Failed to identify the media. Maybe try again.
                files_read_db.update::<FileProcessItem>(
                    &file_control.file_path,
                    &file_control.update_attempt(),
                )?;
                identify_file(file_control, files_read_db, ai_requester).await?;
                continue;
            }
            FileProcessResult::DecompressedFailed => {
                // Failed to decompress. Maybe try again.
                files_read_db.update::<FileProcessItem>(
                    &file_control.file_path,
                    &file_control.update_attempt(),
                )?;

                handle_decompress(file_control, files_read_db)?;
                continue;
            }
            FileProcessResult::CopiedFailed => {
                // Failed to copy. Try again.
                files_read_db.update::<FileProcessItem>(
                    &file_control.file_path,
                    &file_control.update_attempt(),
                )?;

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

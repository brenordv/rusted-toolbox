use crate::shared::sqlite::dictionary_db::DictionaryDb;
use crate::shared::system::pathbuf_extensions::PathBufExtensions;
use crate::tools::ai::models::models::FileProcessResult::{
    DecompressedFailed, DecompressedOk, Decompressing, Ignored,
};
use crate::tools::ai::models::models::{FileProcessItem, FileProcessResult};
use anyhow::{Context, Result};
use decompress::ExtractOptsBuilder;
use log::{error, info, warn};
use notify::Event;
use std::path::PathBuf;
use std::process::Command;
use std::{env, fs};

pub fn handle_file_created(event: Event) -> anyhow::Result<()> {
    let data_folder_name =
        env::var("AI_MEDIA_SORTER_DATA_FOLDER").unwrap_or_else(|_| ".data".to_string());

    let db_path = PathBuf::from(data_folder_name)
        .join("files_read.db")
        .absolute_to_string()?;

    let files_read_db = DictionaryDb::new(db_path, "files_read".to_string())?;

    let files = &event.paths;

    for file_or_folder in files {
        if file_or_folder.is_dir() {
            handle_new_folder(file_or_folder, &files_read_db)?;
            continue;
        }

        handle_new_file(file_or_folder, &files_read_db)?;

        println!(
            "File: {:?} / {:?}",
            file_or_folder.file_name(),
            file_or_folder
        );
    }

    Ok(())
}

fn handle_file_decompression(
    control: &FileProcessItem,
    files_read_db: &DictionaryDb,
) -> anyhow::Result<()> {
    let file = &control.file;

    if !file.is_compressed() {
        // If it is not compressed, we just need to copy it. So let's consider the file as
        // decompressed.
        files_read_db.update::<FileProcessItem>(
            &control.file_path,
            &control.update_status(DecompressedOk),
        )?;
        return Ok(());
    }

    if !file.is_main_file_multi_part_compression() {
        // Compressed, but not the main file on a multi-part compressed.
        files_read_db
            .update::<FileProcessItem>(&control.file_path, &control.update_status(Ignored))?;
        return Ok(());
    }

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

fn handle_new_file(file: &PathBuf, files_read_db: &DictionaryDb) -> anyhow::Result<()> {
    let file_str = file.absolute_to_string()?;

    if let Some(mut control_db_item) = files_read_db.get::<FileProcessItem>(&file_str)? {
        let file_control: &mut FileProcessItem = &mut control_db_item.value;
        match &file_control.status {
            FileProcessResult::Undefined => {
                // No work done yet. Let's decompress the file.
                handle_file_decompression(file_control, files_read_db)?;
                return Ok(());
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
                    &control_db_item.value
                );
                return Ok(());
            }
            FileProcessResult::DecompressedOk => {
                // File decompressed. Time to copy it.
            }
            FileProcessResult::DecompressedFailed => {
                // Failed to decompress. Maybe try again.
            }
            FileProcessResult::IdentifiedOk => {
                // Media identified.
            }
            FileProcessResult::IdentifiedFailed => {
                // Failed to identify the media. Maybe try again.
            }
            FileProcessResult::CopiedOk => {
                // All done for this file. Nothing else to do.
            }
            FileProcessResult::CopiedFailed => {
                // Failed to copy. Try again.
            }
            FileProcessResult::Ignored => {
                // File is compressed, but it's not the main one from a multi-part,
                // so we just ignore it.
            }
        };
    } else {
        // Basically the same as undefined.
        // Never saw this file before.
        let mut file_control = FileProcessItem {
            file_path: file_str.clone(),
            file: file.clone(),
            attempt: 1,
            status: FileProcessResult::Undefined,
        };

        files_read_db.add::<FileProcessItem>(&file_str, &file_control)?;

        handle_file_decompression(&mut file_control, files_read_db)?;
    };

    Ok(())
}

fn handle_new_folder(folder: &PathBuf, files_read_db: &DictionaryDb) -> anyhow::Result<()> {
    // Try to read the directory entries.
    let entries = match fs::read_dir(folder) {
        Ok(e) => e,
        Err(e) => {
            error!("Failed to read directory {:?}: {}", folder, e);
            return Err(anyhow::anyhow!(
                "Failed to read directory {:?}: {}",
                folder,
                e
            ));
        }
    };

    for folder_entry in entries {
        match folder_entry {
            Ok(entry) => {
                let path = entry.path();

                if let Ok(md) = entry.metadata() {
                    // Skip symlinks, just in case.
                    // Not what we want to deal with here...
                    if md.file_type().is_symlink() {
                        info!("Skipping symlink: {:?}", path);
                        continue;
                    }
                    if md.is_dir() {
                        // Recurse into a subdirectory
                        if let Err(e) = handle_new_folder(&path, files_read_db) {
                            error!("Error in subdirectory {:?}: {}", path, e);
                            continue;
                        }
                    } else if md.is_file() {
                        // Process a new file
                        if let Err(e) = handle_new_file(&path, files_read_db) {
                            error!("Error handling file {:?}: {}", path, e);
                            continue;
                        }
                    } else {
                        // Not a regular file/dir
                        info!("Skipping special file: {:?}", path);
                    }
                } else {
                    error!("Failed to get metadata for: {:?}", path);
                    // Continue
                }
            }
            Err(e) => {
                error!("Error reading directory entry in {:?}: {}", folder, e);
                // Continue processing others
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

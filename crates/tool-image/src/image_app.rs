use std::collections::HashSet;
use std::path::PathBuf;
use anyhow::{anyhow, Result};
use indicatif::MultiProgress;
use crate::models::{EditArgs, EditJob};
use tracing::{debug, info};
use shared::system::folder_walkthrough::list_all_files_recursively;

pub fn run_image_edit_commands(args: &EditArgs) -> Result<()> {

    let input_batch = expand_input_paths(&args.input_files)?;
    let jobs = build_jobs(input_batch, args)?;
    let progress_bar = MultiProgress::new();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(num_cpus::get())
        .build()?;

    
    
    Ok(())
}

fn expand_input_paths(paths: &Vec<PathBuf>) -> Result<HashSet<PathBuf>> {
    let mut expanded_paths = HashSet::new();

    for path in paths {
        if !path.exists() {
            debug!("Path does not exist: {}", path.display());
            continue;
        }

        if path.is_file() && is_supported_image_file(path) {
            expanded_paths.insert(path.to_path_buf());
            continue;
        }

        for file in list_all_files_recursively(path) {
            if !is_supported_image_file(&file) {
                continue;
            }
            expanded_paths.insert(file);
        }
    }

    if expanded_paths.is_empty() {
        return Err(anyhow!("No supported image files found. Nothing to work with."));
    }

    info!("Found {} supported image files.", expanded_paths.len());
    Ok(expanded_paths)
}

fn is_supported_image_file(path: &PathBuf) -> bool {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let ext = ext.to_lowercase();
        matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "gif" | "webp" | "avif" | "tiff" | "tif" | "bmp")
    } else {
        false
    }
}

fn build_jobs(expanded_paths: HashSet<PathBuf>, args: &EditArgs) -> Result<Vec<EditJob>> {
    let mut jobs = Vec::new();
    for path in expanded_paths {
        jobs.push(EditJob {
            input_file: path,
            resize: args.resize.clone(),
            grayscale: args.grayscale.clone(),
            convert: args.convert.clone(),
        })
    }

    Ok(jobs)
}
use crate::image_edit_routines::{create_job_progress_bar, process_edit_job};
use crate::models::{EditArgs, EditJob, ProcessingStatsInner};
use anyhow::{anyhow, Result};
use indicatif::{MultiProgress};
use rayon::iter::IntoParallelIterator;
use rayon::iter::ParallelIterator;
use shared::system::folder_walkthrough::list_all_files_recursively;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info, warn};

pub fn run_image_edit_commands(args: &EditArgs) -> Result<()> {
    let input_batch = expand_input_paths(&args.input_files)?;
    let jobs = build_jobs(input_batch, args)?;
    let progress_bar = MultiProgress::new();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(num_cpus::get())
        .build()?;

    // Shared statistics
    let stats = Arc::new(ProcessingStatsInner::new());

    let results: Vec<Result<()>> = pool.install(|| {
        jobs.into_par_iter()
            .map(|job| {
                let pb = create_job_progress_bar(&job, &progress_bar)?;
                let result = process_edit_job(job, &pb);

                if result.is_ok() {
                    pb.finish_with_message("✓ Completed");
                    stats.increment_success();
                } else {
                    pb.finish_with_message("✗ Failed");
                    stats.increment_error();
                }

                result
            })
            .collect()
    });

    // Collect final statistics
    let final_stats = Arc::try_unwrap(stats)
        .map_err(|_| anyhow!("Failed to unwrap Arc<ProcessingStatsInner>"))?
        .into_stats();

    let mut first_error = None;
    for result in results {
        if let Err(e) = result {
            if first_error.is_none() {
                first_error = Some(e);
                break;
            }
        }
    }

    if final_stats.error_count > 0 {
        warn!("{} jobs failed", final_stats.error_count);
        if let Some(e) = first_error {
            debug!("First error: {}", e);
        }
    }

    info!(
        "Finished processing {} jobs: {} succeeded, {} failed",
        final_stats.total_count, final_stats.success_count, final_stats.error_count
    );

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
        return Err(anyhow!(
            "No supported image files found. Nothing to work with."
        ));
    }

    info!("Found {} supported image files.", expanded_paths.len());
    Ok(expanded_paths)
}

fn is_supported_image_file(path: &PathBuf) -> bool {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let ext = ext.to_lowercase();
        matches!(
            ext.as_str(),
            "jpg" | "jpeg" | "png" | "gif" | "webp" | "avif" | "tiff" | "tif" | "bmp"
        )
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
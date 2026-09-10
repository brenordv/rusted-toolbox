use crate::image_edit_routines::{create_job_progress_bar, process_edit_job};
use crate::models::{EditJob, ImageConfig, ProcessingStatsInner};
use anyhow::{anyhow, Result};
use indicatif::MultiProgress;
use rayon::iter::IntoParallelIterator;
use rayon::iter::ParallelIterator;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info, warn};

pub fn run_image_edit_commands(args: &ImageConfig) -> Result<()> {
    let jobs = build_jobs(&args.input_files, args)?;
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
                    pb.finish_with_message("Completed");
                    stats.increment_success();
                } else {
                    pb.finish_with_message("Failed");
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

fn build_jobs(input_files: &[PathBuf], args: &ImageConfig) -> Result<Vec<EditJob>> {
    let mut jobs = Vec::new();
    for path in input_files {
        jobs.push(EditJob {
            input_file: path.clone(),
            resize: args.resize.clone(),
            grayscale: args.grayscale,
            convert: args.convert,
            quality: args.quality,
            filter: args.filter,
        })
    }

    Ok(jobs)
}

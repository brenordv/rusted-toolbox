use crate::image_edit_routines::{create_job_progress_bar, process_edit_job};
use crate::models::{EditJob, ImageConfig, ProcessingStatsInner};
use anyhow::{anyhow, bail, Result};
use indicatif::MultiProgress;
use rayon::iter::IntoParallelIterator;
use rayon::iter::ParallelIterator;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, warn};

pub fn run_image_edit_commands(args: &ImageConfig) -> Result<()> {
    let jobs = build_jobs(&args.input_files, args)?;
    let job_count = jobs.len();
    let progress_bar = MultiProgress::new();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(num_cpus::get())
        .build()?;

    // Shared statistics
    let stats = Arc::new(ProcessingStatsInner::new());

    let results: Vec<(PathBuf, Result<()>)> = pool.install(|| {
        jobs.into_par_iter()
            .map(|job| {
                let input_file = job.input_file.clone();
                let result = run_single_job(job, &progress_bar);

                match &result {
                    Ok(()) => stats.increment_success(),
                    Err(_) => stats.increment_error(),
                }

                (input_file, result)
            })
            .collect()
    });

    // Collect final statistics
    let final_stats = Arc::try_unwrap(stats)
        .map_err(|_| anyhow!("Failed to unwrap Arc<ProcessingStatsInner>"))?
        .into_stats();

    // Logged after the pool completes so the lines do not fight the
    // progress-bar redraw on stderr.
    for (input_file, result) in &results {
        if let Err(e) = result {
            warn!("Job failed for {}: {:#}", input_file.display(), e);
        }
    }

    info!(
        "Finished processing {} jobs: {} succeeded, {} failed",
        final_stats.total_count, final_stats.success_count, final_stats.error_count
    );

    if final_stats.error_count > 0 {
        bail!("{} of {} jobs failed", final_stats.error_count, job_count);
    }

    Ok(())
}

/// Runs one edit job under its own progress bar, counting a progress-bar
/// setup failure the same as a processing failure.
fn run_single_job(job: EditJob, progress_bar: &MultiProgress) -> Result<()> {
    let pb = create_job_progress_bar(&job, progress_bar)?;
    let result = process_edit_job(job, &pb);

    match &result {
        Ok(()) => pb.finish_with_message("Completed"),
        Err(_) => pb.finish_with_message("Failed"),
    }

    result
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image_encoders::encode_png;
    use crate::models::ImageMeta;
    use image::imageops::FilterType;
    use image::{DynamicImage, ImageFormat};
    use std::path::Path;

    /// Grayscale-only config, so outputs get a `-grayscale` suffix instead of
    /// overwriting their inputs.
    fn config_for(input_files: Vec<PathBuf>) -> ImageConfig {
        ImageConfig {
            input_files,
            resize: None,
            grayscale: true,
            convert: None,
            quality: None,
            filter: FilterType::Lanczos3,
        }
    }

    fn write_test_png(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        let meta = ImageMeta {
            icc: None,
            original_format: ImageFormat::Png,
        };
        encode_png(&path, &DynamicImage::new_rgb8(4, 4), &meta).unwrap();
        path
    }

    #[test]
    fn run_returns_error_when_all_jobs_fail() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing.png");

        let result = run_image_edit_commands(&config_for(vec![missing]));

        assert!(result.is_err());
    }

    #[test]
    fn run_returns_error_when_some_jobs_fail() {
        let dir = tempfile::tempdir().unwrap();
        let good = write_test_png(dir.path(), "good.png");
        let missing = dir.path().join("missing.png");

        let result = run_image_edit_commands(&config_for(vec![good, missing]));

        assert_eq!(result.unwrap_err().to_string(), "1 of 2 jobs failed");
        assert!(dir.path().join("good-grayscale.png").exists());
    }

    #[test]
    fn run_succeeds_when_all_jobs_succeed() {
        let dir = tempfile::tempdir().unwrap();
        let good = write_test_png(dir.path(), "good.png");

        run_image_edit_commands(&config_for(vec![good])).unwrap();

        assert!(dir.path().join("good-grayscale.png").exists());
    }

    #[test]
    fn run_with_no_operations_reencodes_the_input_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let input = write_test_png(dir.path(), "photo.png");

        let config = ImageConfig {
            input_files: vec![input.clone()],
            resize: None,
            grayscale: false,
            convert: None,
            quality: None,
            filter: FilterType::Lanczos3,
        };

        run_image_edit_commands(&config).unwrap();

        // No suffix means the output path IS the input; the run must end
        // with a single decodable file, not a truncated original or a
        // stray temp.
        image::open(&input).unwrap();
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}

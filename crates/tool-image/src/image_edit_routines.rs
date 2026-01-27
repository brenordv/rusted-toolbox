use crate::image_encoders::{
    encode_avif, encode_bmp, encode_gif, encode_jpeg, encode_png, encode_webp,
};
use crate::image_format_traits::ImageFormatTraits;
use crate::models::{DecodedImage, EditJob, ImageMeta, ResizeSpec};
use anyhow::Result;
use image::imageops::FilterType;
use image::metadata::Orientation;
use image::ImageReader;
use image::{DynamicImage, ImageDecoder, ImageFormat};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::path::PathBuf;
use tracing::{debug, error, info, warn};

pub fn process_edit_job(job: EditJob, progress_bar: &ProgressBar) -> Result<()> {
    const PROGRESS_BAR_MAX: u64 = 100;

    let inc_step = if let Some(length) = progress_bar.length() {
        PROGRESS_BAR_MAX / length
    } else {
        1
    };

    progress_bar.set_length(100);

    info!("Decoding image: {}", job.input_file.display());
    progress_bar.set_message("Decoding image...");
    progress_bar.inc(inc_step);
    let mut img_info = decode_image(&job.input_file)?;

    debug!("Image decoded...");
    if let Some(resize) = &job.resize {
        info!("Resizing image to {}", resize);
        progress_bar.set_message(format!("Resizing image to {}...", resize));
        progress_bar.inc(inc_step);
        img_info.dynamic_image = apply_resize(img_info.dynamic_image, resize)?;
        debug!("Image resized...");
    }

    if job.grayscale {
        info!("Converting image to grayscale");
        progress_bar.set_message("Converting to greyscale...");
        progress_bar.inc(inc_step);
        img_info.dynamic_image = img_info.dynamic_image.grayscale();
        debug!("Image converted to grayscale...");
    }

    info!("Determining output plan...");
    progress_bar.set_message("Determining output plan...");
    progress_bar.inc(inc_step);
    let (output_format, output_path) = determine_output_plan(&job, &img_info.image_meta)?;

    info!("Saving image to {}", output_path.display());
    if job.convert.is_some() {
        // The job of converting the image will be done at save time...
        progress_bar.inc(inc_step);
    }

    progress_bar.set_message(format!("Saving image to {}...", output_path.display()));
    encode_image(
        &img_info.dynamic_image,
        &output_path,
        output_format,
        &img_info.image_meta,
    )?;
    progress_bar.inc(inc_step);

    debug!(
        "The file [{:?}] was processed and saved as: [{:?}]",
        job.input_file, output_path
    );
    Ok(())
}

pub fn create_job_progress_bar(job: &EditJob, progress_bar: &MultiProgress) -> Result<ProgressBar> {
    let step_count = get_progress_step(&job);
    let pb = progress_bar.add(ProgressBar::new(step_count));
    pb.set_style(
        ProgressStyle::default_bar()
            .template(format!("{{spinner:.blue}} [{{elapsed_precise}}] [{}] {{bar:50.green/black}} {{pos:>3}}% {{msg}}", job.input_file.display()).as_str())?,
    );

    Ok(pb)
}

fn encode_image(
    image: &DynamicImage,
    output_path: &PathBuf,
    output_format: ImageFormat,
    metadata: &ImageMeta,
) -> Result<()> {
    match output_format {
        ImageFormat::Png => encode_png(output_path, image, metadata),
        ImageFormat::Jpeg => encode_jpeg(output_path, image, metadata),
        ImageFormat::Gif => encode_gif(output_path, image),
        ImageFormat::WebP => encode_webp(output_path, image),
        ImageFormat::Bmp => encode_bmp(output_path, image),
        ImageFormat::Avif => encode_avif(output_path, image, metadata),
        _ => {
            warn!(
                "Unsupported output format: {:?}, quality may degrade.",
                output_format
            );
            image.save_with_format(&output_path, output_format)?;
            Ok(())
        }
    }
}

fn get_progress_step(job: &EditJob) -> u64 {
    // First step: decoding image
    // Later on: determine an output path
    let mut step_count = 2;

    if job.resize.is_some() {
        step_count += 1;
    }

    if job.grayscale {
        step_count += 1;
    }

    if job.convert.is_some() {
        step_count += 1;
    }

    step_count
}

fn determine_output_plan(job: &EditJob, metadata: &ImageMeta) -> Result<(ImageFormat, PathBuf)> {
    let output_format = job.convert.unwrap_or(metadata.original_format);

    // Generate suffix from operations
    let mut suffix_parts = Vec::new();

    if let Some(resize) = &job.resize {
        suffix_parts.push(resize.suffix());
    }

    if job.grayscale {
        suffix_parts.push("grayscale".to_string());
    }

    if let Some(target_format) = job.convert {
        suffix_parts.push(format!("convert{:?}", target_format));
    }

    let suffix = suffix_parts.join("-");

    let stem = job
        .input_file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("image-name");

    let extension = output_format.to_file_extension();

    let filename = if suffix.is_empty() {
        format!("{}.{}", stem, extension)
    } else {
        format!("{}-{}.{}", stem, suffix, extension)
    };

    let output_path = job.input_file.parent().unwrap().join(filename);

    Ok((output_format, output_path))
}

fn decode_image(image_path: &PathBuf) -> Result<DecodedImage> {
    let reader = ImageReader::open(image_path)?.with_guessed_format()?;

    let format = reader.format().unwrap_or(ImageFormat::Png);
    debug!("Detected format: {:?}", format);

    let mut decoder = reader.into_decoder()?;

    // Trying to get metadata
    let orientation = decoder.orientation();
    let icc_profile = decoder.icc_profile().unwrap_or_default();
    let color_type = decoder.color_type();
    let (width, height) = decoder.dimensions();

    debug!(
        "Image info: {}x{}, color_type: {:?}, orientation: {:?}, ICC: {} bytes",
        width,
        height,
        color_type,
        orientation,
        icc_profile.as_ref().map_or(0, |p| p.len())
    );

    // Decode the image
    let mut image = DynamicImage::from_decoder(decoder)?;

    // Applying orientation
    match orientation {
        Ok(orientation_data) => {
            if orientation_data != Orientation::NoTransforms {
                image.apply_orientation(orientation_data);
            }
        }
        Err(e) => {
            error!("Failed to get orientation: {}", e);
        }
    };

    let image_meta = ImageMeta {
        icc: icc_profile,
        original_format: format,
    };

    Ok(DecodedImage {
        dynamic_image: image,
        image_meta,
    })
}

fn apply_resize(image: DynamicImage, resize: &ResizeSpec) -> Result<DynamicImage> {
    match resize {
        ResizeSpec::Percent(percent) => {
            let new_width = (image.width() as f64 * (*percent / 100.0)).round() as u32;
            let new_height = (image.height() as f64 * (*percent / 100.0)).round() as u32;

            if new_width == 0 || new_height == 0 {
                anyhow::bail!("Invalid resize parameters");
            }

            debug!(
                "Resizing from {}x{} to {}x{}",
                image.width(),
                image.height(),
                new_width,
                new_height
            );

            Ok(image.resize(new_width, new_height, FilterType::Lanczos3))
        }
        ResizeSpec::Dimensions { width, height } => {
            warn_if_ratio_differs(image.width(), image.height(), *width, *height);

            let new_width = width.round() as u32;
            let new_height = height.round() as u32;

            if new_width == 0 || new_height == 0 {
                anyhow::bail!("Invalid resize parameters");
            }

            debug!(
                "Resizing from {}x{} to {}x{}",
                image.width(),
                image.height(),
                new_width,
                new_height
            );

            Ok(image.resize_exact(new_width, new_height, FilterType::Lanczos3))
        }
    }
}

fn warn_if_ratio_differs(original_width: u32, original_height: u32, width: f64, height: f64) {
    let width_ratio = width / original_width as f64;
    let height_ratio = height / original_height as f64;
    let ratio_delta = (width_ratio - height_ratio).abs();
    let epsilon = 0.0001_f64;

    if ratio_delta > epsilon {
        warn!(
            "Resize ratio mismatch: original={}x{}, target={}x{}, width_ratio={:.6}, height_ratio={:.6}",
            original_width, original_height, width, height, width_ratio, height_ratio
        );
    }
}

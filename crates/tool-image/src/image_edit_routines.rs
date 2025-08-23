use image::{ColorType, DynamicImage, ImageDecoder, ImageFormat, ImageResult};
use std::path::PathBuf;
use crate::models::{DecodedImage, EditJob, ImageMeta, ImageSize, TargetFormat};
use anyhow::Result;
use image::imageops::FilterType;
use image::ImageReader;
use image::metadata::Orientation;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use tracing::{info, debug, error, warn};
use crate::image_encoders::{encode_avif, encode_bmp, encode_gif, encode_jpeg, encode_png, encode_webp};
use crate::image_format_traits::ImageFormatTraits;

pub fn process_edit_job(job: EditJob, progress_bar: &MultiProgress) -> Result<()> {
    let step_count = get_progress_step(&job);
    let pb = progress_bar.add(ProgressBar::new(step_count));
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}")?
    );

    pb.set_message(format!("Processing {}", job.input_file.display()));

    info!("Decoding image: {}", job.input_file.display());
    pb.inc(1);
    let mut img_info = decode_image(&job.input_file)?;

    debug!("Image decoded...");

    if let Some(resize) = job.resize {
        info!("Resizing image to {}% of its original size", resize);
        pb.inc(1);
        img_info.dynamic_image = apply_resize(img_info.dynamic_image, resize)?;
        debug!("Image resized...");
    }

    if job.grayscale {
        info!("Converting image to grayscale");
        pb.inc(1);
        img_info.dynamic_image = img_info.dynamic_image.grayscale();
        debug!("Image converted to grayscale...");
    }

    info!("Determining output plan...");
    pb.inc(1);
    let (output_format, output_path) = determine_output_plan(&job, &img_info.image_meta)?;

    info!("Saving image to {}", output_path.display());
    if job.convert.is_some() {
        // The job of converting the image will be done at save time...
        pb.inc(1);
    }    
    encode_image(&img_info.dynamic_image, &output_path, output_format, &img_info.image_meta)?;
    pb.inc(1);
    
    debug!("The file [{:?}] was processed and saved as: [{:?}]", job.input_file, output_path);
    Ok(())
}

fn encode_image(image: &DynamicImage, output_path: &PathBuf, output_format: ImageFormat, metadata: &ImageMeta) -> Result<()> {
    match output_format {
        ImageFormat::Png => {
            encode_png(output_path, image, metadata)
        }
        ImageFormat::Jpeg => {
            encode_jpeg(output_path, image, metadata)
        }
        ImageFormat::Gif => {
            encode_gif(output_path, image)
        }
        ImageFormat::WebP => {
            encode_webp(output_path, image)
        }
        ImageFormat::Bmp => {
            encode_bmp(output_path, image)
        }
        ImageFormat::Avif => {
            encode_avif(output_path, image, metadata)
        }
        _ => {
            warn!("Unsupported output format: {:?}, quality may degrade.", output_format);
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

    if let Some(resize) = job.resize {
        suffix_parts.push(format!("resized{}", resize));
    }

    if job.grayscale {
        suffix_parts.push("grayscale".to_string());
    }

    if let Some(target_format) = job.convert {
        suffix_parts.push(format!("convert{:?}", target_format));
    }

    let suffix = suffix_parts.join("-");

    let stem = job.input_file.file_stem()
        .and_then(|s| s.to_str()).unwrap_or("image-name");

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
    let reader = ImageReader::open(image_path)?
        .with_guessed_format()?;

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
        width, height, color_type, orientation, icc_profile.as_ref().map_or(0, |p| p.len())
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
        color_type,
        bit_depth: get_bit_depth_from_color_type(color_type),
        original_format: format,
        original_size: ImageSize {
            width,
            height,
        },
    };

    Ok(DecodedImage {
        dynamic_image: image,
        image_meta,
    })
}

fn get_bit_depth_from_color_type(color_type: ColorType) -> u8 {
    match color_type {
        ColorType::L8 | ColorType::La8 | ColorType::Rgb8 | ColorType::Rgba8 => 8,
        ColorType::L16 | ColorType::La16 | ColorType::Rgb16 | ColorType::Rgba16 => 16,
        ColorType::Rgb32F | ColorType::Rgba32F => 32,
        _ => 8, // Default fallback
    }
}

fn apply_resize(image: DynamicImage, percent: u32) -> Result<DynamicImage> {
    let new_width = (image.width() as f64 * percent as f64 / 100.0).round() as u32;
    let new_height = (image.height() as f64 * percent as f64 / 100.0).round() as u32;

    if new_width == 0 || new_height == 0 {
        anyhow::bail!("Invalid resize parameters");
    }

    debug!("Resizing from {}x{} to {}x{}", image.width(), image.height(), new_width, new_height);

    Ok(image.resize(new_width, new_height, FilterType::Lanczos3))
}
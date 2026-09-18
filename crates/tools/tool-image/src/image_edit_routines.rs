use crate::image_encoders::{
    encode_avif, encode_bmp, encode_gif, encode_jpeg, encode_png, encode_webp,
};
use crate::image_format_traits::ImageFormatTraits;
use crate::models::{DecodedImage, EditJob, ImageMeta, ResizeSpec};
use anyhow::{Context, Result};
use image::ImageReader;
use image::imageops::FilterType;
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageFormat};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

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
        img_info.dynamic_image = apply_resize(img_info.dynamic_image, resize, job.filter)?;
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
        job.quality,
    )?;
    progress_bar.inc(inc_step);

    debug!(
        "The file [{:?}] was processed and saved as: [{:?}]",
        job.input_file, output_path
    );
    Ok(())
}

pub fn create_job_progress_bar(job: &EditJob, progress_bar: &MultiProgress) -> Result<ProgressBar> {
    let step_count = get_progress_step(job);
    let pb = progress_bar.add(ProgressBar::new(step_count));
    pb.set_style(
        ProgressStyle::default_bar()
            .template(format!("{{spinner:.blue}} [{{elapsed_precise}}] [{}] {{bar:50.green/black}} {{pos:>3}}% {{msg}}", job.input_file.display()).as_str())?,
    );

    Ok(pb)
}

fn encode_image(
    image: &DynamicImage,
    output_path: &Path,
    output_format: ImageFormat,
    metadata: &ImageMeta,
    quality: Option<u8>,
) -> Result<()> {
    if let Some(quality) = quality
        && !matches!(output_format, ImageFormat::Jpeg | ImageFormat::Avif)
    {
        warn!(
            format = ?output_format,
            quality,
            "--quality only affects JPEG and AVIF output; ignored for this format"
        );
    }

    encode_via_temp(output_path, |temp_file| match output_format {
        ImageFormat::Png => encode_png(temp_file, image, metadata),
        ImageFormat::Jpeg => encode_jpeg(temp_file, image, metadata, quality),
        ImageFormat::Gif => encode_gif(temp_file, image),
        ImageFormat::WebP => encode_webp(temp_file, image),
        ImageFormat::Bmp => encode_bmp(temp_file, image),
        ImageFormat::Avif => encode_avif(temp_file, image, metadata, quality),
        _ => {
            if fallback_format_is_lossless(output_format) {
                info!(
                    format = ?output_format,
                    "No dedicated encoder; using the image library's lossless encoder."
                );
            } else {
                warn!(
                    format = ?output_format,
                    "No dedicated encoder; the image library's encoder for this format is lossy, quality may degrade."
                );
            }
            image.save_with_format(temp_file, output_format)?;
            Ok(())
        }
    })
}

/// Builds the temp-file builder used for encoder output. On Unix the mode is
/// widened to 0o666 before the umask applies, matching what a plain
/// `File::create` would produce; tempfile's default 0o600 would otherwise
/// survive the rename and tighten every output.
#[cfg(unix)]
fn output_temp_builder() -> tempfile::Builder<'static, 'static> {
    use std::os::unix::fs::PermissionsExt;
    let mut builder = tempfile::Builder::new();
    builder.permissions(std::fs::Permissions::from_mode(0o666));
    builder
}

/// Builds the temp-file builder used for encoder output. Windows derives
/// effective permissions from the directory's ACL, so the default builder is
/// already right.
#[cfg(not(unix))]
fn output_temp_builder() -> tempfile::Builder<'static, 'static> {
    tempfile::Builder::new()
}

/// Runs `encode` against a temporary file in the destination's directory,
/// then renames it over `output_path`. A failed encode leaves the
/// destination untouched, which matters most when no operation changes the
/// file name and the destination is the input itself: a truncating write
/// there would destroy the only copy. The temp file lives next to the
/// destination because the rename must not cross filesystems.
fn encode_via_temp(output_path: &Path, encode: impl FnOnce(&PathBuf) -> Result<()>) -> Result<()> {
    // A bare filename has an empty parent, which tempfile would reject;
    // only root paths have no parent at all, and the caller already
    // refused those.
    let output_dir = match output_path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };

    let temp = output_temp_builder()
        .tempfile_in(output_dir)
        .with_context(|| {
            format!(
                "Cannot create a temporary file in '{}'",
                output_dir.display()
            )
        })?
        .into_temp_path();
    let temp_file = temp.to_path_buf();

    if let Err(encode_error) = encode(&temp_file) {
        if let Err(close_error) = temp.close() {
            warn!(
                path = %temp_file.display(),
                error = %close_error,
                "Failed to remove the temporary output file"
            );
        }
        return Err(encode_error);
    }

    // A destination being replaced keeps its own permissions rather than
    // inheriting the temp file's.
    if let Ok(existing) = std::fs::metadata(output_path)
        && let Err(error) = std::fs::set_permissions(&temp_file, existing.permissions())
    {
        warn!(
            path = %output_path.display(),
            error = %error,
            "Failed to copy the destination's permissions to the new output"
        );
    }

    temp.persist(output_path).with_context(|| {
        format!(
            "Cannot move temporary file '{}' over '{}'",
            temp_file.display(),
            output_path.display()
        )
    })?;

    Ok(())
}

/// Reports whether a format without a dedicated encoder in this tool still
/// encodes losslessly through the image library's default encoder. Formats
/// outside this list are either lossy (for example HDR's shared-exponent
/// encoding) or unsupported for writing, so the fallback warns about them.
fn fallback_format_is_lossless(format: ImageFormat) -> bool {
    matches!(
        format,
        ImageFormat::Tiff
            | ImageFormat::Pnm
            | ImageFormat::Tga
            | ImageFormat::Qoi
            | ImageFormat::Farbfeld
            | ImageFormat::Ico
    )
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

    // A bare filename like "img.png" has an empty parent, and joining onto it
    // keeps the output path relative. Only root paths have no parent at all.
    let output_path = match job.input_file.parent() {
        Some(parent) => parent.join(filename),
        None => anyhow::bail!(
            "Cannot determine an output directory for '{}': the path has no parent.",
            job.input_file.display()
        ),
    };

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
            warn!("Failed to get orientation: {}", e);
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

fn apply_resize(
    image: DynamicImage,
    resize: &ResizeSpec,
    filter: FilterType,
) -> Result<DynamicImage> {
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

            Ok(image.resize(new_width, new_height, filter))
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

            Ok(image.resize_exact(new_width, new_height, filter))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn job(input: &str) -> EditJob {
        EditJob {
            input_file: PathBuf::from(input),
            resize: None,
            grayscale: false,
            convert: None,
            quality: None,
            filter: FilterType::Lanczos3,
        }
    }

    fn meta(original_format: ImageFormat) -> ImageMeta {
        ImageMeta {
            icc: None,
            original_format,
        }
    }

    fn output_file_name(job: &EditJob, meta: &ImageMeta) -> String {
        let (_, path) = determine_output_plan(job, meta).unwrap();
        path.file_name().unwrap().to_str().unwrap().to_string()
    }

    #[test]
    fn progress_step_counts_enabled_operations() {
        let mut j = job("photo.png");
        assert_eq!(get_progress_step(&j), 2);

        j.resize = Some(ResizeSpec::Percent(50.0));
        j.grayscale = true;
        j.convert = Some(ImageFormat::Jpeg);
        assert_eq!(get_progress_step(&j), 5);
    }

    #[test]
    fn output_plan_without_operations_keeps_name_and_format() {
        let (format, _) =
            determine_output_plan(&job("photo.png"), &meta(ImageFormat::Png)).unwrap();
        assert_eq!(format, ImageFormat::Png);
        assert_eq!(
            output_file_name(&job("photo.png"), &meta(ImageFormat::Png)),
            "photo.png"
        );
    }

    #[test]
    fn output_plan_appends_operation_suffixes() {
        let mut j = job("photo.png");
        j.resize = Some(ResizeSpec::Percent(50.0));
        j.grayscale = true;
        assert_eq!(
            output_file_name(&j, &meta(ImageFormat::Png)),
            "photo-resized50pct-grayscale.png"
        );
    }

    #[test]
    fn output_plan_uses_convert_format_and_extension() {
        let mut j = job("photo.png");
        j.convert = Some(ImageFormat::Jpeg);
        let (format, _) = determine_output_plan(&j, &meta(ImageFormat::Png)).unwrap();
        assert_eq!(format, ImageFormat::Jpeg);
        assert_eq!(
            output_file_name(&j, &meta(ImageFormat::Png)),
            "photo-convertJpeg.jpg"
        );
    }

    #[test]
    fn output_plan_keeps_a_bare_filename_relative() {
        let (_, path) = determine_output_plan(&job("img.png"), &meta(ImageFormat::Png)).unwrap();
        assert_eq!(path, PathBuf::from("img.png"));
    }

    #[test]
    fn output_plan_errors_for_a_parentless_root_path() {
        let result = determine_output_plan(&job("/"), &meta(ImageFormat::Png));
        assert!(result.is_err());
    }

    #[test]
    fn fallback_classifies_lossless_and_lossy_formats() {
        assert!(fallback_format_is_lossless(ImageFormat::Tiff));
        assert!(!fallback_format_is_lossless(ImageFormat::Hdr));
    }

    #[test]
    fn apply_resize_percent_scales_dimensions() {
        let image = DynamicImage::new_rgb8(100, 100);
        let resized =
            apply_resize(image, &ResizeSpec::Percent(50.0), FilterType::Lanczos3).unwrap();
        assert_eq!(resized.width(), 50);
        assert_eq!(resized.height(), 50);
    }

    #[test]
    fn apply_resize_dimensions_sets_exact_size() {
        let image = DynamicImage::new_rgb8(100, 100);
        let resized = apply_resize(
            image,
            &ResizeSpec::Dimensions {
                width: 20.0,
                height: 10.0,
            },
            FilterType::Lanczos3,
        )
        .unwrap();
        assert_eq!(resized.width(), 20);
        assert_eq!(resized.height(), 10);
    }

    #[test]
    fn apply_resize_rejects_zero_dimension() {
        let image = DynamicImage::new_rgb8(100, 100);
        assert!(apply_resize(image, &ResizeSpec::Percent(0.4), FilterType::Lanczos3).is_err());
    }

    /// Builds a small gradient so different resize filters produce different
    /// pixels on a downscale.
    fn gradient_image() -> DynamicImage {
        let mut img = image::RgbImage::new(16, 16);
        for (x, y, pixel) in img.enumerate_pixels_mut() {
            *pixel = image::Rgb([(x * 16) as u8, (y * 16) as u8, ((x + y) * 8) as u8]);
        }
        DynamicImage::ImageRgb8(img)
    }

    #[test]
    fn apply_resize_filter_choice_changes_resampling() {
        let spec = ResizeSpec::Percent(50.0);

        let nearest = apply_resize(gradient_image(), &spec, FilterType::Nearest).unwrap();
        let gaussian = apply_resize(gradient_image(), &spec, FilterType::Gaussian).unwrap();

        assert_eq!(nearest.width(), gaussian.width());
        assert_eq!(nearest.height(), gaussian.height());
        assert_ne!(nearest.to_rgb8().as_raw(), gaussian.to_rgb8().as_raw());
    }

    fn directory_entry_count(dir: &Path) -> usize {
        std::fs::read_dir(dir).unwrap().count()
    }

    #[test]
    fn encode_via_temp_failure_leaves_an_existing_destination_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("out.png");
        std::fs::write(&dest, b"original").unwrap();

        let result = encode_via_temp(&dest, |_| anyhow::bail!("encoder failed"));

        assert!(result.is_err());
        assert_eq!(std::fs::read(&dest).unwrap(), b"original");
        assert_eq!(
            directory_entry_count(dir.path()),
            1,
            "the failed encode must not leave a temp file behind"
        );
    }

    #[test]
    fn encode_via_temp_success_replaces_the_destination_without_leftovers() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("out.png");
        std::fs::write(&dest, b"old").unwrap();

        encode_via_temp(&dest, |temp_file| {
            std::fs::write(temp_file, b"new").map_err(Into::into)
        })
        .unwrap();

        assert_eq!(std::fs::read(&dest).unwrap(), b"new");
        assert_eq!(directory_entry_count(dir.path()), 1);
    }

    #[test]
    fn encode_via_temp_creates_a_missing_destination() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("fresh.png");

        encode_via_temp(&dest, |temp_file| {
            std::fs::write(temp_file, b"fresh").map_err(Into::into)
        })
        .unwrap();

        assert_eq!(std::fs::read(&dest).unwrap(), b"fresh");
        assert_eq!(directory_entry_count(dir.path()), 1);
    }
}

use crate::models::ImageMeta;
use anyhow::Result;
use image::{ColorType, DynamicImage, ImageEncoder};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use tracing::{debug, info, warn};

pub fn encode_png(output_path: &PathBuf, image: &DynamicImage, meta: &ImageMeta) -> Result<()> {
    let file = File::create(output_path)?;
    let mut encoder = image::codecs::png::PngEncoder::new(file);

    if let Some(ref icc) = meta.icc {
        debug!("Embedding ICC profile ({} bytes) in PNG", icc.len());
        if let Err(e) = encoder.set_icc_profile(icc.clone()) {
            warn!("Failed to set ICC profile: {}", e);
        }
    }

    encoder.write_image(
        image.as_bytes(),
        image.width(),
        image.height(),
        image.color().into(),
    )?;

    Ok(())
}

/// JPEG quality, on the encoder's 1-100 scale (100 is best). The default when
/// no --quality is given; JPEG stays lossy even at 100.
const JPEG_QUALITY: u8 = 100;

pub fn encode_jpeg(
    output_path: &PathBuf,
    image: &DynamicImage,
    meta: &ImageMeta,
    quality: Option<u8>,
) -> Result<()> {
    // Convert to RGB if needed (JPEG doesn't support transparency)
    let rgb_image = match image.color() {
        ColorType::Rgb8 | ColorType::L8 => image.clone(),
        _ => {
            info!("Converting image to RGB for JPEG encoding");
            DynamicImage::ImageRgb8(image.to_rgb8())
        }
    };

    let quality = quality.unwrap_or(JPEG_QUALITY);
    debug!(quality, "Encoding JPEG...");
    let file = File::create(output_path)?;
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(file, quality);

    if let Some(ref icc) = meta.icc {
        debug!("Embedding ICC profile ({} bytes) in JPEG", icc.len());
        if let Err(e) = encoder.set_icc_profile(icc.clone()) {
            warn!("Failed to set ICC profile: {}", e);
        }
    }

    encoder.write_image(
        rgb_image.as_bytes(),
        rgb_image.width(),
        rgb_image.height(),
        rgb_image.color().into(),
    )?;

    Ok(())
}

pub fn encode_webp(output_path: &PathBuf, image: &DynamicImage) -> Result<()> {
    debug!("Encoding WebP in lossless mode");
    let file = File::create(output_path)?;
    let encoder = image::codecs::webp::WebPEncoder::new_lossless(file);

    encoder.write_image(
        image.as_bytes(),
        image.width(),
        image.height(),
        image.color().into(),
    )?;

    Ok(())
}

/// AVIF quality, on the encoder's 1-100 scale (100 is best). 95 is the default
/// when no --quality is given: near-lossless output instead of the encoder's
/// undocumented lossy defaults.
const AVIF_QUALITY: u8 = 95;

/// AVIF encoder speed, on the encoder's 1-10 scale (1 is slowest/best). 4 is
/// the tool's chosen default, trading encode time for compression efficiency.
const AVIF_SPEED: u8 = 4;

pub fn encode_avif(
    output_path: &PathBuf,
    image: &DynamicImage,
    meta: &ImageMeta,
    quality: Option<u8>,
) -> Result<()> {
    let quality = quality.unwrap_or(AVIF_QUALITY);
    debug!(quality, speed = AVIF_SPEED, "Encoding AVIF...");
    let file = File::create(output_path)?;
    let mut encoder =
        image::codecs::avif::AvifEncoder::new_with_speed_quality(file, AVIF_SPEED, quality);

    if let Some(ref icc) = meta.icc {
        debug!("Embedding ICC profile ({} bytes) in AVIF", icc.len());
        if let Err(e) = encoder.set_icc_profile(icc.clone()) {
            warn!("Failed to set ICC profile: {}", e);
        }
    }

    encoder.write_image(
        image.as_bytes(),
        image.width(),
        image.height(),
        image.color().into(),
    )?;

    Ok(())
}

pub fn encode_bmp(output_path: &PathBuf, image: &DynamicImage) -> Result<()> {
    debug!("Encoding BMP...");
    let mut file = File::create(output_path)?;
    let encoder = image::codecs::bmp::BmpEncoder::new(&mut file);

    encoder.write_image(
        image.as_bytes(),
        image.width(),
        image.height(),
        image.color().into(),
    )?;

    Ok(())
}

pub fn encode_gif(output_path: &PathBuf, image: &DynamicImage) -> Result<()> {
    debug!("Encoding GIF");

    let mut file = File::create(output_path)?;

    // Convert to RGBA for quantization
    let rgba_image = image.to_rgba8();

    // Create GIF encoder
    let mut encoder = gif::Encoder::new(
        &mut file,
        rgba_image.width() as u16,
        rgba_image.height() as u16,
        &[],
    )?;

    // Use global color table
    encoder.set_repeat(gif::Repeat::Infinite)?;

    // Quantize the image to 256 colors
    encode_gif_with_quantization(&rgba_image, &mut encoder)
}

fn encode_gif_with_quantization<W: Write>(
    rgba_image: &image::RgbaImage,
    encoder: &mut gif::Encoder<W>,
) -> Result<()> {
    use imagequant::{Attributes, RGBA};

    debug!("Using high-quality quantization for GIF");

    let width = rgba_image.width();
    let height = rgba_image.height();
    let pixels: Vec<RGBA> = rgba_image
        .pixels()
        .map(|p| RGBA::new(p[0], p[1], p[2], p[3]))
        .collect();

    let mut liq = Attributes::new();
    liq.set_quality(0, 100)?;

    let mut img = liq.new_image(pixels.clone(), width as usize, height as usize, 0.0)?;

    let mut result = liq.quantize(&mut img)?;

    let (palette, pixels) = result.remapped(&mut img)?;

    // Convert palette to GIF format
    let transparent_values = palette.iter().map(|p| p.a).collect::<Vec<_>>();
    let first_available_alpha: u8 =
        transparent_values.iter().position(|a| *a != 0).unwrap_or(0) as u8;
    let transparent = if first_available_alpha > 0 {
        Some(first_available_alpha)
    } else {
        None
    };

    let mut frame =
        gif::Frame::from_indexed_pixels(width as u16, height as u16, pixels.clone(), transparent);
    frame.delay = 0; // Static image

    encoder.write_frame(&frame)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, Rgba, RgbaImage};

    /// Builds an 8x8 RGBA image with a per-pixel gradient, including
    /// non-opaque alpha values so alpha handling is exercised.
    fn tiny_rgba() -> DynamicImage {
        let mut img = RgbaImage::new(8, 8);
        for (x, y, pixel) in img.enumerate_pixels_mut() {
            *pixel = Rgba([
                (x * 31) as u8,
                (y * 31) as u8,
                ((x + y) * 15) as u8,
                255 - (x * 8) as u8,
            ]);
        }
        DynamicImage::ImageRgba8(img)
    }

    fn no_meta() -> ImageMeta {
        ImageMeta {
            icc: None,
            original_format: ImageFormat::Png,
        }
    }

    #[test]
    fn png_round_trip_is_pixel_identical() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.png");
        let original = tiny_rgba();

        encode_png(&path, &original, &no_meta()).unwrap();
        let decoded = image::open(&path).unwrap();

        assert_eq!(decoded.to_rgba8().as_raw(), original.to_rgba8().as_raw());
    }

    #[test]
    fn webp_round_trip_is_pixel_identical() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.webp");
        let original = tiny_rgba();

        encode_webp(&path, &original).unwrap();
        let decoded = image::open(&path).unwrap();

        assert_eq!(decoded.to_rgba8().as_raw(), original.to_rgba8().as_raw());
    }

    #[test]
    fn jpeg_from_rgba_is_opaque_rgb() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.jpg");

        encode_jpeg(&path, &tiny_rgba(), &no_meta(), None).unwrap();
        let decoded = image::open(&path).unwrap();

        assert_eq!(decoded.color(), ColorType::Rgb8);
        assert!(!decoded.color().has_alpha());
    }

    /// Builds a 64x64 RGB image with per-pixel variation; large enough that
    /// JPEG output size is dominated by image data rather than headers.
    fn gradient_rgb_64() -> DynamicImage {
        let mut img = image::RgbImage::new(64, 64);
        for (x, y, pixel) in img.enumerate_pixels_mut() {
            *pixel = image::Rgb([
                ((x * 7) % 256) as u8,
                ((y * 13) % 256) as u8,
                (((x + y) * 5) % 256) as u8,
            ]);
        }
        DynamicImage::ImageRgb8(img)
    }

    #[test]
    fn jpeg_quality_setting_changes_output_size() {
        let dir = tempfile::tempdir().unwrap();
        let low_path = dir.path().join("low.jpg");
        let high_path = dir.path().join("high.jpg");
        let image = gradient_rgb_64();

        encode_jpeg(&low_path, &image, &no_meta(), Some(10)).unwrap();
        encode_jpeg(&high_path, &image, &no_meta(), Some(95)).unwrap();

        let low_len = std::fs::metadata(&low_path).unwrap().len();
        let high_len = std::fs::metadata(&high_path).unwrap().len();
        assert!(
            low_len < high_len,
            "quality 10 output ({low_len} bytes) should be smaller than quality 95 ({high_len} bytes)"
        );
    }
}

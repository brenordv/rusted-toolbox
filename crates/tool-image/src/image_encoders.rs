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

pub fn encode_jpeg(output_path: &PathBuf, image: &DynamicImage, meta: &ImageMeta) -> Result<()> {
    // Convert to RGB if needed (JPEG doesn't support transparency)
    let rgb_image = match image.color() {
        ColorType::Rgb8 | ColorType::L8 => image.clone(),
        _ => {
            info!("Converting image to RGB for JPEG encoding");
            DynamicImage::ImageRgb8(image.to_rgb8())
        }
    };

    let file = File::create(output_path)?;
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(file, 100);

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

pub fn encode_avif(output_path: &PathBuf, image: &DynamicImage, meta: &ImageMeta) -> Result<()> {
    debug!("Encoding AVIF...");
    let file = File::create(output_path)?;
    let mut encoder = image::codecs::avif::AvifEncoder::new(file);

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

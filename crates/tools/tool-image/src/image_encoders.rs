use crate::models::ImageMeta;
use anyhow::{Context, Result, bail};
use image::{ColorType, DynamicImage, ImageEncoder};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use tracing::{debug, info, warn};

pub fn encode_png(output_path: &PathBuf, image: &DynamicImage, meta: &ImageMeta) -> Result<()> {
    let file = File::create(output_path)
        .with_context(|| format!("Cannot create output file '{}'", output_path.display()))?;
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
    let file = File::create(output_path)
        .with_context(|| format!("Cannot create output file '{}'", output_path.display()))?;
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
    let file = File::create(output_path)
        .with_context(|| format!("Cannot create output file '{}'", output_path.display()))?;
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
    let file = File::create(output_path)
        .with_context(|| format!("Cannot create output file '{}'", output_path.display()))?;
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
    let mut file = File::create(output_path)
        .with_context(|| format!("Cannot create output file '{}'", output_path.display()))?;
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

    // Convert to RGBA for quantization
    let rgba_image = image.to_rgba8();

    // GIF stores dimensions as u16, so anything past 65535 cannot be encoded.
    let (Ok(width), Ok(height)) = (
        u16::try_from(rgba_image.width()),
        u16::try_from(rgba_image.height()),
    ) else {
        bail!(
            "Cannot encode a {}x{} image as GIF: the format caps each dimension at {} pixels",
            rgba_image.width(),
            rgba_image.height(),
            u16::MAX
        );
    };

    // All fallible pixel work happens before the output file exists.
    let quantized = quantize_for_gif(&rgba_image)?;

    let file = File::create(output_path)
        .with_context(|| format!("Cannot create output file '{}'", output_path.display()))?;

    write_gif(file, width, height, quantized)
}

/// A GIF-ready image: an RGB global color table, pixels as palette indexes,
/// and the palette index that renders transparent, when one exists.
struct QuantizedGif {
    palette_rgb: Vec<u8>,
    indexed_pixels: Vec<u8>,
    transparent: Option<u8>,
}

/// Quantizes an RGBA image to at most 256 colors for GIF encoding.
fn quantize_for_gif(rgba_image: &image::RgbaImage) -> Result<QuantizedGif> {
    use imagequant::{Attributes, RGBA};

    debug!("Using high-quality quantization for GIF");

    let width = rgba_image.width() as usize;
    let height = rgba_image.height() as usize;
    let pixels: Vec<RGBA> = rgba_image
        .pixels()
        .map(|p| RGBA::new(p[0], p[1], p[2], p[3]))
        .collect();

    let mut liq = Attributes::new();
    liq.set_quality(0, 100)?;

    let mut img = liq.new_image(pixels, width, height, 0.0)?;

    let mut result = liq.quantize(&mut img)?;

    let (palette, indexed_pixels) = result.remapped(&mut img)?;

    // The first fully transparent palette entry becomes the GIF transparent
    // index; the try_from never fails in practice because imagequant palettes
    // hold at most 256 entries.
    let transparent = palette
        .iter()
        .position(|color| color.a == 0)
        .and_then(|index| u8::try_from(index).ok());

    let palette_rgb = palette
        .iter()
        .flat_map(|color| [color.r, color.g, color.b])
        .collect();

    Ok(QuantizedGif {
        palette_rgb,
        indexed_pixels,
        transparent,
    })
}

/// Writes a single-frame GIF through `writer`, using the quantized palette as
/// the global color table.
fn write_gif<W: Write>(writer: W, width: u16, height: u16, quantized: QuantizedGif) -> Result<()> {
    let mut encoder = gif::Encoder::new(writer, width, height, &quantized.palette_rgb)?;
    encoder.set_repeat(gif::Repeat::Infinite)?;

    let mut frame = gif::Frame::from_indexed_pixels(
        width,
        height,
        quantized.indexed_pixels,
        quantized.transparent,
    );
    frame.delay = 0; // Static image

    encoder.write_frame(&frame)?;

    // The encoder's Drop discards trailer-write errors; into_inner propagates
    // them and hands the writer back, so the handle is closed before any
    // cleanup runs on the path it wrote to.
    encoder.into_inner()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use common_cli::test_writers::FailAfter;
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
    fn gif_round_trip_decodes_with_original_dimensions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.gif");
        let original = tiny_rgba();

        encode_gif(&path, &original).unwrap();
        let decoded = image::open(&path).unwrap();

        assert_eq!(decoded.width(), original.width());
        assert_eq!(decoded.height(), original.height());
    }

    /// Builds an 8x8 image whose right half is fully transparent.
    fn half_transparent_rgba() -> DynamicImage {
        let mut img = RgbaImage::new(8, 8);
        for (x, _y, pixel) in img.enumerate_pixels_mut() {
            *pixel = if x < 4 {
                Rgba([255, 0, 0, 255])
            } else {
                Rgba([0, 0, 0, 0])
            };
        }
        DynamicImage::ImageRgba8(img)
    }

    #[test]
    fn gif_keeps_fully_transparent_pixels_transparent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.gif");

        encode_gif(&path, &half_transparent_rgba()).unwrap();
        let decoded = image::open(&path).unwrap().to_rgba8();

        assert!(decoded.pixels().any(|p| p[3] == 0));
        assert!(decoded.pixels().any(|p| p[3] == 255));
    }

    #[test]
    fn gif_opaque_image_stays_opaque() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.gif");

        encode_gif(&path, &gradient_rgb_64()).unwrap();
        let decoded = image::open(&path).unwrap().to_rgba8();

        assert!(decoded.pixels().all(|p| p[3] == 255));
    }

    #[test]
    fn gif_rejects_dimensions_over_the_format_ceiling() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.gif");
        let image = DynamicImage::ImageRgba8(RgbaImage::new(65_536, 1));

        let result = encode_gif(&path, &image);

        assert!(result.is_err());
        assert!(!path.exists());
    }

    fn quantized_tiny() -> QuantizedGif {
        quantize_for_gif(&tiny_rgba().to_rgba8()).unwrap()
    }

    #[test]
    fn write_gif_surfaces_immediate_writer_failure() {
        // Encoder::new writes the screen descriptor immediately, so a writer
        // that fails on the first byte errors during construction.
        let result = write_gif(FailAfter::storage_full(0), 8, 8, quantized_tiny());

        assert!(result.is_err());
    }

    #[test]
    fn write_gif_surfaces_late_writer_failure() {
        let mut full = Vec::new();
        write_gif(&mut full, 8, 8, quantized_tiny()).unwrap();

        // One byte short of a complete file: construction succeeds and the
        // failure surfaces from the frame write or the trailer.
        let result = write_gif(
            FailAfter::storage_full(full.len() - 1),
            8,
            8,
            quantized_tiny(),
        );

        assert!(result.is_err());
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

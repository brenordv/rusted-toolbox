use std::path::PathBuf;
use clap::ValueEnum;
use image::{ColorType, DynamicImage, ImageFormat};
use indicatif::MultiProgress;

#[derive(ValueEnum, Clone, Debug)]
pub enum TargetFormat {
    Png,
    Jpg,
    Jpeg,
    Gif,
    Webp,
    Avif,
    Tiff,
    Bmp,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum FilterType {
    Nearest,
    Triangle,
    CatmullRom,
    Gaussian,
    Lanczos3,
}

pub struct EditArgs {
    pub input_files: Vec<PathBuf>,
    pub resize: Option<u32>,
    pub grayscale: bool,
    pub convert: Option<ImageFormat>
}

pub struct EditJob {
    pub input_file: PathBuf,
    pub resize: Option<u32>,
    pub grayscale: bool,
    pub convert: Option<ImageFormat>,
}

pub struct DecodedImage {
    pub dynamic_image: DynamicImage,
    pub image_meta: ImageMeta,
}

pub struct ImageMeta {
    pub icc: Option<Vec<u8>>,
    pub color_type: ColorType,
    pub bit_depth: u8,
    pub original_format: ImageFormat,
    pub original_size: ImageSize
}

pub struct ImageSize {
    pub width: u32,
    pub height: u32,
}

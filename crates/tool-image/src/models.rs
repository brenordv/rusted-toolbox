use clap::ValueEnum;
use image::{DynamicImage, ImageFormat};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

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
    pub resize: Option<ResizeSpec>,
    pub grayscale: bool,
    pub convert: Option<ImageFormat>,
}

pub struct EditJob {
    pub input_file: PathBuf,
    pub resize: Option<ResizeSpec>,
    pub grayscale: bool,
    pub convert: Option<ImageFormat>,
}

#[derive(Clone, Debug)]
pub enum ResizeSpec {
    Percent(f64),
    Dimensions { width: f64, height: f64 },
}

impl ResizeSpec {
    pub fn suffix(&self) -> String {
        match self {
            ResizeSpec::Percent(percent) => format!("resized{}pct", format_decimal(*percent)),
            ResizeSpec::Dimensions { width, height } => {
                format!(
                    "resized{}x{}",
                    format_decimal(*width),
                    format_decimal(*height)
                )
            }
        }
    }
}

impl std::fmt::Display for ResizeSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResizeSpec::Percent(percent) => write!(f, "{}%", format_decimal(*percent)),
            ResizeSpec::Dimensions { width, height } => {
                write!(f, "{}x{}", format_decimal(*width), format_decimal(*height))
            }
        }
    }
}

fn format_decimal(value: f64) -> String {
    let mut formatted = format!("{:.4}", value);
    if formatted.contains('.') {
        while formatted.ends_with('0') {
            formatted.pop();
        }
        if formatted.ends_with('.') {
            formatted.pop();
        }
    }
    if formatted.is_empty() {
        "0".to_string()
    } else {
        formatted
    }
}

pub struct DecodedImage {
    pub dynamic_image: DynamicImage,
    pub image_meta: ImageMeta,
}

pub struct ImageMeta {
    pub icc: Option<Vec<u8>>,
    pub original_format: ImageFormat,
}

pub struct ProcessingStatsInner {
    success_count: AtomicUsize,
    error_count: AtomicUsize,
}

impl ProcessingStatsInner {
    pub fn new() -> Self {
        Self {
            success_count: AtomicUsize::new(0),
            error_count: AtomicUsize::new(0),
        }
    }

    pub fn increment_success(&self) {
        self.success_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_error(&self) {
        self.error_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn into_stats(self) -> ProcessingStats {
        let success_count = self.success_count.into_inner();
        let error_count = self.error_count.into_inner();

        ProcessingStats {
            success_count,
            error_count,
            total_count: success_count + error_count,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ProcessingStats {
    pub success_count: usize,
    pub error_count: usize,
    pub total_count: usize,
}

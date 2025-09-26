use image::ImageFormat;

pub trait ImageFormatTraits {
    fn to_file_extension(&self) -> String;
}

impl ImageFormatTraits for ImageFormat {
    fn to_file_extension(&self) -> String {
        match self {
            ImageFormat::Png => "png".to_string(),
            ImageFormat::Jpeg => "jpg".to_string(),
            ImageFormat::Gif => "gif".to_string(),
            ImageFormat::WebP => "webp".to_string(),
            ImageFormat::Pnm => "pnm".to_string(),
            ImageFormat::Tiff => "tiff".to_string(),
            ImageFormat::Tga => "tga".to_string(),
            ImageFormat::Dds => "dds".to_string(),
            ImageFormat::Bmp => "bmp".to_string(),
            ImageFormat::Ico => "ico".to_string(),
            ImageFormat::Hdr => "hdr".to_string(),
            ImageFormat::OpenExr => "exr".to_string(),
            ImageFormat::Farbfeld => "ff".to_string(),
            ImageFormat::Avif => "avif".to_string(),
            ImageFormat::Qoi => "qoi".to_string(),
            _ => "".to_string(),
        }
    }
}

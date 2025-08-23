use image::ImageFormat;

pub trait StringExt {
    fn to_image_format(&self) -> ImageFormat;
}

impl StringExt for String {
    fn to_image_format(&self) -> ImageFormat {
        match self.to_lowercase().as_str() {
            "jpg" | "jpeg" => ImageFormat::Jpeg,
            "png" => ImageFormat::Png,
            "gif" => ImageFormat::Gif,
            "bmp" => ImageFormat::Bmp,
            "ico" => ImageFormat::Ico,
            "tiff" | "tif" => ImageFormat::Tiff,
            "tga" => ImageFormat::Tga,
            "webp" => ImageFormat::WebP,
            "dds" => ImageFormat::Dds,
            "hdr" => ImageFormat::Hdr,
            "pnm" | "pbm" => ImageFormat::Pnm,
            "exr" => ImageFormat::OpenExr,
            "avif" => ImageFormat::Avif,
            "qoi" => ImageFormat::Qoi,
            _ => panic!("Unknown image format: {}", self),
        }
    }
}

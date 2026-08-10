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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_formats_to_extensions() {
        assert_eq!(ImageFormat::Png.to_file_extension(), "png");
        assert_eq!(ImageFormat::Jpeg.to_file_extension(), "jpg");
        assert_eq!(ImageFormat::Gif.to_file_extension(), "gif");
        assert_eq!(ImageFormat::WebP.to_file_extension(), "webp");
        assert_eq!(ImageFormat::Avif.to_file_extension(), "avif");
    }

    #[test]
    fn extension_round_trips_through_string_ext() {
        use crate::string_traits::StringExt;
        for format in [
            ImageFormat::Png,
            ImageFormat::Jpeg,
            ImageFormat::Gif,
            ImageFormat::WebP,
            ImageFormat::Bmp,
        ] {
            let extension = format.to_file_extension();
            assert_eq!(extension.to_image_format(), format);
        }
    }
}

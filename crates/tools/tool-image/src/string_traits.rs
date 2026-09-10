use image::ImageFormat;

/// Format names (lowercase) that `to_image_format` maps to an `ImageFormat`.
/// The `--convert` value parser in `cli_utils` validates against this list,
/// so it must stay in sync with the match arms below; the
/// `every_supported_name_maps_to_a_format` test guards that invariant.
pub const SUPPORTED_FORMAT_NAMES: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "bmp", "ico", "tiff", "tif", "tga", "webp", "dds", "hdr", "pnm",
    "pbm", "exr", "avif", "qoi",
];

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
            // CLI input never reaches this arm: the --convert value parser
            // only admits strings from SUPPORTED_FORMAT_NAMES. It guards
            // direct callers passing a string outside that list.
            _ => panic!("Unknown image format: {}", self),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_common_extensions() {
        assert_eq!("png".to_string().to_image_format(), ImageFormat::Png);
        assert_eq!("jpg".to_string().to_image_format(), ImageFormat::Jpeg);
        assert_eq!("jpeg".to_string().to_image_format(), ImageFormat::Jpeg);
        assert_eq!("webp".to_string().to_image_format(), ImageFormat::WebP);
        assert_eq!("gif".to_string().to_image_format(), ImageFormat::Gif);
    }

    #[test]
    fn mapping_is_case_insensitive() {
        assert_eq!("PNG".to_string().to_image_format(), ImageFormat::Png);
        assert_eq!("JPEG".to_string().to_image_format(), ImageFormat::Jpeg);
    }

    #[test]
    #[should_panic(expected = "Unknown image format")]
    fn unknown_extension_panics() {
        let _ = "not-a-format".to_string().to_image_format();
    }

    #[test]
    fn every_supported_name_maps_to_a_format() {
        for name in SUPPORTED_FORMAT_NAMES {
            let _ = name.to_string().to_image_format();
        }
    }
}

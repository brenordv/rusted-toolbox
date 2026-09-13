use crate::models::{HowMode, QrCodeConfig};
use anyhow::Result;
use chrono::Local;
use common_utils::datetime_utc_utils::DateTimeUtilsExt;
use image::{ImageBuffer, Luma};
use qrcodegen::{QrCode, QrCodeEcc};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use tracing::{debug, info, warn};

pub fn generate_qrcode(args: &QrCodeConfig) -> Result<()> {
    info!("Preparing QR code data...");

    // For wifi payloads, `log_data` carries a masked password so debug logging
    // never exposes the real one.
    let (data, log_data) = match args.get_payload() {
        HowMode::TextPayload(text) => {
            let log_data = text.clone();
            (text, log_data)
        }
        HowMode::WifiPayload(ssid, password, auth) => (
            build_wifi_payload(&ssid, &password, &auth),
            build_wifi_payload(&ssid, "********", &auth),
        ),
    };

    let data_size = data.len();

    debug!(size = log_data.len(), data = %log_data, "prepared QR payload");
    info!("Generating QR code...");

    let qr_code_ecc = choose_ecc(data_size);
    let qr_code = qrcodegen::QrCode::encode_text(&data, qr_code_ecc)?;
    info!("QR code generated.");

    if !args.dont_print {
        print_qr_to_console(&qr_code);
    }

    if args.output_format.is_none() && args.output_file.is_none() {
        return Ok(());
    }

    // Explicit -f wins; without it, a recognized -o extension picks the
    // format, and only an unrecognized or missing extension falls back to png.
    let filename_ext = match (&args.output_format, &args.output_file) {
        (Some(format), _) => format.clone(),
        (None, Some(output_file)) => {
            inferred_output_format(output_file).unwrap_or_else(|| "png".to_string())
        }
        (None, None) => "png".to_string(),
    };

    if let Some(output_file) = &args.output_file {
        if let Some(ext) = mismatched_output_extension(output_file, &filename_ext) {
            warn!(
                extension = %ext,
                format = %filename_ext,
                output_file = %output_file,
                "output file extension does not match the output format; the format extension will be appended"
            );
        }
    }

    let mut filename = args.output_file.clone().unwrap_or_else(|| {
        format!(
            "qrcode--{}.{}",
            Local::now().get_datetime_as_filename_safe_string(),
            filename_ext
        )
    });

    if !filename_matches_format(&filename, &filename_ext) {
        filename.push_str(&format!(".{}", filename_ext));
    }

    let qr_code_scale = 30;

    match filename_ext.to_lowercase().trim() {
        "svg" => {
            info!("Saving QR code to SVG...");
            save_qr_to_svg(&qr_code, qr_code_scale, &filename)?;
        }
        _ => {
            info!("Saving QR code to image ({})...", filename_ext);
            save_qr_to_image(&qr_code, qr_code_scale, &filename)?;
        }
    }

    Ok(())
}

/// Builds the `WIFI:T:{auth};S:{ssid};P:{password};;` payload, escaping the
/// SSID and password fields per the zxing barcode-contents spec.
fn build_wifi_payload(ssid: &str, password: &str, auth: &str) -> String {
    format!(
        "WIFI:T:{};S:{};P:{};;",
        auth,
        escape_wifi_field(ssid),
        escape_wifi_field(password)
    )
}

/// Backslash-escapes the characters the wifi payload spec reserves in field
/// values: backslash, semicolon, comma, colon, and double quote.
fn escape_wifi_field(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for c in value.chars() {
        if matches!(c, '\\' | ';' | ',' | ':' | '"') {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}

/// Extensions eligible for `-o` format inference: the raster formats the
/// grayscale (Luma8) QR buffer encodes through `image::save` at this image
/// size; the test suite writes each one. Recognized-but-unencodable
/// extensions (ico's 256px cap, webp/gif rejecting L8, avif behind a
/// non-default feature, dds with no encoder) stay out so inference can never
/// turn into a hard save failure.
const INFERABLE_RASTER_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "bmp", "tif", "tiff", "tga"];

/// Infers the output format from the output file's extension: `svg` plus the
/// [`INFERABLE_RASTER_EXTENSIONS`] qualify, kept in the extension's original
/// casing so the filename is used as given. Returns `None` for a missing or
/// non-inferable extension; the caller then falls back to the png default,
/// where the mismatch warning and appended extension still apply.
fn inferred_output_format(output_file: &str) -> Option<String> {
    let ext = Path::new(output_file).extension()?.to_str()?;
    if ext.eq_ignore_ascii_case("svg")
        || INFERABLE_RASTER_EXTENSIONS
            .iter()
            .any(|known| ext.eq_ignore_ascii_case(known))
    {
        Some(ext.to_string())
    } else {
        None
    }
}

/// Reports whether `filename` already carries `format`, compared
/// case-insensitively like the save dispatch: either its extension matches,
/// or the whole basename is the dotfile form `.{format}` (which has no
/// `Path::extension` but needs no appending either).
fn filename_matches_format(filename: &str, format: &str) -> bool {
    let path = Path::new(filename);
    if let Some(ext) = path.extension().and_then(|ext| ext.to_str()) {
        return ext.eq_ignore_ascii_case(format);
    }
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.strip_prefix('.')
                .is_some_and(|rest| rest.eq_ignore_ascii_case(format))
        })
}

/// Returns the extension of `output_file` when the saved file will end up as
/// `{output_file}.{format}`, carrying an extension that contradicts the
/// effective output format. Returns `None` when the filename already carries
/// the format (compared case-insensitively) or has no extension.
fn mismatched_output_extension(output_file: &str, format: &str) -> Option<String> {
    Path::new(output_file)
        .extension()
        .and_then(|ext| ext.to_str())
        .filter(|ext| !ext.eq_ignore_ascii_case(format))
        .map(|ext| ext.to_string())
}

fn save_qr_to_image(qr: &QrCode, scale: u32, path: &str) -> Result<()> {
    let size = qr.size() as u32;
    let border_modules: u32 = 2; // thin quiet zone (in modules)
    let quiet = border_modules * scale;
    let img_size = (size * scale) + (2 * quiet);

    let mut img = ImageBuffer::from_pixel(img_size, img_size, Luma([255u8])); // white background

    for y in 0..size {
        for x in 0..size {
            if qr.get_module(x as i32, y as i32) {
                let x0 = quiet + x * scale;
                let y0 = quiet + y * scale;
                for dy in 0..scale {
                    for dx in 0..scale {
                        img.put_pixel(x0 + dx, y0 + dy, Luma([0u8]));
                    }
                }
            }
        }
    }

    img.save(path)?;
    Ok(())
}

fn save_qr_to_svg(qr: &QrCode, scale: u32, path: &str) -> Result<()> {
    let size = qr.size() as u32;
    let border_modules: u32 = 2;
    let quiet = border_modules * scale;
    let dim = (size * scale) + (2 * quiet);

    let mut file = File::create(path)?;
    writeln!(
        file,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {dim} {dim}" width="{dim}" height="{dim}" shape-rendering="crispEdges">"#
    )?;
    writeln!(file, r#"<rect width="100%" height="100%" fill="white"/>"#)?;

    for y in 0..size {
        for x in 0..size {
            if qr.get_module(x as i32, y as i32) {
                let sx = quiet + x * scale;
                let sy = quiet + y * scale;
                writeln!(
                    file,
                    r#"<rect x="{sx}" y="{sy}" width="{scale}" height="{scale}" fill="black"/>"#
                )?;
            }
        }
    }

    writeln!(file, "</svg>")?;
    Ok(())
}

fn print_qr_to_console(qr: &QrCode) {
    let border = 1;
    for y in -border..qr.size() + border {
        for x in -border..qr.size() + border {
            let c = if qr.get_module(x, y) { "██" } else { "  " };
            print!("{}", c);
        }
        println!();
    }
}

fn choose_ecc(len: usize) -> QrCodeEcc {
    match len {
        0..=50 => QrCodeEcc::High,
        51..=100 => QrCodeEcc::Quartile,
        101..=300 => QrCodeEcc::Medium,
        _ => QrCodeEcc::Low,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::QrCodePayload;
    use tempfile::tempdir;

    #[test]
    fn choose_ecc_maps_length_to_error_correction_level() {
        assert!(matches!(choose_ecc(0), QrCodeEcc::High));
        assert!(matches!(choose_ecc(50), QrCodeEcc::High));
        assert!(matches!(choose_ecc(51), QrCodeEcc::Quartile));
        assert!(matches!(choose_ecc(100), QrCodeEcc::Quartile));
        assert!(matches!(choose_ecc(101), QrCodeEcc::Medium));
        assert!(matches!(choose_ecc(300), QrCodeEcc::Medium));
        assert!(matches!(choose_ecc(301), QrCodeEcc::Low));
    }

    fn text_config(output_format: Option<String>, output_file: Option<String>) -> QrCodeConfig {
        QrCodeConfig::new(
            QrCodePayload::new(Some("https://example.com".to_string()), None, None, None),
            true,
            output_format,
            output_file,
        )
    }

    #[test]
    fn build_wifi_payload_escapes_reserved_characters() {
        let payload = build_wifi_payload(r#"my;net:work"#, r#"pa\ss,wo"rd"#, "WPA");

        assert_eq!(payload, r#"WIFI:T:WPA;S:my\;net\:work;P:pa\\ss\,wo\"rd;;"#);
    }

    #[test]
    fn build_wifi_payload_leaves_plain_fields_untouched() {
        assert_eq!(
            build_wifi_payload("net", "secret", "nopass"),
            "WIFI:T:nopass;S:net;P:secret;;"
        );
    }

    #[test]
    fn inferred_output_format_recognizes_svg_and_raster_extensions() {
        assert_eq!(inferred_output_format("x.svg"), Some("svg".to_string()));
        assert_eq!(inferred_output_format("x.SVG"), Some("SVG".to_string()));
        assert_eq!(inferred_output_format("x.jpg"), Some("jpg".to_string()));
    }

    #[test]
    fn inferred_output_format_rejects_unknown_or_missing_extension() {
        assert_eq!(inferred_output_format("x.txt"), None);
        assert_eq!(inferred_output_format("x"), None);
    }

    #[test]
    fn inferred_output_format_rejects_recognized_but_unencodable_extensions() {
        assert_eq!(inferred_output_format("x.ico"), None);
        assert_eq!(inferred_output_format("x.webp"), None);
        assert_eq!(inferred_output_format("x.gif"), None);
    }

    #[test]
    fn generate_qrcode_writes_every_inferable_raster_extension() {
        let dir = tempdir().unwrap();
        for ext in INFERABLE_RASTER_EXTENSIONS {
            let path = dir.path().join(format!("code.{ext}"));
            let cfg = text_config(None, Some(path.to_string_lossy().to_string()));

            generate_qrcode(&cfg).unwrap_or_else(|e| panic!("saving .{ext} failed: {e}"));

            assert!(
                std::fs::metadata(&path).unwrap().len() > 0,
                ".{ext} file is empty"
            );
        }
    }

    #[test]
    fn generate_qrcode_infers_svg_from_output_extension() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("code.svg");
        let cfg = text_config(None, Some(path.to_string_lossy().to_string()));

        generate_qrcode(&cfg).unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("<svg"));
    }

    #[test]
    fn mismatched_output_extension_flags_contradicting_extension() {
        assert_eq!(
            mismatched_output_extension("x.svg", "png"),
            Some("svg".to_string())
        );
    }

    #[test]
    fn mismatched_output_extension_accepts_matching_or_missing_extension() {
        assert_eq!(mismatched_output_extension("x.png", "png"), None);
        assert_eq!(mismatched_output_extension("x", "png"), None);
    }

    #[test]
    fn mismatched_output_extension_compares_case_insensitively() {
        assert_eq!(mismatched_output_extension("x.PNG", "png"), None);
        assert_eq!(mismatched_output_extension("x.svg", "SVG"), None);
    }

    #[test]
    fn filename_matches_format_compares_case_insensitively() {
        assert!(filename_matches_format("x.png", "png"));
        assert!(filename_matches_format("x.png", "PNG"));
        assert!(filename_matches_format("x.SVG", "svg"));
        assert!(!filename_matches_format("x.txt", "png"));
        assert!(!filename_matches_format("x", "png"));
    }

    #[test]
    fn filename_matches_format_accepts_the_dotfile_form() {
        assert!(filename_matches_format(".png", "png"));
        assert!(filename_matches_format(".PNG", "png"));
        assert!(!filename_matches_format(".txt", "png"));
    }

    #[test]
    fn generate_qrcode_keeps_filename_when_format_differs_only_in_case() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("code.png");
        let cfg = text_config(
            Some("PNG".to_string()),
            Some(path.to_string_lossy().to_string()),
        );

        generate_qrcode(&cfg).unwrap();

        assert!(std::fs::metadata(&path).unwrap().len() > 0);
        assert!(!dir.path().join("code.png.PNG").exists());
    }

    #[test]
    fn generate_qrcode_keeps_uppercase_filename_for_svg_format() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("code.SVG");
        let cfg = text_config(
            Some("svg".to_string()),
            Some(path.to_string_lossy().to_string()),
        );

        generate_qrcode(&cfg).unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("<svg"));
        assert!(!dir.path().join("code.SVG.svg").exists());
    }

    #[test]
    fn generate_qrcode_infers_format_from_uppercase_extension() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("code.PNG");
        let cfg = text_config(None, Some(path.to_string_lossy().to_string()));

        generate_qrcode(&cfg).unwrap();

        assert!(std::fs::metadata(&path).unwrap().len() > 0);
        assert!(!dir.path().join("code.PNG.PNG").exists());
    }

    #[test]
    fn generate_qrcode_without_output_returns_ok() {
        let cfg = text_config(None, None);

        assert!(generate_qrcode(&cfg).is_ok());
    }

    #[test]
    fn generate_qrcode_rejects_oversized_text() {
        let cfg = QrCodeConfig::new(
            QrCodePayload::new(Some("a".repeat(5000)), None, None, None),
            true,
            None,
            None,
        );

        assert!(generate_qrcode(&cfg).is_err());
    }

    #[test]
    fn generate_qrcode_writes_png_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("code.png");
        let cfg = text_config(
            Some("png".to_string()),
            Some(path.to_string_lossy().to_string()),
        );

        generate_qrcode(&cfg).unwrap();

        let metadata = std::fs::metadata(&path).unwrap();
        assert!(metadata.len() > 0);
    }

    #[test]
    fn generate_qrcode_writes_svg_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("code.svg");
        let cfg = text_config(
            Some("svg".to_string()),
            Some(path.to_string_lossy().to_string()),
        );

        generate_qrcode(&cfg).unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("<svg"));
        assert!(contents.contains("</svg>"));
    }
}

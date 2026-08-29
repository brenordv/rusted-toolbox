use crate::models::{HowMode, QrCodeConfig};
use anyhow::Result;
use chrono::Local;
use common_utils::datetime_utc_utils::DateTimeUtilsExt;
use image::{ImageBuffer, Luma};
use qrcodegen::{QrCode, QrCodeEcc};
use std::fs::File;
use std::io::Write;
use tracing::{debug, info};

pub fn generate_qrcode(args: &QrCodeConfig) -> Result<()> {
    info!("Preparing QR code data...");

    let data = match args.get_payload() {
        HowMode::TextPayload(text) => text,
        HowMode::WifiPayload(ssid, password, auth) => {
            format!("WIFI:T:{};S:{};P:{};;", auth, ssid, password)
        }
    };

    let data_size = data.len();

    debug!("Data [{}]: {}", data_size, data);
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

    let filename_ext = args
        .output_format
        .clone()
        .unwrap_or_else(|| "png".to_string());

    let mut filename = args.output_file.clone().unwrap_or_else(|| {
        format!(
            "qrcode--{}.{}",
            Local::now().get_datetime_as_filename_safe_string(),
            filename_ext
        )
    });

    if !filename.ends_with(&format!(".{}", filename_ext)) {
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
    fn generate_qrcode_without_output_returns_ok() {
        let cfg = text_config(None, None);

        assert!(generate_qrcode(&cfg).is_ok());
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

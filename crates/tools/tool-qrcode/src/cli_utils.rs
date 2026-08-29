use crate::models::{HowMode, QrCodeConfig, QrCodePayload};
use anyhow::Result;
use clap::Parser;
use common_cli::common_tool_args::CommonToolArgs;
use common_utils::constants::{CONFIG_UL_ITEM_LEVEL_2, CONFIG_UL_ITEM_LEVEL_3};

/// Generate QR codes for text or Wi-Fi payloads.
///
/// This tool can generate QR codes for text, URLs, Wi-Fi payloads, or other types of data.
/// The output can be printed to the console and/or saved to a file.
#[derive(Parser, Debug)]
#[command(about, long_about, version)]
struct CliArgs {
    /// Text payload for the QR code
    #[arg(short = 't', long = "text", value_name = "TEXT")]
    pub text: Option<String>,

    /// SSID for a Wi-Fi payload
    #[arg(short = 's', long = "wifi-ssid", value_name = "SSID")]
    pub wifi_ssid: Option<String>,

    /// Password for a Wi-Fi payload
    #[arg(short = 'p', long = "wifi-password", value_name = "PASSWORD")]
    pub wifi_password: Option<String>,

    /// Authentication type for a wifi payload (default: WPA)
    #[arg(short = 'a', long = "wifi-auth", value_name = "AUTH")]
    pub wifi_auth: Option<String>,

    /// Skip printing the QR code to the console
    #[arg(short = 'x', long = "dont-print")]
    pub dont_print: bool,

    /// Format of the output file, regardless of the filename extension
    #[arg(short = 'f', long = "output-format", value_name = "FORMAT")]
    pub output_format: Option<String>,

    /// Output file name. If not specified, a random one is generated
    #[arg(short = 'o', long = "output-file", value_name = "FILENAME")]
    pub output_file: Option<String>,

    #[command(flatten)]
    pub common: CommonToolArgs,
}

/// Parses command-line arguments and returns the runtime configuration.
///
/// # Errors
/// Returns an error when neither a text payload nor a complete wifi payload
/// (both an SSID and a password) is provided.
pub fn initialize() -> Result<QrCodeConfig> {
    let args = CliArgs::parse();

    let config = build_config(&args)?;

    args.common.app_boot_up(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        false,
        false,
        Some(|| {
            print_header(&config);
        }),
    );

    Ok(config)
}

/// Validates the parsed arguments and resolves them into a [`QrCodeConfig`].
fn build_config(args: &CliArgs) -> Result<QrCodeConfig> {
    if args.text.is_none() {
        match (args.wifi_ssid.is_some(), args.wifi_password.is_some()) {
            (true, true) => {}
            (true, false) => {
                anyhow::bail!("Wifi payload is missing a password (--wifi-password).")
            }
            (false, true) => anyhow::bail!("Wifi payload is missing an SSID (--wifi-ssid)."),
            (false, false) => anyhow::bail!(
                "Either a text payload (--text) or a wifi payload (--wifi-ssid with --wifi-password) must be provided."
            ),
        }
    }

    let wifi_auth = args.wifi_auth.clone().unwrap_or_else(|| "WPA".to_string());

    Ok(QrCodeConfig::new(
        QrCodePayload::new(
            args.text.clone(),
            args.wifi_ssid.clone(),
            args.wifi_password.clone(),
            Some(wifi_auth),
        ),
        args.dont_print,
        args.output_format.clone(),
        args.output_file.clone(),
    ))
}

/// Prints the tool's runtime configuration, shown under `--app-header`.
fn print_header(config: &QrCodeConfig) {
    match config.get_payload() {
        HowMode::TextPayload(text_payload) => {
            println!("{} Payload: text", CONFIG_UL_ITEM_LEVEL_2);
            println!("{} Text: {}", CONFIG_UL_ITEM_LEVEL_3, text_payload);
        }
        HowMode::WifiPayload(wifi_ssid, wifi_pass, wifi_auth) => {
            println!("{} Payload: wifi", CONFIG_UL_ITEM_LEVEL_2);
            println!("{} SSID: {}", CONFIG_UL_ITEM_LEVEL_3, wifi_ssid);
            println!("{} Password: {}", CONFIG_UL_ITEM_LEVEL_3, wifi_pass);
            println!("{} Auth: {}", CONFIG_UL_ITEM_LEVEL_3, wifi_auth);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_has_no_conflicting_flags() {
        CliArgs::command().debug_assert();
    }

    #[test]
    fn text_payload_builds_config() {
        let args = CliArgs::try_parse_from(["qrcode", "--text", "hello"]).unwrap();
        let config = build_config(&args).unwrap();
        assert!(matches!(
            config.get_payload(),
            HowMode::TextPayload(text) if text == "hello"
        ));
    }

    #[test]
    fn wifi_payload_defaults_auth_to_wpa() {
        let args =
            CliArgs::try_parse_from(["qrcode", "--wifi-ssid", "net", "--wifi-password", "secret"])
                .unwrap();
        let config = build_config(&args).unwrap();
        assert!(matches!(
            config.get_payload(),
            HowMode::WifiPayload(ssid, password, auth)
                if ssid == "net" && password == "secret" && auth == "WPA"
        ));
    }

    #[test]
    fn no_payload_is_rejected() {
        let args = CliArgs::try_parse_from(["qrcode"]).unwrap();
        assert!(build_config(&args).is_err());
    }

    #[test]
    fn wifi_without_password_is_rejected() {
        let args = CliArgs::try_parse_from(["qrcode", "--wifi-ssid", "net"]).unwrap();
        assert!(build_config(&args).is_err());
    }

    #[test]
    fn wifi_without_ssid_is_rejected() {
        let args = CliArgs::try_parse_from(["qrcode", "--wifi-password", "secret"]).unwrap();
        assert!(build_config(&args).is_err());
    }
}

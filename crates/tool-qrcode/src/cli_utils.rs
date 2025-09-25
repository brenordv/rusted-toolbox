use crate::models::{HowMode, QrCodeConfig, QrCodePayload};
use anyhow::Result;
use clap::{Arg, Command};
use shared::command_line::cli_builder::CommandExt;
use shared::constants::general::DASH_LINE;
use shared::system::tool_exit_helpers::exit_error;
use tracing::error;

pub fn print_runtime_info(args: &QrCodeConfig) {
    println!("QrCode Generator v{}", env!("CARGO_PKG_VERSION"));
    println!("{}", DASH_LINE);

    match args.get_payload() {
        HowMode::TextPayload(text_payload) => {
            println!("Generating Text QR code");
            println!("Text: {}", text_payload);
        }
        HowMode::WifiPayload(wifi_ssid, wifi_pass, wifi_auth) => {
            println!("Generating Wifi QR code");
            println!("SSID: {}", wifi_ssid);
            println!("Password: {}", wifi_pass);
            println!("Auth: {}", wifi_auth);
        }
    }

    println!();
}

pub fn get_cli_arguments() -> Result<QrCodeConfig> {
    let matches = Command::new(env!("CARGO_PKG_NAME"))
        .add_basic_metadata(
            env!("CARGO_PKG_VERSION"),
            env!("CARGO_PKG_DESCRIPTION"),
            "This tool can generate QR codes for text, URLs, wifi payloads, or other types of data. The output can be printed to the console and/or saved to a file."
        )
        .arg(Arg::new("text")
            .long("text")
            .short('t')
            .value_name("text")
            .help("Text payload for QR code."))
        .arg(Arg::new("wifi-ssid")
            .long("wifi-ssid")
            .short('s')
            .help("SSID for wifi payload."))
    .arg(Arg::new("wifi-password")
            .long("wifi-password")
            .short('p')
            .value_name("password")
            .help("Password for wifi payload."))
    .arg(Arg::new("wifi-auth")
            .long("wifi-auth")
            .short('a')
            .help("Authentication type for wifi payload. (Default: WPA)"))
    .arg(Arg::new("no-header")
            .long("no-header")
            .short('n')
            .action(clap::ArgAction::SetTrue)
            .help("Do not print header."))
    .arg(Arg::new("dont-print")
            .long("dont-print")
            .short('x')
            .action(clap::ArgAction::SetTrue)
            .help("Skips printing QR code to console."))
        .arg(Arg::new("output-format")
            .long("output-format")
            .short('f')
            .help("Format of output file. This defines the actual format of the file, regardless of the filename."))
        .arg(Arg::new("output-file")
            .long("output-file")
            .short('o')
            .value_name("filename")
            .help("Output file name. If not specified, will generate random."))
        .get_matches();

    let text_payload = matches.get_one::<String>("text");
    let wifi_ssid = matches.get_one::<String>("wifi-ssid");
    let wifi_password = matches.get_one::<String>("wifi-password");
    let wifi_auth = match matches.get_one::<String>("wifi-auth") {
        Some(auth) => Some(auth.to_string()),
        None => Some("WPA".to_string()),
    };
    let no_header = matches.get_flag("no-header");
    let dont_print = matches.get_flag("dont-print");
    let output_format = matches.get_one::<String>("output-format");
    let output_file = matches.get_one::<String>("output-file");

    let is_text_payload_set = text_payload.is_some();
    let is_wifi_payload_set = wifi_ssid.is_some() && wifi_password.is_some();

    if !is_text_payload_set && !is_wifi_payload_set {
        error!("Error: Either text or wifi payload must be provided.");
        exit_error();
        unreachable!();
    } else if !is_text_payload_set {
        if wifi_ssid.is_none() && wifi_password.is_some() {
            error!("Error: Wifi payload doesn't have an SSID.");
            exit_error();
            unreachable!();
        } else if wifi_ssid.is_some() && wifi_password.is_none() {
            error!("Error: Wifi payload doesn't have a password.");
            exit_error();
            unreachable!();
        }
    }

    Ok(QrCodeConfig::new(
        QrCodePayload::new(
            text_payload.map(|text| text.to_string()),
            wifi_ssid.map(|ssid| ssid.to_string()),
            wifi_password.map(|password| password.to_string()),
            wifi_auth.map(|auth| auth.to_string()),
        ),
        no_header,
        dont_print,
        output_format.map(|format| format.to_string()),
        output_file.map(|file| file.to_string()),
    ))
}
use crate::models::{NetQualityCliArgs, Thresholds};
use anyhow::{anyhow, Result};
use clap::{Arg, ArgAction, Command};
use shared::command_line::cli_builder::CommandExt;
use shared::constants::general::DASH_LINE;
use std::path::PathBuf;

pub fn get_cli_arguments() -> Result<NetQualityCliArgs> {
    let matches = Command::new(env!("CARGO_PKG_NAME"))
        .add_basic_metadata(
            env!("CARGO_PKG_VERSION"),
            env!("CARGO_PKG_DESCRIPTION"),
            "Monitor connectivity and speed with notifications, saving the results to a SQLite database, allowing you to analyze them later. Notification to Telegram and/or Open Telemetry is also available.",
        )
        .preset_arg_config(None)
        .preset_arg_verbose(None)
        .arg(
            Arg::new("url")
                .long("url")
                .action(ArgAction::Append)
                .value_name("URL")
                .help("Connectivity check URL (repeatable)"),
        )
        .arg(
            Arg::new("replace-urls")
                .long("replace-urls")
                .action(ArgAction::SetTrue)
                .help("Replace default URL list instead of merging"),
        )
        .arg(
            Arg::new("expected-download")
                .long("expected-download")
                .value_name("MBPS")
                .help("Expected download speed in Mbps (required if not in config)")
                .value_parser(clap::value_parser!(f64)),
        )
        .arg(
            Arg::new("expected-upload")
                .long("expected-upload")
                .value_name("MBPS")
                .help("Expected upload speed in Mbps (optional)")
                .value_parser(clap::value_parser!(f64)),
        )
        .arg(
            Arg::new("download-thresholds")
                .long("download-thresholds")
                .value_name("V,S,M,MF")
                .help("Download thresholds as comma-separated percentages (e.g. 30,50,65,85)"),
        )
        .arg(
            Arg::new("upload-thresholds")
                .long("upload-thresholds")
                .value_name("V,S,M,MF")
                .help("Upload thresholds as comma-separated percentages (e.g. 30,50,65,85)"),
        )
        .arg(
            Arg::new("connectivity-delay")
                .long("connectivity-delay")
                .value_name("SECS")
                .help("Connectivity check delay in seconds")
                .value_parser(clap::value_parser!(u64)),
        )
        .arg(
            Arg::new("speed-delay")
                .long("speed-delay")
                .value_name("SECS")
                .help("Speed check delay in seconds")
                .value_parser(clap::value_parser!(u64)),
        )
        .arg(
            Arg::new("connectivity-timeout")
                .long("connectivity-timeout")
                .value_name("SECS")
                .help("Connectivity request timeout in seconds")
                .value_parser(clap::value_parser!(u64)),
        )
        .arg(
            Arg::new("outage-backoff")
                .long("outage-backoff")
                .value_name("SECS")
                .help("Outage backoff delay in seconds")
                .value_parser(clap::value_parser!(u64)),
        )
        .arg(
            Arg::new("outage-backoff-max")
                .long("outage-backoff-max")
                .value_name("SECS")
                .help("Maximum outage backoff delay in seconds")
                .value_parser(clap::value_parser!(u64)),
        )
        .arg(
            Arg::new("db-path")
                .long("db-path")
                .value_name("FILE")
                .help("SQLite database path")
                .value_parser(clap::value_parser!(PathBuf)),
        )
        .arg(
            Arg::new("telegram-token")
                .long("telegram-token")
                .value_name("TOKEN")
                .help("Telegram bot token"),
        )
        .arg(
            Arg::new("telegram-chat-id")
                .long("telegram-chat-id")
                .value_name("CHAT")
                .help("Telegram chat ID"),
        )
        .arg(
            Arg::new("otel-endpoint")
                .long("otel-endpoint")
                .value_name("URL")
                .help("OpenTelemetry OTLP endpoint"),
        )
        .get_matches();

    let urls: Vec<String> = matches
        .get_many::<String>("url")
        .map(|values| values.cloned().collect())
        .unwrap_or_default();

    let download_thresholds = matches
        .get_one::<String>("download-thresholds")
        .map(|value| parse_thresholds(value))
        .transpose()?;

    let upload_thresholds = matches
        .get_one::<String>("upload-thresholds")
        .map(|value| parse_thresholds(value))
        .transpose()?;

    let telegram_token = matches.get_one::<String>("telegram-token").cloned();
    let telegram_chat_id = matches.get_one::<String>("telegram-chat-id").cloned();

    if telegram_token.is_some() ^ telegram_chat_id.is_some() {
        return Err(anyhow!(
            "Both --telegram-token and --telegram-chat-id must be provided together."
        ));
    }

    Ok(NetQualityCliArgs {
        config_path: matches.get_one::<PathBuf>("config").cloned(),
        urls,
        replace_urls: matches.get_flag("replace-urls"),
        expected_download_mbps: matches.get_one::<f64>("expected-download").copied(),
        expected_upload_mbps: matches.get_one::<f64>("expected-upload").copied(),
        download_thresholds,
        upload_thresholds,
        connectivity_delay_secs: matches.get_one::<u64>("connectivity-delay").copied(),
        speed_delay_secs: matches.get_one::<u64>("speed-delay").copied(),
        connectivity_timeout_secs: matches.get_one::<u64>("connectivity-timeout").copied(),
        outage_backoff_secs: matches.get_one::<u64>("outage-backoff").copied(),
        outage_backoff_max_secs: matches.get_one::<u64>("outage-backoff-max").copied(),
        db_path: matches.get_one::<PathBuf>("db-path").cloned(),
        telegram_token,
        telegram_chat_id,
        otel_endpoint: matches.get_one::<String>("otel-endpoint").cloned(),
        verbose: matches.get_flag("verbose"),
    })
}

pub fn print_runtime_info(config_label: &str, runtime_info: &[(&str, String)]) {
    println!("NetQuality v{}", env!("CARGO_PKG_VERSION"));
    println!("{}", DASH_LINE);
    println!("- Config source: {}", config_label);
    for (label, value) in runtime_info {
        println!("- {}: {}", label, value);
    }
    println!("{}", DASH_LINE);
    println!();
}

fn parse_thresholds(value: &str) -> Result<Thresholds> {
    let parts: Vec<&str> = value.split(',').map(|part| part.trim()).collect();
    if parts.len() != 4 {
        return Err(anyhow!(
            "Thresholds must have 4 comma-separated values (e.g. 30,50,65,85)."
        ));
    }

    let values: Result<Vec<f64>, _> = parts
        .iter()
        .map(|part| {
            part.parse::<f64>()
                .map_err(|_| anyhow!("Invalid threshold value: {part}"))
        })
        .collect();

    let values = values?;
    let thresholds = Thresholds {
        very_slow: values[0],
        slow: values[1],
        medium: values[2],
        medium_fast: values[3],
    };

    thresholds
        .validate()
        .map_err(|error| anyhow!("Invalid threshold values: {error}"))?;

    Ok(thresholds)
}

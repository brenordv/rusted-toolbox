use clap::{Arg, Command};
use anyhow::Result;
use shared::command_line::cli_builder::CommandExt;
use crate::models::shared_types::RuntimeType;
use crate::models::whisper_args::WhisperArgs;

const DEFAULT_PORT: u16 = 2428; // The word chat in the old T9

pub fn get_cli_arguments() -> Result<WhisperArgs> {
    let matches = Command::new(env!("CARGO_PKG_NAME"))
        .add_basic_metadata(
            env!("CARGO_PKG_VERSION"),
            env!("CARGO_PKG_DESCRIPTION"),
            "", // TODO: Add a long description.
        )
        .arg(Arg::new("wait")
            .long("wait")
            .short('w')
            .value_name("PORT")
            .value_parser(clap::value_parser!(u16))
            .required(false)
            .help(&format!("Host mode: Listen for connections on the specified port (default: {})", DEFAULT_PORT)))
        .arg(Arg::new("connect")
            .long("connect")
            .short('c')
            .value_name("HOST:PORT")
            .help("Client mode: Connect to the specified host and port (format: host:port)"))
        .arg(Arg::new("bind-to-all-interfaces")
            .long("bind-to-all-interfaces")
            .short('b')
            .action(clap::ArgAction::SetTrue)
            .help("Bind to all interfaces (default: bind to localhost)"))
        .get_matches();

    let ip = if *matches.get_one::<bool>("bind-to-all-interfaces").unwrap_or(&false) {
        "0.0.0.0"
    } else {
        "127.0.0.1"
    };

    if matches.contains_id("wait") {
        let port = matches.get_one::<u16>("wait").copied().unwrap_or(DEFAULT_PORT);
        return Ok(WhisperArgs {
            host: format!("{}:{}", ip, port),
            runtime: RuntimeType::Host,
            role: "HOST".to_string(),
        });
    }

    let connect_address = matches.get_one::<String>("connect").cloned();

    if connect_address.is_none() {
        anyhow::bail!("You must specify either --wait or --connect");
    }

    Ok(WhisperArgs {
        host: connect_address.unwrap(),
        runtime: RuntimeType::Client,
        role: "CLIENT".to_string(),
    })
}
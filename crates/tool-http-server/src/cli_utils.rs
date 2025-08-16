use crate::models::ServerArgs;
use clap::{Arg, Command};
use shared::command_line::cli_builder::CommandExt;
use shared::constants::general::DASH_LINE;
use std::net::IpAddr;
use std::path::PathBuf;

pub fn print_runtime_info(args: &ServerArgs) {
    println!("🚀 Simple HTTP Server v{}", env!("CARGO_PKG_VERSION"));
    println!("{}", DASH_LINE);
    println!("📂 Root directory: {}", args.root_path.display());
    println!("🚪 Port: {}", args.port);
}

pub fn get_cli_arguments() -> ServerArgs {
    let matches = Command::new(env!("CARGO_PKG_NAME"))
        .add_basic_metadata(
            env!("CARGO_PKG_VERSION"),
            env!("CARGO_PKG_DESCRIPTION"),
            "Simple HTTP server for local files.",
        )
        .preset_arg_verbose(None)
        .arg(
            Arg::new("path")
                .help("Path to serve as web root (defaults to current directory)")
                .index(1)
                .required(false),
        )
        .arg(
            Arg::new("port")
                .short('p')
                .long("port")
                .value_name("PORT")
                .help("Port number to listen on (default: 4200)")
                .value_parser(clap::value_parser!(u16)),
        )
        .arg(
            Arg::new("host")
                .short('o')
                .long("host")
                .value_name("HOST")
                .help("Host that will be used to bind the server (default: 127.0.0.1)")
                .required(false)
                .default_value("127.0.0.1"),
        )
        .get_matches();

    let root_path = matches
        .get_one::<String>("path")
        .map(|p| PathBuf::from(p))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let port = matches.get_one::<u16>("port").copied().unwrap_or(4200);

    let host: IpAddr = matches
        .get_one::<String>("host")
        .unwrap()
        .parse()
        .unwrap_or_else(|_| "127.0.0.1".parse().unwrap());

    let config = ServerArgs {
        root_path,
        port,
        host,
    };

    // Validate root path exists
    if !config.root_path.exists() {
        eprintln!(
            "Error: Path '{}' does not exist",
            config.root_path.display()
        );
        std::process::exit(1);
    }

    if !config.root_path.is_dir() {
        eprintln!(
            "Error: Path '{}' is not a directory",
            config.root_path.display()
        );
        std::process::exit(1);
    }

    config
}

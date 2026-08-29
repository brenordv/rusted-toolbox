use crate::models::ServerConfig;
use anyhow::{Context, Result};
use clap::Parser;
use common_cli::common_tool_args::CommonToolArgs;
use common_utils::constants::CONFIG_UL_ITEM_LEVEL_2;
use std::net::IpAddr;
use std::path::PathBuf;

/// Simple HTTP server for local files.
///
/// Lightweight async HTTP server for quickly serving static files with directory browsing, MIME
/// detection, logging, and secure development-focused features.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about)]
pub struct CliArgs {
    /// Path to serve as web root (defaults to current directory).
    #[arg(
        short = 'a',
        long = "path",
        num_args = 1,
        required = false,
        default_value = "."
    )]
    pub path: PathBuf,

    /// Port number to listen on.
    #[arg(short = 'p', long = "port", default_value_t = 4200)]
    pub port: u16,

    /// Host that will be used to bind the server.
    #[arg(
        short = 'h',
        long = "host",
        default_value = "127.0.0.1",
        required = false
    )]
    pub host: String,

    /// Serve hidden files and directories (names starting with '.').
    #[arg(short = 's', long = "serve-hidden", required = false)]
    pub serve_hidden: bool,

    #[command(flatten)]
    pub common: CommonToolArgs,
}

fn print_runtime_info(args: &ServerConfig) {
    println!(
        "{} Root directory: {}",
        CONFIG_UL_ITEM_LEVEL_2,
        args.root_path.display()
    );
    println!("{} Port: {}", CONFIG_UL_ITEM_LEVEL_2, args.port);
    println!(
        "{} Serve hidden files: {}",
        CONFIG_UL_ITEM_LEVEL_2, args.serve_hidden
    );
}

pub fn initialize() -> Result<ServerConfig> {
    let args = CliArgs::parse();

    let host: IpAddr = args
        .host
        .parse()
        .with_context(|| format!("Invalid host IP address: {:?}", args.host))?;

    let config = ServerConfig {
        root_path: args.path,
        port: args.port,
        host,
        serve_hidden: args.serve_hidden,
    };

    args.common.app_boot_up(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        false,
        false,
        Some(|| {
            print_runtime_info(&config);
        }),
    );

    if !config.root_path.exists() {
        anyhow::bail!(
            "Error: Path '{}' does not exist",
            config.root_path.display()
        )
    }

    if !config.root_path.is_dir() {
        anyhow::bail!(
            "Error: Path '{}' is not a directory",
            config.root_path.display()
        )
    }

    if config.port == 0 {
        anyhow::bail!("Error: Invalid port number: {}", config.port)
    }

    Ok(config)
}

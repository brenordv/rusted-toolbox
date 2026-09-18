use crate::models::ServerConfig;
use anyhow::{Context, Result};
use clap::Parser;
use common_cli::broken_pipe::{BrokenPipe, write_out};
use common_cli::common_tool_args::CommonToolArgs;
use common_cli::header_format::format_config_item;
use common_cli::tool_exit_helpers::exit_error;
use std::net::IpAddr;
use std::path::PathBuf;
use tracing::{debug, warn};

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

    /// Host that will be used to bind the server. Long-only so `-h` stays
    /// bound to clap's help.
    #[arg(
        short = 'o',
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

/// Prints the tool-specific header lines. The lines are informational, so a
/// failing stdout never fails the boot: a closed pipe stops with a debug note,
/// any other write error with one warning.
fn print_runtime_info(args: &ServerConfig) {
    let lines = [
        format_config_item("Root directory", args.root_path.display()),
        format_config_item("Port", args.port),
        format_config_item("Serve hidden files", args.serve_hidden),
    ];

    let mut stdout = std::io::stdout();
    for line in lines {
        if let Err(error) = write_out(&mut stdout, format!("{line}\n").as_bytes()) {
            if error.is::<BrokenPipe>() {
                debug!("Runtime-config lines skipped: stdout closed by the consumer");
            } else {
                warn!("Runtime-config lines not printed: {error:#}");
            }
            return;
        }
    }
}

/// Parses the CLI arguments, validates them, and boots logging. Validation
/// failures happen before the logging subscriber is installed, so they are
/// reported on stderr and the process exits with an error code.
pub fn initialize() -> ServerConfig {
    let args = CliArgs::parse();

    let config = match build_and_validate(&args) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("{e:#}");
            exit_error();
        }
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

    config
}

/// Builds the runtime configuration from parsed CLI arguments, validating the
/// host address, the web-root path, and the port.
fn build_and_validate(args: &CliArgs) -> Result<ServerConfig> {
    let host: IpAddr = args
        .host
        .parse()
        .with_context(|| format!("Invalid host IP address: {:?}", args.host))?;

    let config = ServerConfig {
        root_path: args.path.clone(),
        port: args.port,
        host,
        serve_hidden: args.serve_hidden,
    };

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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn cli_definition_has_no_conflicting_flags() {
        CliArgs::command().debug_assert();
    }

    #[test]
    fn short_h_prints_help_instead_of_binding_host() {
        let error = CliArgs::try_parse_from(["https", "-h"]).unwrap_err();
        assert_eq!(error.kind(), clap::error::ErrorKind::DisplayHelp);
    }

    #[test]
    fn build_and_validate_rejects_invalid_host_ip() {
        let args = CliArgs::parse_from(["https", "--host", "not-an-ip"]);
        assert!(build_and_validate(&args).is_err());
    }

    #[test]
    fn build_and_validate_rejects_port_zero() {
        let dir = tempdir().unwrap();
        let args = CliArgs::parse_from(["https", "-a", dir.path().to_str().unwrap(), "-p", "0"]);
        assert!(build_and_validate(&args).is_err());
    }

    #[test]
    fn build_and_validate_rejects_nonexistent_path() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("nope");
        let args = CliArgs::parse_from(["https", "-a", missing.to_str().unwrap()]);
        assert!(build_and_validate(&args).is_err());
    }

    #[test]
    fn build_and_validate_rejects_non_directory_path() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("file.txt");
        fs::write(&file, "x").unwrap();
        let args = CliArgs::parse_from(["https", "-a", file.to_str().unwrap()]);
        assert!(build_and_validate(&args).is_err());
    }

    #[test]
    fn build_and_validate_accepts_valid_arguments() {
        let dir = tempdir().unwrap();
        let args = CliArgs::parse_from([
            "https",
            "-a",
            dir.path().to_str().unwrap(),
            "--host",
            "0.0.0.0",
            "-p",
            "8080",
            "-s",
        ]);

        let config = build_and_validate(&args).unwrap();

        assert_eq!(config.port, 8080);
        assert!(config.host.is_unspecified());
        assert!(config.serve_hidden);
        assert_eq!(config.root_path, dir.path());
    }
}

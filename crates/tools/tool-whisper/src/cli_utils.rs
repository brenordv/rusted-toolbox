use crate::models::shared_types::RuntimeType;
use crate::models::whisper_args::WhisperArgs;
use anyhow::Result;
use clap::Parser;
use common_cli::common_tool_args::CommonToolArgs;

/// Bare-bones, secure, and private P2P chat.
///
/// Start in host mode with --wait to listen for a connection, or in client mode with --connect to
/// reach a host. Messages are end-to-end encrypted.
#[derive(Parser, Debug)]
#[command(about, long_about, version)]
struct CliArgs {
    /// Host mode: listen for connections on the given port (default: 2428)
    #[arg(
        short = 'w',
        long = "wait",
        value_name = "PORT",
        num_args = 0..=1,
        default_missing_value = "2428"
    )]
    pub wait: Option<u16>,

    /// Client mode: connect to the given host and port (format: host:port)
    #[arg(short = 'c', long = "connect", value_name = "HOST:PORT")]
    pub connect: Option<String>,

    /// Bind to all interfaces (default: bind to localhost)
    #[arg(short = 'b', long = "bind-to-all-interfaces")]
    pub bind_to_all_interfaces: bool,

    #[command(flatten)]
    pub common: CommonToolArgs,
}

/// Parses command-line arguments and returns the runtime configuration.
///
/// # Errors
/// Returns an error when neither `--wait` (host mode) nor `--connect` (client mode) is provided.
pub fn initialize() -> Result<WhisperArgs> {
    let args = CliArgs::parse();

    let config = build_args(&args)?;

    args.common.app_boot_up(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        false,
        false,
        None::<fn()>,
    );

    Ok(config)
}

/// Resolves the parsed CLI arguments into the runtime configuration.
///
/// `--wait` takes precedence over `--connect` when both are supplied.
fn build_args(args: &CliArgs) -> Result<WhisperArgs> {
    let ip = if args.bind_to_all_interfaces {
        "0.0.0.0"
    } else {
        "127.0.0.1"
    };

    if let Some(port) = args.wait {
        return Ok(WhisperArgs {
            host: format!("{}:{}", ip, port),
            runtime: RuntimeType::Host,
            role: "HOST".to_string(),
        });
    }

    match &args.connect {
        Some(connect_address) => Ok(WhisperArgs {
            host: connect_address.clone(),
            runtime: RuntimeType::Client,
            role: "CLIENT".to_string(),
        }),
        None => anyhow::bail!("You must specify either --wait or --connect"),
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
    fn wait_selects_host_mode_on_localhost() {
        let args = CliArgs::try_parse_from(["whisper", "--wait", "3000"]).unwrap();
        let config = build_args(&args).unwrap();
        assert!(matches!(config.runtime, RuntimeType::Host));
        assert_eq!(config.host, "127.0.0.1:3000");
    }

    #[test]
    fn wait_without_value_uses_default_port() {
        let args = CliArgs::try_parse_from(["whisper", "--wait"]).unwrap();
        let config = build_args(&args).unwrap();
        assert_eq!(config.host, "127.0.0.1:2428");
    }

    #[test]
    fn bind_to_all_interfaces_uses_wildcard_host() {
        let args = CliArgs::try_parse_from(["whisper", "--wait", "3000", "-b"]).unwrap();
        let config = build_args(&args).unwrap();
        assert_eq!(config.host, "0.0.0.0:3000");
    }

    #[test]
    fn connect_selects_client_mode() {
        let args = CliArgs::try_parse_from(["whisper", "--connect", "example.com:3000"]).unwrap();
        let config = build_args(&args).unwrap();
        assert!(matches!(config.runtime, RuntimeType::Client));
        assert_eq!(config.host, "example.com:3000");
    }

    #[test]
    fn neither_wait_nor_connect_is_rejected() {
        let args = CliArgs::try_parse_from(["whisper"]).unwrap();
        assert!(build_args(&args).is_err());
    }
}

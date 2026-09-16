use crate::models::{MqttCommand, MqttConfig, MqttCredentials};
use anyhow::Result;
use clap::{Args, Parser, Subcommand};

use common_cli::common_tool_args::CommonToolArgs;
use common_cli::header_format::{format_config_item, format_config_item_level3};

/// Cli tool to perform quickly post to or read from a MQTT broker.
///
/// Tool that allows posting a message to a MQTT broker or reading from it.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about)]
pub struct CliArgs {
    #[command(subcommand)]
    command: Commands,

    #[command(flatten)]
    pub common: CommonToolArgs,
}

#[derive(Args, Debug)]
struct ReadArgs {
    #[command(flatten)]
    pub common: CommonArgs,
}

#[derive(Args, Debug)]
struct PostArgs {
    /// Message to post to the MQTT broker.
    #[arg(short = 'm', long = "message", required = true)]
    pub message: String,

    #[command(flatten)]
    pub common: CommonArgs,
}

#[derive(Args, Debug)]
struct CommonArgs {
    /// Hostname or IP address of the MQTT broker.
    #[arg(short = 'o', long = "host", required = true)]
    pub host: String,

    /// Port used when connecting to the MQTT broker.
    #[arg(short = 'p', long = "port", default_value_t = 1883)]
    pub port: u16,

    /// Topic to read from or post to.
    #[arg(short = 't', long = "topic", required = true)]
    pub topic: String,

    /// Username used when connecting to the MQTT broker.
    #[arg(short = 'u', long = "username", required = false)]
    pub username: Option<String>,

    /// Password used when connecting to the MQTT broker.
    #[arg(short = 'a', long = "password", required = false)]
    pub password: Option<String>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Read messages from a MQTT broker.
    #[command(
        visible_alias = "reads",
        after_help = "Examples:\n  \
            mqtt read --host broker.hivemq.com --topic sensors/temperature\n  \
            mqtt reads --host my-broker.com --port 8883 --topic private/data --username myuser --password mypass"
    )]
    Read(ReadArgs),

    /// Post a message to a MQTT broker.
    #[command(
        visible_alias = "send",
        after_help = "Examples:\n  \
            mqtt post --host broker.hivemq.com --topic sensors/temperature --message \"22.5\"\n  \
            mqtt send --host localhost --port 1884 --topic test/message --message \"Hello MQTT!\" --username myuser --password mypass"
    )]
    Post(PostArgs),
}

fn print_runtime_info(args: &MqttConfig) {
    println!(
        "{}",
        format_config_item("Host", format!("{}:{}", args.host, args.port))
    );

    let connection_type = if args.is_anonymous() {
        "Anonymous"
    } else {
        "Authenticated"
    };

    println!("{}", format_config_item("Connection type", connection_type));
    println!("{}", format_config_item("Topic", &args.topic));

    match args.command {
        MqttCommand::Read => {
            println!("{}", format_config_item("Command", "Read"));
        }
        MqttCommand::Post => {
            println!("{}", format_config_item("Command", "Post"));
            if let Some(msg) = &args.message {
                println!("{}", format_config_item_level3("Message", msg));
            }
        }
    }
}

pub fn initialize() -> Result<MqttConfig> {
    let args = CliArgs::parse();

    let (command, connection, message) = match args.command {
        Commands::Read(read_args) => (MqttCommand::Read, read_args.common, None),
        Commands::Post(post_args) => (MqttCommand::Post, post_args.common, Some(post_args.message)),
    };

    // A one-sided pair resolves to no credentials here and is rejected right
    // after logging boots, so the error is reported through the subscriber.
    let credential_spec = resolve_credentials(connection.username, connection.password);
    let partial_credentials = matches!(credential_spec, CredentialSpec::Partial);

    let command_config = MqttConfig {
        command,
        host: connection.host,
        port: connection.port,
        topic: connection.topic,
        message,
        credentials: match credential_spec {
            CredentialSpec::Complete(credentials) => Some(credentials),
            CredentialSpec::Anonymous | CredentialSpec::Partial => None,
        },
    };

    args.common.app_boot_up(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        false,
        false,
        Some(|| print_runtime_info(&command_config)),
    );

    if partial_credentials {
        anyhow::bail!("Username and password are required together.");
    }

    validate_host_and_port(&command_config.host, command_config.port)?;
    validate_topic(&command_config.topic)?;

    match &command_config.command {
        MqttCommand::Read => {}
        MqttCommand::Post => {
            validate_message(&command_config.message)?;
        }
    }

    Ok(command_config)
}

/// Outcome of pairing the optional `--username`/`--password` flags: a complete
/// pair, no credentials at all, or the one-sided combination the caller must
/// reject.
enum CredentialSpec {
    Anonymous,
    Complete(MqttCredentials),
    Partial,
}

fn resolve_credentials(username: Option<String>, password: Option<String>) -> CredentialSpec {
    match (username, password) {
        (Some(username), Some(password)) => {
            CredentialSpec::Complete(MqttCredentials { username, password })
        }
        (None, None) => CredentialSpec::Anonymous,
        _ => CredentialSpec::Partial,
    }
}

fn validate_message(message: &Option<String>) -> Result<()> {
    match message {
        None => {
            anyhow::bail!("Message is required for post command.");
        }
        Some(m) => {
            if m.is_empty() {
                anyhow::bail!("Message cannot be empty.");
            }
            Ok(())
        }
    }
}

fn validate_host_and_port(host: &str, port: u16) -> Result<()> {
    if host.is_empty() {
        anyhow::bail!("Host is required.");
    }

    if port == 0 {
        anyhow::bail!("Port is required.");
    }

    Ok(())
}

fn validate_topic(topic: &str) -> Result<()> {
    if topic.is_empty() {
        anyhow::bail!("Topic is required.");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_consistent() {
        CliArgs::command().debug_assert();
    }

    #[test]
    fn subcommand_aliases_parse_to_the_right_variants() {
        let read_args = CliArgs::try_parse_from([
            "mqtt",
            "reads",
            "--host",
            "localhost",
            "--topic",
            "sensors/temp",
        ])
        .expect("the reads alias should parse");
        assert!(matches!(read_args.command, Commands::Read(_)));

        let post_args = CliArgs::try_parse_from([
            "mqtt",
            "send",
            "--host",
            "localhost",
            "--topic",
            "sensors/temp",
            "--message",
            "22.5",
        ])
        .expect("the send alias should parse");
        assert!(matches!(post_args.command, Commands::Post(_)));
    }

    #[test]
    fn validate_message_rejects_missing_message() {
        assert!(validate_message(&None).is_err());
    }

    #[test]
    fn validate_message_rejects_empty_message() {
        assert!(validate_message(&Some(String::new())).is_err());
    }

    #[test]
    fn validate_message_accepts_non_empty_message() {
        assert!(validate_message(&Some("22.5".to_string())).is_ok());
    }

    #[test]
    fn validate_host_and_port_rejects_empty_host() {
        assert!(validate_host_and_port("", 1883).is_err());
    }

    #[test]
    fn validate_host_and_port_rejects_port_zero() {
        assert!(validate_host_and_port("localhost", 0).is_err());
    }

    #[test]
    fn validate_host_and_port_accepts_host_with_port() {
        assert!(validate_host_and_port("localhost", 1883).is_ok());
    }

    #[test]
    fn validate_topic_rejects_empty_topic() {
        assert!(validate_topic("").is_err());
    }

    #[test]
    fn validate_topic_accepts_non_empty_topic() {
        assert!(validate_topic("sensors/temp").is_ok());
    }

    #[test]
    fn resolve_credentials_neither_is_anonymous() {
        assert!(matches!(
            resolve_credentials(None, None),
            CredentialSpec::Anonymous
        ));
    }

    #[test]
    fn resolve_credentials_both_builds_the_pair() {
        let spec = resolve_credentials(Some("user".to_string()), Some("pass".to_string()));

        match spec {
            CredentialSpec::Complete(credentials) => {
                assert_eq!(credentials.username, "user");
                assert_eq!(credentials.password, "pass");
            }
            _ => panic!("expected a complete credential pair"),
        }
    }

    #[test]
    fn resolve_credentials_username_only_is_partial() {
        assert!(matches!(
            resolve_credentials(Some("user".to_string()), None),
            CredentialSpec::Partial
        ));
    }

    #[test]
    fn resolve_credentials_password_only_is_partial() {
        assert!(matches!(
            resolve_credentials(None, Some("pass".to_string())),
            CredentialSpec::Partial
        ));
    }
}

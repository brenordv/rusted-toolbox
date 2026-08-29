use crate::models::{MqttCommand, MqttConfig};
use anyhow::Result;
use clap::{Args, Parser, Subcommand};

use common_cli::common_tool_args::CommonToolArgs;
use common_utils::constants::{CONFIG_UL_ITEM_LEVEL_2, CONFIG_UL_ITEM_LEVEL_3};

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
    //TODO: Bring the examples from the readme file to the after_help
    /// Read messages from a MQTT broker.
    #[command(after_help = "Examples:<ADD EXAMPLE>")]
    Read(ReadArgs),

    //TODO: Bring the examples from the readme file to the after_help
    /// Post a message to a MQTT broker.
    #[command(after_help = "Examples:<ADD EXAMPLE>")]
    Post(PostArgs),
}

fn print_runtime_info(args: &MqttConfig) {
    println!(
        "{} Host: {}:{}",
        CONFIG_UL_ITEM_LEVEL_2, args.host, args.port
    );

    let connection_type = if args.is_anonymous() {
        "Anonymous"
    } else {
        "Authenticated"
    };

    println!(
        "{} Connection type: {}",
        CONFIG_UL_ITEM_LEVEL_2, connection_type
    );
    println!("{} Topic: {}", CONFIG_UL_ITEM_LEVEL_2, args.topic);

    match args.command {
        MqttCommand::Read => {
            println!("{} Command: Read", CONFIG_UL_ITEM_LEVEL_2);
        }
        MqttCommand::Post => {
            println!("{} Command: Post", CONFIG_UL_ITEM_LEVEL_2);
            if let Some(msg) = &args.message {
                println!("{} Message: {}", CONFIG_UL_ITEM_LEVEL_3, msg);
            }
        }
    }
}

pub fn initialize() -> Result<MqttConfig> {
    let args = CliArgs::parse();

    let command_config = match args.command {
        Commands::Read(read_args) => MqttConfig {
            command: MqttCommand::Read,
            host: read_args.common.host,
            port: read_args.common.port,
            topic: read_args.common.topic,
            username: read_args.common.username,
            password: read_args.common.password,
            message: None,
        },
        Commands::Post(post_args) => MqttConfig {
            command: MqttCommand::Post,
            host: post_args.common.host,
            port: post_args.common.port,
            topic: post_args.common.topic,
            username: post_args.common.username,
            password: post_args.common.password,
            message: Some(post_args.message),
        },
    };

    args.common.app_boot_up(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        false,
        false,
        Some(|| print_runtime_info(&command_config)),
    );

    validate_host_and_port(&command_config.host, command_config.port)?;
    validate_user_and_password(&command_config.username, &command_config.password)?;
    validate_topic(&command_config.topic)?;

    match &command_config.command {
        MqttCommand::Read => {}
        MqttCommand::Post => {
            validate_message(&command_config.message)?;
        }
    }

    Ok(command_config)
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

fn validate_user_and_password(username: &Option<String>, password: &Option<String>) -> Result<()> {
    if username.is_none() && password.is_none() {
        return Ok(());
    };

    if username.is_some() && password.is_some() {
        return Ok(());
    };

    anyhow::bail!("Username and password are required together.");
}

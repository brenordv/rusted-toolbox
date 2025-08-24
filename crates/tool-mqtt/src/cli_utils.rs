use crate::models::{MqttArgs, MqttCommand};
use crate::string_traits::StringExt;
use anyhow::Result;
use clap::{Arg, Command};
use shared::command_line::cli_builder::CommandExt;
use shared::constants::general::DASH_LINE;

pub fn print_runtime_info(args: &MqttArgs) {
    println!("✉️ MQTT v{}", env!("CARGO_PKG_VERSION"));
    println!("{}", DASH_LINE);

    println!("Host: {}:{}", args.host, args.port);

    let connection_type = if args.is_anonymous() {
        "Anonymous"
    } else {
        "Authenticated"
    };

    println!("Connection type: {}", connection_type);
    println!("Topic: {}", args.topic);

    match args.command {
        MqttCommand::Unknown => {}
        MqttCommand::Read => {
            println!("Command: Read");
        }
        MqttCommand::Post => {
            println!("Command: Post");
            if let Some(msg) = &args.message {
                println!("Message: {}", msg);
            }
        }
    }
}

pub fn get_cli_arguments() -> Result<MqttArgs> {
    let matches = Command::new(env!("CARGO_PKG_NAME"))
        .add_basic_metadata(
            env!("CARGO_PKG_VERSION"),
            env!("CARGO_PKG_DESCRIPTION"),
            "Cli tool to perform quickly post to or read from a MQTT broker.",
        )
        .arg(
            Arg::new("command")
                .help("Command to execute.")
                .num_args(1)
                .required(false),
        )
        .arg(
            Arg::new("host")
                .long("host")
                .short('o')
                .required(true)
                .help("Host to connect to."),
        )
        .arg(
            Arg::new("port")
                .long("port")
                .short('p')
                .required(false)
                .default_value("1883")
                .help("Port to connect to. (Default: 1883)"),
        )
        .arg(
            Arg::new("topic")
                .long("topic")
                .short('t')
                .required(true)
                .help("Topic to post to or read from."),
        )
        .arg(
            Arg::new("message")
                .long("message")
                .short('m')
                .required(false)
                .help("Message to post to the topic."),
        )
        .arg(
            Arg::new("username")
                .long("username")
                .short('u')
                .required(false)
                .help("Username to connect with."),
        )
        .arg(
            Arg::new("password")
                .long("password")
                .short('a')
                .required(false)
                .help("Password for authentication."),
        )
        .get_matches();

    let command = match matches.get_one::<String>("command") {
        None => {
            // Field is required. This shouldn't happen.
            MqttCommand::Unknown
        }
        Some(cmd) => cmd.to_mqtt_command(),
    };

    let host = matches.get_one::<String>("host").unwrap();
    let port = matches.get_one::<String>("port").unwrap().parse::<u16>()?;
    let topic = matches
        .get_one::<String>("topic")
        .unwrap()
        .trim()
        .to_string();
    let message = matches.get_one::<String>("message");
    let username = matches.get_one::<String>("username");
    let password = matches.get_one::<String>("password");

    Ok(MqttArgs {
        command,
        host: host.clone(),
        port,
        topic: topic.clone(),
        message: message.cloned(),
        username: username.cloned(),
        password: password.cloned(),
    })
}

pub fn validate_args(args: &MqttArgs) -> Result<()> {
    match args.command {
        MqttCommand::Unknown => {
            anyhow::bail!("Unknown command. Review your command line and try again.");
        }
        MqttCommand::Read => {}
        MqttCommand::Post => {
            if args.message.is_none() {
                anyhow::bail!("Message is required for post command.");
            }
        }
    }

    validate_host_and_port(&args.host, args.port)?;
    validate_user_and_password(&args.username, &args.password)?;
    validate_topic(&args.topic)?;

    Ok(())
}

fn validate_host_and_port(host: &String, port: u16) -> Result<()> {
    if host.is_empty() {
        anyhow::bail!("Host is required.");
    }

    if port == 0 {
        anyhow::bail!("Port is required.");
    }

    Ok(())
}

fn validate_topic(topic: &String) -> Result<()> {
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

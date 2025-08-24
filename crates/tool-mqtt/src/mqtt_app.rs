use crate::models::MqttArgs;
use anyhow::Result;
use rumqttc::{AsyncClient, Event, EventLoop, Incoming, MqttOptions, Outgoing, QoS};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, error, info};

pub async fn read_messages(args: &MqttArgs) -> Result<()> {
    let (topic, client, mut event_loop) = create_connection_options("reader".to_string(), args);

    info!("Subscribing to topic: {}", topic);
    client.subscribe(&topic, QoS::AtMostOnce).await?;
    info!("Subscribed to topic: {}", topic);

    info!("Waiting for messages...");
    loop {
        match event_loop.poll().await {
            Ok(event) => match event {
                Event::Incoming(inc_message) => match inc_message {
                    Incoming::Publish(message) => {
                        debug!("Publish received: {:?}", message);
                        let decoded_payload = String::from_utf8(message.payload.to_vec())?;
                        info!("Message received: {:?}", decoded_payload);
                    }
                    _ => {}
                },
                Event::Outgoing(_) => {}
            },
            Err(e) => {
                error!("Error = {:?}", e);
            }
        }

        sleep(Duration::from_millis(200)).await;
    }
}

fn create_connection_options(
    client_id: String,
    args: &MqttArgs,
) -> (String, AsyncClient, EventLoop) {
    debug!("Creating connection options");
    let topic = args.topic.clone();
    let host = args.host.clone();
    let port = args.port;

    let mut mqtt_options = MqttOptions::new(
        format!("{}-{}", env!("CARGO_PKG_NAME"), client_id),
        host,
        port,
    );

    mqtt_options.set_keep_alive(Duration::from_secs(5));

    if !args.is_anonymous() {
        let username = args.username.clone().unwrap();
        let password = args.password.clone().unwrap();
        mqtt_options.set_credentials(username, password);
    }

    debug!("Creating connection");
    let (client, event_loop) = AsyncClient::new(mqtt_options, 10);

    debug!("Connecting to broker");
    (topic, client, event_loop)
}

pub async fn post_message(args: &MqttArgs) -> Result<()> {
    let message = match &args.message {
        None => {
            anyhow::bail!("No message provided");
        }
        Some(msg) => msg,
    };

    let (topic, client, mut event_loop) = create_connection_options("sender".to_string(), args);

    info!("Publishing message to topic: {}", topic);
    client
        .publish(topic, QoS::AtLeastOnce, false, message.clone())
        .await?;

    info!("Message published. Waiting for it to be flushed...");
    loop {
        match event_loop.poll().await {
            Ok(event) => match event {
                Event::Incoming(inc_message) => match inc_message {
                    Incoming::PubAck(_) => {
                        info!("Message publication acknowledged!");
                        break;
                    }
                    _ => {}

                },
                Event::Outgoing(outgoing) => match outgoing {
                    Outgoing::Publish(_) => {
                        info!("Message published. Waiting for ack...");
                    }
                    _ => {}
                },
            },
            Err(e) => {
                error!("Error = {:?}", e);
            }
        }

        sleep(Duration::from_millis(100)).await;
    }

    Ok(())
}

use crate::models::MqttConfig;
use anyhow::Result;
use common_utils_ext::new_guid::new_guid;
use rumqttc::{AsyncClient, Event, EventLoop, Incoming, MqttOptions, Outgoing, QoS};
use std::borrow::Cow;
use std::io::{self, Write};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

/// Renders a raw MQTT payload for stdout. Bytes decode through lossy UTF-8
/// conversion; control characters (C0, DEL, C1) and Unicode
/// bidirectional-control characters render as escape sequences so
/// broker-controlled bytes cannot inject terminal escapes or reorder the
/// display. Every other character passes through unchanged, keeping JSON and
/// plain-text payloads byte-identical for piping. The flag reports whether the
/// payload contained invalid UTF-8.
fn format_payload_for_display(payload: &[u8]) -> (String, bool) {
    let decoded = String::from_utf8_lossy(payload);
    let was_lossy = matches!(decoded, Cow::Owned(_));

    let mut escaped = String::with_capacity(decoded.len());
    for c in decoded.chars() {
        if c.is_control() || is_bidi_control(c) {
            escaped.extend(c.escape_debug());
        } else {
            escaped.push(c);
        }
    }

    (escaped, was_lossy)
}

/// Unicode bidirectional-control characters (the embedding/override set, the
/// isolate set, and the implicit marks LRM/RLM/ALM) can visually reorder
/// terminal output, so display treats them like control characters.
fn is_bidi_control(c: char) -> bool {
    matches!(
        c,
        '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{200E}' | '\u{200F}' | '\u{061C}'
    )
}

pub async fn read_messages(args: &MqttConfig) -> Result<()> {
    let (topic, client, mut event_loop) = create_connection_options("reader".to_string(), args);

    info!("Subscribing to topic: {}", topic);
    client.subscribe(&topic, QoS::AtMostOnce).await?;
    info!("Subscribed to topic: {}", topic);

    info!("Waiting for messages...");
    loop {
        match event_loop.poll().await {
            Ok(event) => match event {
                Event::Incoming(inc_message) => {
                    if let Incoming::Publish(message) = inc_message {
                        debug!("Publish received: {:?}", message);
                        let (display, was_lossy) = format_payload_for_display(&message.payload);
                        if was_lossy {
                            warn!("Payload is not valid UTF-8; invalid bytes replaced with U+FFFD for display");
                        }
                        if let Err(e) = writeln!(io::stdout().lock(), "{}", display) {
                            // A closed pipe (e.g. piping into head) ends the
                            // subscription cleanly.
                            if e.kind() == io::ErrorKind::BrokenPipe {
                                debug!("stdout closed; stopping subscriber");
                                return Ok(());
                            }
                            return Err(e.into());
                        }
                    }
                }
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
    args: &MqttConfig,
) -> (String, AsyncClient, EventLoop) {
    debug!("Creating connection options");
    let topic = args.topic.clone();
    let host = args.host.clone();
    let port = args.port;

    let mut mqtt_options = MqttOptions::new(
        //Adding a guid to the id so that we can have multiple instances of the same client
        format!("{}-{}-{}", env!("CARGO_PKG_NAME"), client_id, new_guid()),
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

pub async fn post_message(args: &MqttConfig) -> Result<()> {
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
                Event::Incoming(inc_message) => {
                    if let Incoming::PubAck(_) = inc_message {
                        info!("Message publication acknowledged!");
                        break;
                    }
                }
                Event::Outgoing(outgoing) => {
                    if let Outgoing::Publish(_) = outgoing {
                        info!("Message published. Waiting for ack...");
                    }
                }
            },
            Err(e) => {
                error!("Error = {:?}", e);
            }
        }

        sleep(Duration::from_millis(100)).await;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_payload_passes_plain_utf8_through() {
        let (display, lossy) = format_payload_for_display("hello world".as_bytes());

        assert_eq!(display, "hello world");
        assert!(!lossy);
    }

    #[test]
    fn format_payload_keeps_json_byte_identical() {
        let (display, lossy) = format_payload_for_display(br#"{"a":1,"b":"x y"}"#);

        assert_eq!(display, r#"{"a":1,"b":"x y"}"#);
        assert!(!lossy);
    }

    #[test]
    fn format_payload_escapes_the_c0_escape_byte() {
        let (display, _) = format_payload_for_display(b"\x1b[31mred");

        assert_eq!(display, "\\u{1b}[31mred");
    }

    #[test]
    fn format_payload_escapes_c1_csi() {
        // U+009B (CSI) arrives as the valid UTF-8 sequence C2 9B, so the
        // conversion is not lossy and the escaping alone must neutralize it.
        let (display, lossy) = format_payload_for_display(b"\xc2\x9bx");

        assert_eq!(display, "\\u{9b}x");
        assert!(!lossy);
    }

    #[test]
    fn format_payload_escapes_bidi_override() {
        let (display, _) = format_payload_for_display("a\u{202E}b".as_bytes());

        assert_eq!(display, "a\\u{202e}b");
    }

    #[test]
    fn format_payload_escapes_implicit_bidi_marks() {
        let (display, _) = format_payload_for_display("a\u{200E}b\u{200F}c\u{061C}d".as_bytes());

        assert_eq!(display, "a\\u{200e}b\\u{200f}c\\u{61c}d");
    }

    #[test]
    fn format_payload_escapes_newlines_to_keep_one_line_per_message() {
        let (display, _) = format_payload_for_display(b"line1\nline2");

        assert_eq!(display, "line1\\nline2");
    }

    #[test]
    fn format_payload_replaces_invalid_utf8_and_flags_lossy() {
        let (display, lossy) = format_payload_for_display(b"a\xffb");

        assert_eq!(display, "a\u{FFFD}b");
        assert!(lossy);
    }
}

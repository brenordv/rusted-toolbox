use crate::models::MqttConfig;
use anyhow::Result;
use common_utils::string_utils::escape_for_terminal_display;
use common_utils_ext::new_guid::new_guid;
use rumqttc::{AsyncClient, Event, EventLoop, Incoming, MqttOptions, Outgoing, QoS};
use std::borrow::Cow;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

/// First retry delay after a failed poll; doubled per consecutive failure.
const INITIAL_RECONNECT_DELAY: Duration = Duration::from_millis(500);

/// Ceiling for the reconnect backoff.
const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(30);

/// Longest uninterrupted sleep slice, so Ctrl+C stays responsive even in the
/// middle of a long backoff wait.
const SHUTDOWN_POLL_SLICE: Duration = Duration::from_millis(200);

/// Sleeps for `total` in short slices, returning `true` as soon as the
/// shutdown flag flips during the wait.
async fn sleep_unless_shutdown(total: Duration, shutdown: &AtomicBool) -> bool {
    let mut remaining = total;
    while !remaining.is_zero() {
        if shutdown.load(Ordering::Relaxed) {
            return true;
        }
        let slice = remaining.min(SHUTDOWN_POLL_SLICE);
        sleep(slice).await;
        remaining = remaining.saturating_sub(slice);
    }
    shutdown.load(Ordering::Relaxed)
}

/// Logs a failed poll once per distinct error: the first occurrence at error
/// level, identical repeats at debug, so a dead broker cannot flood the log
/// with the same line every retry. `last_error` carries the previously logged
/// rendering; a recovery clears it (see the call sites).
fn log_connection_error(rendered: String, last_error: &mut Option<String>) {
    if last_error.as_deref() == Some(rendered.as_str()) {
        debug!(error = %rendered, "broker connection error repeated");
    } else {
        error!(error = %rendered, "broker connection error; retrying with backoff");
        *last_error = Some(rendered);
    }
}

/// Renders a raw MQTT payload for stdout. Bytes decode through lossy UTF-8
/// conversion; control characters (C0, DEL, C1) and Unicode
/// bidirectional-control characters render as escape sequences
/// (`common_utils::string_utils::escape_for_terminal_display`) so
/// broker-controlled bytes cannot inject terminal escapes or reorder the
/// display. Every other character passes through unchanged, keeping JSON and
/// plain-text payloads byte-identical for piping. The flag reports whether the
/// payload contained invalid UTF-8.
fn format_payload_for_display(payload: &[u8]) -> (String, bool) {
    let decoded = String::from_utf8_lossy(payload);
    let was_lossy = matches!(decoded, Cow::Owned(_));

    (escape_for_terminal_display(&decoded), was_lossy)
}

pub async fn read_messages(args: &MqttConfig, shutdown: Arc<AtomicBool>) -> Result<()> {
    let (topic, client, mut event_loop) = create_connection_options("reader".to_string(), args);

    info!("Subscribing to topic: {}", topic);
    client.subscribe(&topic, QoS::AtMostOnce).await?;
    info!("Subscribed to topic: {}", topic);

    info!("Waiting for messages...");
    let mut reconnect_delay = INITIAL_RECONNECT_DELAY;
    let mut last_error: Option<String> = None;

    loop {
        if shutdown.load(Ordering::Relaxed) {
            info!("Shutdown requested; stopping subscriber");
            return Ok(());
        }

        match event_loop.poll().await {
            Ok(event) => {
                reconnect_delay = INITIAL_RECONNECT_DELAY;
                if last_error.take().is_some() {
                    info!("Broker connection recovered");
                }
                match event {
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
                }
            }
            Err(e) => {
                log_connection_error(format!("{e:?}"), &mut last_error);
                if sleep_unless_shutdown(reconnect_delay, &shutdown).await {
                    info!("Shutdown requested; stopping subscriber");
                    return Ok(());
                }
                reconnect_delay = (reconnect_delay * 2).min(MAX_RECONNECT_DELAY);
            }
        }
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

pub async fn post_message(args: &MqttConfig, shutdown: Arc<AtomicBool>) -> Result<()> {
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
    let mut reconnect_delay = INITIAL_RECONNECT_DELAY;
    let mut last_error: Option<String> = None;

    loop {
        if shutdown.load(Ordering::Relaxed) {
            anyhow::bail!("interrupted before the broker acknowledged the publication");
        }

        match event_loop.poll().await {
            Ok(event) => {
                reconnect_delay = INITIAL_RECONNECT_DELAY;
                if last_error.take().is_some() {
                    info!("Broker connection recovered");
                }
                match event {
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
                }
            }
            Err(e) => {
                log_connection_error(format!("{e:?}"), &mut last_error);
                if sleep_unless_shutdown(reconnect_delay, &shutdown).await {
                    anyhow::bail!("interrupted before the broker acknowledged the publication");
                }
                reconnect_delay = (reconnect_delay * 2).min(MAX_RECONNECT_DELAY);
            }
        }
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

    #[test]
    fn log_connection_error_remembers_the_last_distinct_error() {
        let mut last_error = None;

        log_connection_error("refused".to_string(), &mut last_error);
        assert_eq!(last_error.as_deref(), Some("refused"));

        log_connection_error("refused".to_string(), &mut last_error);
        assert_eq!(last_error.as_deref(), Some("refused"));

        log_connection_error("timeout".to_string(), &mut last_error);
        assert_eq!(last_error.as_deref(), Some("timeout"));
    }

    #[tokio::test]
    async fn sleep_unless_shutdown_returns_true_when_flag_is_already_set() {
        let shutdown = AtomicBool::new(true);

        assert!(sleep_unless_shutdown(Duration::from_secs(60), &shutdown).await);
    }

    #[tokio::test]
    async fn sleep_unless_shutdown_completes_quiet_waits_with_false() {
        let shutdown = AtomicBool::new(false);

        assert!(!sleep_unless_shutdown(Duration::from_millis(1), &shutdown).await);
    }
}

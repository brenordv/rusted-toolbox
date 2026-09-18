use crate::chat_session::ChatSession;
use crate::connection::TcpConnection;
use crate::encrypt::encryption::Encryption;
use crate::encrypt::message_encrypter::MessageEncrypter;
use crate::models::shared_types::{RuntimeType, UiMessage};
use crate::models::whisper_args::WhisperArgs;
use crate::ui::chat_ui::ChatUi;
use anyhow::Result;
use common_cli::tool_exit_helpers::exit_success;
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::thread;
use std::thread::{JoinHandle, sleep};
use std::time::{Duration, SystemTime};
use tracing::{debug, error, info, warn};

pub fn start_chat_session(cli_args: WhisperArgs) -> Result<ChatSession> {
    let (tx_own_messages, rx_own_messages) = mpsc::channel::<String>();
    let (tx_peer_message, rx_peer_message) = mpsc::channel::<UiMessage>();
    let peer_encrypter: MessageEncrypter;
    let mut chat_connection: TcpConnection;

    info!("Generating Keypair...");
    let keypair = Encryption::new_keypair()?;
    let pub_key = keypair.get_public_key()?;

    match cli_args.runtime {
        RuntimeType::Host => {
            info!("Initializing listener on: {}", cli_args.host);
            let listener = TcpListener::bind(cli_args.host)?;

            info!("Waiting for someone to talk to...");
            let stream = match listener.accept() {
                Ok((socket, addr)) => {
                    info!("Client connected! Client address: {addr:?}");
                    socket
                }
                Err(e) => {
                    anyhow::bail!("failed to accept client connection: {e}");
                }
            };

            info!("Creating connection manager...");
            chat_connection = TcpConnection::new_from_connection(stream)?;

            info!("Starting handshake...");
            info!("Waiting for client to send public key...");
            let client_pub_key_string = match chat_connection.read_message()? {
                None => {
                    error!("Handshake failed! No public key received from client.");
                    anyhow::bail!("Handshake failed! No public key received from client.");
                }
                Some(received_client_pub_key) => {
                    info!("Received public key from client...");
                    received_client_pub_key
                }
            };

            let server_pub_key =
                Encryption::create_pub_key_from_base64(client_pub_key_string.as_str())?;
            peer_encrypter = MessageEncrypter::new(server_pub_key)?;

            info!("Sending public key to client...");
            chat_connection.write_message(pub_key.as_str())?;

            info!("Handshake completed!");
        }
        RuntimeType::Client => {
            info!("Connecting to: {}", cli_args.host);
            let stream = TcpStream::connect(cli_args.host)?;

            info!("Connected! Creating connection manager...");
            chat_connection = TcpConnection::new_from_connection(stream)?;

            info!("Starting handshake...");
            info!("Sending public key...");
            chat_connection.write_message(pub_key.as_str())?;

            info!("Waiting for server response to complete handshake...");
            let server_pub_key_string = match chat_connection.read_message()? {
                None => {
                    error!("Handshake failed! No public key received from server.");
                    anyhow::bail!("Handshake failed! No public key received from server.");
                }
                Some(received_server_pub_key) => {
                    info!("Received public key from server. Completing handshake...");
                    received_server_pub_key
                }
            };

            let server_pub_key =
                Encryption::create_pub_key_from_base64(server_pub_key_string.as_str())?;
            peer_encrypter = MessageEncrypter::new(server_pub_key)?;
            info!("Handshake completed!");
        }
    };

    Ok(ChatSession::new(
        cli_args.role.clone().to_string(),
        chat_connection,
        keypair,
        peer_encrypter,
        tx_own_messages,
        tx_peer_message,
        rx_peer_message,
        rx_own_messages,
    ))
}

type ChatHandlers = (
    JoinHandle<Result<()>>,
    JoinHandle<Result<()>>,
    JoinHandle<Result<()>>,
);

/// True when the error chain bottoms out in an I/O error that means the peer went away.
fn is_disconnect(err: &anyhow::Error) -> bool {
    err.downcast_ref::<std::io::Error>().is_some_and(|io_err| {
        matches!(
            io_err.kind(),
            std::io::ErrorKind::UnexpectedEof
                | std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::ConnectionAborted
                | std::io::ErrorKind::BrokenPipe
        )
    })
}

/// Encrypts and writes one outgoing message. Returns a system notice describing why the message
/// was not sent, or `None` on success.
fn send_outgoing_message(
    conn: &mut TcpConnection,
    encrypter: &MessageEncrypter,
    plain_msg: &str,
    max_plaintext_len: usize,
) -> Option<String> {
    if plain_msg.len() > max_plaintext_len {
        return Some(format!(
            "message not sent: exceeds {max_plaintext_len} bytes"
        ));
    }

    let encrypted_msg = match encrypter.encrypt_message(plain_msg) {
        Ok(encrypted_msg) => encrypted_msg,
        Err(e) => return Some(format!("message not sent: encryption failed: {e:#}")),
    };

    debug!(
        plaintext_bytes = plain_msg.len(),
        ciphertext_bytes = encrypted_msg.len(),
        "Sending encrypted message"
    );

    if let Err(e) = conn.write_message(encrypted_msg.as_str()) {
        return Some(format!("message not sent: connection error: {e:#}"));
    }

    None
}

pub fn create_handlers(mut chat_session: ChatSession) -> Result<ChatHandlers> {
    // Taking ownership of the required values before moving them into the threads.
    let outgoing_msg_receiver = chat_session.take_outgoing_receiver();
    let outgoing_conn = chat_session.split_connection()?;
    let incoming_conn = chat_session.split_connection()?;
    let incoming_msg_receiver = chat_session.take_incoming_receiver();

    // Cloning the encrypters/decrypters
    // Note: Not a fan of cloning the private key object, but its ok for now.
    let msg_encrypter = chat_session.get_message_encrypter();
    let message_decrypter = chat_session.get_message_decrypter().clone();

    // Getting all the other values
    let outgoing_role_name = chat_session.get_role_name();
    let incoming_role_name = chat_session.get_role_name();
    let ui_role_name = chat_session.get_role_name();
    let incoming_msg_transmitter = chat_session.get_incoming_transmitter();
    let outgoing_status_transmitter = chat_session.get_incoming_transmitter();
    let outgoing_msg_transmitter = chat_session.get_outgoing_transmitter();

    // Creating the threads
    let outgoing_messages_handler = thread::spawn(move || -> Result<()> {
        info!("Starting outgoing messages handler...");
        let mut conn = outgoing_conn;
        let encrypter = msg_encrypter;
        let role_name = outgoing_role_name;
        let out_msg_rx = outgoing_msg_receiver;
        let status_tx = outgoing_status_transmitter;
        // PKCS#1 v1.5 caps the plaintext at the peer key's modulus size minus 11 bytes.
        let max_plaintext_len = encrypter.max_plaintext_len();

        loop {
            match out_msg_rx.try_recv() {
                Ok(plain_msg) => {
                    if plain_msg.is_empty() {
                        debug!(role = %role_name, "Empty message received. Ignoring...");
                        continue;
                    }

                    match send_outgoing_message(
                        &mut conn,
                        &encrypter,
                        &plain_msg,
                        max_plaintext_len,
                    ) {
                        None => debug!(role = %role_name, "Message sent successfully."),
                        Some(notice) => {
                            warn!(role = %role_name, notice, "Outgoing message dropped");
                            if status_tx.send(UiMessage::System(notice)).is_err() {
                                // The UI is gone; there is nobody left to notify.
                                return Ok(());
                            }
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    info!(role = %role_name, "Outgoing channel closed. Stopping handler.");
                    return Ok(());
                }
            }
            sleep(Duration::from_millis(100));
        }
    });

    let incoming_message_handler = thread::spawn(move || -> Result<()> {
        info!("Starting incoming messages handler...");
        let mut conn = incoming_conn;
        let decrypter = message_decrypter;
        let incoming_msg_tx = incoming_msg_transmitter;
        let role_name = incoming_role_name;

        loop {
            let event = match conn.read_message() {
                Ok(None) => {
                    // A zero-length frame carries nothing to show.
                    None
                }
                Ok(Some(encrypted_message)) => {
                    if encrypted_message.is_empty() {
                        debug!(role = %role_name, "Empty message received. Ignoring...");
                        continue;
                    }

                    debug!(
                        role = %role_name,
                        ciphertext_bytes = encrypted_message.len(),
                        "Received encrypted message"
                    );

                    match decrypter.decrypt_message(encrypted_message.as_str()) {
                        Ok(plain_message) => {
                            debug!(
                                role = %role_name,
                                plaintext_bytes = plain_message.len(),
                                "Decrypted message"
                            );
                            Some(UiMessage::Peer(plain_message))
                        }
                        Err(e) => {
                            warn!(role = %role_name, error = %format!("{e:#}"), "Dropping undecryptable message");
                            Some(UiMessage::System(format!(
                                "incoming message dropped: {e:#}"
                            )))
                        }
                    }
                }
                Err(e) if is_disconnect(&e) => {
                    info!(role = %role_name, "Peer disconnected.");
                    let _ =
                        incoming_msg_tx.send(UiMessage::System("peer disconnected".to_string()));
                    return Ok(());
                }
                Err(e) => {
                    // A failed read leaves the length-prefixed stream desynchronized: the
                    // frame's payload may be partially consumed or still on the wire, so the
                    // next read would parse payload bytes as a header. Receiving cannot
                    // safely continue.
                    warn!(role = %role_name, error = %format!("{e:#}"), "Unreadable frame. Stopping the incoming handler.");
                    let _ = incoming_msg_tx
                        .send(UiMessage::System(format!("receiving stopped: {e:#}")));
                    return Ok(());
                }
            };

            if let Some(event) = event
                && incoming_msg_tx.send(event).is_err()
            {
                // The UI is gone; there is nobody left to notify.
                return Ok(());
            }
            sleep(Duration::from_millis(100));
        }
    });

    let ui_handler = thread::spawn(move || -> Result<()> {
        info!("Starting UI handler...");
        let role_name = ui_role_name;
        let out_msg_tx = outgoing_msg_transmitter;
        let in_msg_rx = incoming_msg_receiver;

        let ui = ChatUi::new(out_msg_tx, in_msg_rx, role_name);

        let session_start = SystemTime::now();

        ui.run()?;

        let duration = session_start.elapsed().unwrap_or_default();

        info!("Disconnected. Session duration: {:?}", duration);

        exit_success();
    });

    // All set up.
    Ok((
        outgoing_messages_handler,
        incoming_message_handler,
        ui_handler,
    ))
}

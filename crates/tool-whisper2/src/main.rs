mod chat_session;
mod cli_utils;
mod connection;
mod encrypt;
mod models;
mod ui;

use crate::chat_session::ChatSession;
use crate::cli_utils::get_cli_arguments;
use crate::connection::Connection;
use crate::encrypt::encryption::Encryption;
use crate::encrypt::message_encrypter::MessageEncrypter;
use crate::models::shared_types::RuntimeType;
use crate::ui::chat_ui::ChatUi;
use anyhow::Result;
use shared::logging::app_logger::LogLevel;
use shared::logging::logging_helpers::get_default_log_builder;
use shared::system::tool_exit_helpers::exit_success;
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::thread;
use std::thread::{sleep, JoinHandle};
use std::time::{Duration, SystemTime};
use tracing::{debug, error, info};

fn main() -> Result<()> {
    get_default_log_builder(env!("CARGO_PKG_NAME"), LogLevel::Info)
        .log_to_console(true)
        .log_to_file(true, false)
        .init();

    let cli_args = get_cli_arguments()?;
    let (tx_own_messages, rx_own_messages) = mpsc::channel::<String>();
    let (tx_peer_message, rx_peer_message) = mpsc::channel::<String>();
    let peer_encrypter: MessageEncrypter;
    let mut chat_connection: Connection;

    info!("Generating Keypair...");
    let keypair = Encryption::new_keypair()?;
    let pub_key = keypair.get_public_key()?;

    match cli_args.runtime {
        RuntimeType::Host => {
            info!("Initializing listener on: {}", cli_args.host);
            let listener = TcpListener::bind(cli_args.host)?;

            info!("Waiting for client connections...");
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
            chat_connection = Connection::new_from_connection(stream)?;

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

            //let (rd_handle, wt_handle) = chatter_over_tcp(&mut stream, "HOST".to_string(), tx_own_messages, tx_peer_message, keypair)?;
            //read_handle = rd_handle;
            //write_handle = wt_handle;
        }
        RuntimeType::Client => {
            info!("Connecting to: {}", cli_args.host);
            let stream = TcpStream::connect(cli_args.host)?;

            info!("Connected! Creating connection manager...");
            chat_connection = Connection::new_from_connection(stream)?;

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

            // let (rd_handle, wt_handle) = chatter_over_tcp(&mut stream, "CLIENT".to_string(), tx_own_messages, tx_peer_message, keypair)?;
            // read_handle = rd_handle;
            // write_handle = wt_handle;
        }
    };

    let chat_session = ChatSession::new(
        cli_args.role.clone().to_string(),
        chat_connection,
        keypair,
        peer_encrypter,
        tx_own_messages,
        tx_peer_message,
        rx_peer_message,
        rx_own_messages,
    );

    let (outgoing_messages_handler, incoming_message_handler, ui_handler) =
        create_handlers(chat_session)?;

    let _ = outgoing_messages_handler.join();
    let _ = incoming_message_handler.join();
    let _ = ui_handler.join();

    Ok(())
}

fn create_handlers(
    mut chat_session: ChatSession,
) -> Result<(
    JoinHandle<Result<()>>,
    JoinHandle<Result<()>>,
    JoinHandle<Result<()>>,
)> {
    // Taking ownership of the required values before moving them into the threads.
    let outgoing_msg_receiver = chat_session.take_outgoing_receiver();
    let outgoing_conn = chat_session.split_connection()?;
    let incoming_conn = chat_session.split_connection()?;
    let incoming_msg_receiver = chat_session.take_incoming_receiver();

    // Cloning the encrypters/decrypters
    // Note: Not a fan of cloning the private key object, but its ok for now.
    let msg_encrypter = chat_session.get_message_encrypter().clone();
    let message_decrypter = chat_session.get_message_decrypter().clone();

    // Getting all the other values
    let outgoing_role_name = chat_session.get_role_name().clone();
    let incoming_role_name = chat_session.get_role_name().clone();
    let ui_role_name = chat_session.get_role_name().clone();
    let incoming_msg_transmitter = chat_session.get_incoming_transmitter().clone();
    let outgoing_msg_transmitter = chat_session.get_outgoing_transmitter().clone();

    // Creating the threads
    let outgoing_messages_handler = thread::spawn(move || -> Result<()> {
        info!("Starting outgoing messages handler...");
        let mut conn = outgoing_conn;
        let encrypter = msg_encrypter;
        let role_name = outgoing_role_name;
        let out_msg_rx = outgoing_msg_receiver;

        loop {
            match out_msg_rx.try_recv() {
                Ok(plain_msg) => {
                    if plain_msg.is_empty() {
                        debug!("[{}] Empty message received. Ignoring...", role_name);
                        continue;
                    }

                    let encrypted_msg = encrypter.encrypt_message(&plain_msg)?;
                    debug!(
                        "[{}] Sending encrypted message of size: {}. Msg: {}",
                        role_name,
                        encrypted_msg.len(),
                        plain_msg
                    );
                    conn.write_message(encrypted_msg.as_str())?;
                    debug!("[{}] Message sent successfully.", role_name);
                }
                Err(_) => {
                    // Careful logging stuff here. No messages returned from the `try_recv` call
                    // equals errors (even though it is now).
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
            match conn.read_message()? {
                None => {
                    // Aaaaalll allooooone!
                    // Nobody is talking to us now. :(
                }
                Some(encrypted_message) => {
                    if encrypted_message.is_empty() {
                        debug!("[{}] Empty message received. Ignoring...", role_name);
                        continue;
                    }

                    debug!(
                        "[{}] Received encrypted message of size: {}",
                        role_name,
                        encrypted_message.len()
                    );

                    let plain_message = decrypter.decrypt_message(encrypted_message.as_str())?;

                    debug!(
                        "[{}] Decrypted message size: {}",
                        role_name,
                        plain_message.len()
                    );

                    incoming_msg_tx.send(plain_message)?;
                }
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

        Ok(())
    });

    // All set up.
    Ok((
        outgoing_messages_handler,
        incoming_message_handler,
        ui_handler,
    ))
}

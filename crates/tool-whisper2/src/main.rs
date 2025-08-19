mod models;
mod cli_utils;
mod ui;

use std::io::{Read, Write};
use tracing::{error, info};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::sync::mpsc::Sender;
use std::thread;
use std::thread::{sleep, JoinHandle};
use std::time::Duration;
use anyhow::Result;
use shared::logging::app_logger::LogLevel;
use shared::logging::logging_helpers::get_default_log_builder;
use crate::cli_utils::get_cli_arguments;
use crate::models::shared_types::RuntimeType;
use crate::ui::dual_column::DualColumnUi;

fn main() -> Result<()> {
    get_default_log_builder(env!("CARGO_PKG_NAME"), LogLevel::Info)
        .log_to_console(false)
        .log_to_file(true, false)
        .init();

    let cli_args = get_cli_arguments()?;
    let (tx_own_messages, rx_own_messages) = mpsc::channel::<String>();
    let (tx_peer_message, rx_peer_message) = mpsc::channel::<String>();

    let ui_handle = thread::spawn(move || -> Result<()> {
        let mut ui = DualColumnUi::new(rx_peer_message, rx_own_messages, cli_args.role);
        ui.run()?;
        Ok(())
    });

    let read_handle: JoinHandle<Result<()>>;
    let write_handle: JoinHandle<Result<()>>;

    match cli_args.runtime {
        RuntimeType::Host => {
            info!("Initializing listener on: {}", cli_args.host);
            let listener = TcpListener::bind(cli_args.host)?;

            info!("Waiting for client connections...");
            let mut stream = match listener.accept() {
                Ok((socket, addr)) => {
                    info!("Client connected! Client address: {addr:?}");
                    socket
                }
                Err(e) => {
                    anyhow::bail!("failed to accept client connection: {e}");
                }
            };

            let (rd_handle, wt_handle) = chatter_over_tcp(&mut stream, "HOST".to_string(), tx_own_messages, tx_peer_message)?;
            read_handle = rd_handle;
            write_handle = wt_handle;

        }
        RuntimeType::Client => {
            info!("Connecting to: {}", cli_args.host);
            let mut stream = TcpStream::connect(cli_args.host)?;

            info!("Connected!");
            let (rd_handle, wt_handle) = chatter_over_tcp(&mut stream, "CLIENT".to_string(), tx_own_messages, tx_peer_message)?;
            read_handle = rd_handle;
            write_handle = wt_handle;
        }
    };

    let _ = read_handle.join();
    let _ = write_handle.join();
    let _ = ui_handle.join();

    Ok(())
}

fn chatter_over_tcp(stream: &mut TcpStream, name: String, tx_own_messages: Sender<String>, tx_peer_message: Sender<String>)
    -> Result<(JoinHandle<Result<()>>, JoinHandle<Result<()>>)>
{
    let mut read_stream = stream.try_clone()?;
    let read_name = name.clone();
    let read_handle = thread::spawn(move || -> Result<()> {
        let mut count: usize = 0;
        loop {
            count += 1;
            // Read message length first (4 bytes). Theoretical limit of 4GB per message, but please don't do that.
            let mut len_buf = [0u8; 4];
            if let Err(e) = read_stream.read_exact(&mut len_buf) {
                info!("Error reading message length: {e}");
                anyhow::bail!("Error reading message length: {e}");
            }

            let message_len = u32::from_be_bytes(len_buf) as usize;

            let mut message_buf = vec![0u8; message_len];

            match read_stream.read_exact(&mut message_buf) {
                Ok(()) => {
                    let message = String::from_utf8_lossy(&message_buf);

                    tx_peer_message.send(message.to_string())?;
                    info!("[{}-{}][{}bytes] Received: {}",
                        read_name,
                        count,
                        message_len,
                        message);
                }
                Err(e) => {
                    error!("[{}-{}][error] Error: {}", read_name, count, e);
                    anyhow::bail!("Error reading message: {e}");
                }
            }
            sleep(Duration::from_millis(100));
        }
    });

    let mut write_stream = stream.try_clone()?;
    let write_name = name.clone();
    let write_handle = thread::spawn(move || -> Result<()> {
        let mut count: usize = 0;

        loop {
            count += 1;
            let message = format!("Hello from {}! Count: {:0>5}", write_name, count);

            let message_bytes = message.as_bytes();
            let message_len = message_bytes.len() as u32;

            // Send length first (4 bytes, big-endian)
            write_stream.write_all(&message_len.to_be_bytes())?;
            // Then send the actual message
            write_stream.write_all(message_bytes)?;

            tx_own_messages.send(message.clone())?;

            info!("[{}-{}][{}bytes] Sent: {}",
            write_name,
            count,
            message_len,
            message);

            sleep(Duration::from_millis(100));
        }
    });

    Ok((read_handle, write_handle))
}
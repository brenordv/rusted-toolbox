mod chat_session;
mod cli_utils;
mod connection;
mod encrypt;
mod models;
mod ui;
mod whisper_app;

use crate::cli_utils::initialize;
use crate::whisper_app::{create_handlers, start_chat_session};
use anyhow::Result;
use tracing::error;

fn main() -> Result<()> {
    let cli_args = initialize()?;
    let chat_session = start_chat_session(cli_args)?;

    let (outgoing_messages_handler, incoming_message_handler, ui_handler) =
        create_handlers(chat_session)?;

    for (name, handle) in [
        ("outgoing", outgoing_messages_handler),
        ("incoming", incoming_message_handler),
        ("ui", ui_handler),
    ] {
        match handle.join() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => error!(thread = name, error = %format!("{e:#}"), "Chat thread failed"),
            Err(_) => error!(thread = name, "Chat thread panicked"),
        }
    }

    Ok(())
}

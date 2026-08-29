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

fn main() -> Result<()> {
    let cli_args = initialize()?;
    let chat_session = start_chat_session(cli_args)?;

    let (outgoing_messages_handler, incoming_message_handler, ui_handler) =
        create_handlers(chat_session)?;

    let _ = outgoing_messages_handler.join();
    let _ = incoming_message_handler.join();
    let _ = ui_handler.join();

    Ok(())
}

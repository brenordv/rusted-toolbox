mod chat_session;
mod cli_utils;
mod connection;
mod encrypt;
mod models;
mod ui;
mod whisper_app;

use crate::cli_utils::get_cli_arguments;
use crate::whisper_app::{create_handlers, start_chat_session};
use anyhow::Result;
use shared::logging::app_logger::LogLevel;
use shared::logging::logging_helpers::get_default_log_builder;

fn main() -> Result<()> {
    get_default_log_builder(env!("CARGO_PKG_NAME"), LogLevel::Info)
        .log_to_console(true)
        .log_to_file(true, false)
        .init();

    let cli_args = get_cli_arguments()?;
    let chat_session = start_chat_session(cli_args)?;

    let (outgoing_messages_handler, incoming_message_handler, ui_handler) =
        create_handlers(chat_session)?;

    let _ = outgoing_messages_handler.join();
    let _ = incoming_message_handler.join();
    let _ = ui_handler.join();

    Ok(())
}

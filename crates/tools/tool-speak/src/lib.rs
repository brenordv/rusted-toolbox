//! speak: sends text to a text-to-speech server and plays the audio it answers with.

pub mod cli_utils;
pub mod config;
pub mod design;
pub mod first_run;
pub mod gui;
pub mod models;
pub mod speak_app;
pub mod speech_client;

#[cfg(test)]
pub(crate) mod test_support;

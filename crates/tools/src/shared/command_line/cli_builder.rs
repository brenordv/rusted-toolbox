use crate::shared::constants::general::AUTHOR_NAME;
use clap::{Arg, Command};
use std::path::PathBuf;

pub trait CommandExt {
    fn add_basic_metadata(
        self,
        version: &'static str,
        about: &'static str,
        long_about: &'static str,
    ) -> Self;

    fn preset_arg_config(self, alt_help_text: Option<&str>) -> Self;
    fn preset_arg_connection_string(self, help_text: &'static str) -> Self;
    fn preset_arg_verbose(self, alt_help_text: Option<&str>) -> Self;
}

impl CommandExt for Command {
    fn add_basic_metadata(
        self,
        version: &'static str,
        about: &'static str,
        long_about: &'static str,
    ) -> Self {
        self.version(version)
            .author(AUTHOR_NAME)
            .about(about)
            .long_about(long_about)
    }

    fn preset_arg_config(self, alt_help_text: Option<&str>) -> Self {
        let help_text = match alt_help_text {
            Some(text) => text.to_string(),
            None => "Configuration file path (JSON format)".to_string(),
        };

        self.arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .help(help_text)
                .value_parser(clap::value_parser!(PathBuf)),
        )
    }

    fn preset_arg_connection_string(self, help_text: &'static str) -> Self {
        self.arg(
            Arg::new("connection-string")
                .short('s')
                .long("connection-string")
                .value_name("STRING")
                .help(help_text),
        )
    }

    fn preset_arg_verbose(self, alt_help_text: Option<&str>) -> Self {
        let help_text = match alt_help_text {
            Some(text) => text.to_string(),
            None => "Enable verbose output".to_string(),
        };

        self.arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .action(clap::ArgAction::SetTrue)
                .help(help_text),
        )
    }
}

use crate::shared::command_line::cli_builder::CommandExt;
use clap::{Arg, Command};

pub trait CommandCommonExt {
    fn add_eh_base_shared_args(self) -> Self;
    fn add_eh_reader_args(self) -> Self;
    fn add_eh_export_args(self) -> Self;
}

impl CommandCommonExt for Command {
    fn add_eh_base_shared_args(self) -> Self {
        self.arg(
            Arg::new("base-data-folder")
                .short('b')
                .long("base-data-folder")
                .value_name("PATH")
                .help("Base folder for all data storage (default: .eh-read-data)"),
        )
        .arg(
            Arg::new("database-path")
                .short('d')
                .long("database-path")
                .value_name("PATH")
                .help("Relative path within base folder for database (default: db)"),
        )
        .arg(
            Arg::new("dump-filter")
                .long("dump-filter")
                .value_name("KEYWORDS")
                .help("Comma-separated keywords to filter messages for export")
                .value_delimiter(','),
        )
        .arg(
            Arg::new("feedback-interval")
                .long("feedback-interval")
                .value_name("SECONDS")
                .help("Show progress feedback every N seconds (default: 1)")
                .value_parser(clap::value_parser!(u64))
                .default_value("1"),
        )
        .arg(
            Arg::new("ignore-checkpoint")
                .short('i')
                .long("ignore-checkpoint")
                .action(clap::ArgAction::SetTrue)
                .help("Process all messages, ignoring stored checkpoints"),
        )
    }

    fn add_eh_reader_args(self) -> Self {
        self.preset_arg_connection_string("EventHub connection string")
            .arg(
                Arg::new("entity-path")
                    .short('e')
                    .long("entity-path")
                    .value_name("STRING")
                    .help("EventHub entity path (name)"),
            )
            .arg(
                Arg::new("consumer-group")
                    .short('g')
                    .long("consumer-group")
                    .value_name("STRING")
                    .help("Consumer group name (default: $Default)"),
            )
            .arg(
                Arg::new("partition-id")
                    .short('p')
                    .long("partition-id")
                    .value_name("NUMBER")
                    .help("Partition ID to read from (-1 for all partitions, default: -1)")
                    .value_parser(clap::value_parser!(i32)),
            )
            .arg(
                Arg::new("received-msg-path")
                    .short('r')
                    .long("received-msg-path")
                    .value_name("PATH")
                    .help(
                        "Relative path within base folder for exported messages (default: inbound)",
                    ),
            )
            .arg(
                Arg::new("read-to-file")
                    .short('f')
                    .long("read-to-file")
                    .action(clap::ArgAction::SetTrue)
                    .help("Export messages to files as they are read"),
            )
            .arg(
                Arg::new("dump-content-only")
                    .long("dump-content-only")
                    .action(clap::ArgAction::SetTrue)
                    .help("When exporting to file, save only message content (not metadata)"),
            )
    }

    fn add_eh_export_args(self) -> Self {
        self.arg(
            Arg::new("export-base-data-folder")
                .long("export-base-data-folder")
                .value_name("PATH")
                .help("Base folder for export data (default: .eh-export-data)"),
        )
        .arg(
            Arg::new("export-format")
                .short('f')
                .long("export-format")
                .value_name("FORMAT")
                .help("Export format: txt, csv, json (default: txt)")
                .value_parser(["txt", "csv", "json"]),
        )
        .arg(
            Arg::new("condense-output")
                .long("condense-output")
                .action(clap::ArgAction::SetTrue)
                .help("Condense multiple messages into fewer files (default: false)"),
        )
        .arg(
            Arg::new("include-metadata")
                .long("include-metadata")
                .action(clap::ArgAction::SetTrue)
                .help("Include message metadata in exports (default: true)"),
        )
        .arg(
            Arg::new("export-folder")
                .short('e')
                .long("export-folder")
                .value_name("PATH")
                .help("Folder for exported files (default: exports)"),
        )
        .arg(
            Arg::new("use-local-time")
                .long("use-local-time")
                .action(clap::ArgAction::SetTrue)
                .help("Use local time for timestamps (default: UTC)"),
        )
        .arg(
            Arg::new("export-database-path")
                .short('x')
                .long("export-database-path")
                .value_name("PATH")
                .help(
                    "Relative path within base folder for export tracking database (default: db)",
                ),
        )
    }
}

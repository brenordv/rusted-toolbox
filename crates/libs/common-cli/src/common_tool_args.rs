use crate::app_logger::AppLogger;
use crate::tool_log_level::ToolLogLevel;
use clap::Args;
use common_utils::constants::{CONFIG_UL_ITEM_LEVEL_1, CONFIG_UL_ITEM_LEVEL_2, DASH_LINE};

#[derive(Args, Debug)]
#[command(about, long_about, version)]
pub struct CommonToolArgs {
    /// Shows the header with tool name, version, and runtime options
    #[arg(long = "app-header")]
    pub app_header: bool,

    /// Enables verbose output
    #[arg(long = "verbose")]
    pub verbose: bool,

    /// Sets the log level. Output goes to stderr by default with no channel flag
    /// needed; `disabled` silences everything and overrides `RUST_LOG`, while
    /// every other level yields to `RUST_LOG` when it is set.
    #[arg(short = 'L', long = "log-level", default_value = "warn")]
    pub default_logging_level: ToolLogLevel,

    /// Log to stdout instead of the default stderr
    #[arg(long = "log-to-console")]
    pub log_to_stdout: bool,

    /// If the tool should log to a file
    #[arg(long = "log-to-file")]
    pub log_to_file: bool,

    /// Rotate the log file by day
    #[arg(long = "rotate-log-file-by-day")]
    pub rotate_log_file_by_day: bool,
}

impl CommonToolArgs {
    fn initialize_logging(&self, app_name: &str, force_disable: bool) {
        let app_logger = AppLogger::new(
            app_name,
            self.default_logging_level.clone(),
            self.log_to_stdout,
            self.log_to_file,
            self.rotate_log_file_by_day,
        );

        app_logger.init(force_disable);
    }

    pub fn app_boot_up_headerless(&self, app_name: &str, force_disable_log: bool) {
        self.app_boot_up(app_name, "", false, force_disable_log, None::<fn()>);
    }
    
    pub fn app_boot_up(
        &self,
        app_name: &str,
        app_version: &str,
        uses_verbose_flag: bool,
        force_disable_log: bool,
        tool_header_printer:  Option<impl FnOnce()>) {
        self.initialize_logging(app_name, force_disable_log);

        if !self.app_header {
            return;
        }

        println!("{} ({})", app_name, app_version);
        println!("{}", DASH_LINE);

        println!("{} Basic Runtime Config", CONFIG_UL_ITEM_LEVEL_1);
        if uses_verbose_flag {
            println!("{} Verbose mode: {}", CONFIG_UL_ITEM_LEVEL_2, self.verbose);
        } else {
            println!("{} Verbose mode: <unused>", CONFIG_UL_ITEM_LEVEL_2);
        }
        println!("{} Log level: {}", CONFIG_UL_ITEM_LEVEL_2, self.default_logging_level);
        println!("{} Log to stdout: {}", CONFIG_UL_ITEM_LEVEL_2, self.log_to_stdout);
        println!("{} Log to file: {}", CONFIG_UL_ITEM_LEVEL_2, self.log_to_file);
        println!("{} Rotate log file by day: {}", CONFIG_UL_ITEM_LEVEL_2, self.rotate_log_file_by_day);

        match tool_header_printer {
            Some(printer) => {
                println!("{} Tool Runtime Config", CONFIG_UL_ITEM_LEVEL_1);
                printer();
            },
            None => {}
        }
    }
}
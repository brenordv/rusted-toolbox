use crate::app_logger::AppLogger;
use crate::tool_log_level::ToolLogLevel;
use clap::Args;
use common_utils::constants::{CONFIG_UL_ITEM_LEVEL_1, CONFIG_UL_ITEM_LEVEL_2, DASH_LINE};

/// The CLI flags shared by every tool: logging level and channels, the header
/// toggle, and verbose mode. Flattened into each tool's argument parser with
/// `#[command(flatten)]`.
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
    #[arg(long = "log-level", default_value_t = ToolLogLevel::Warn, ignore_case = true)]
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
    /// Builds the [` AppLogger `] for this tool from the parsed CLI flags without
    /// installing it. Callers that hand the logger's layers to an externally
    /// owned subscriber use this seam before initialization.
    pub fn build_logger(&self, app_name: &str) -> AppLogger {
        AppLogger::new(
            app_name,
            self.default_logging_level.clone(),
            self.log_to_stdout,
            self.log_to_file,
            self.rotate_log_file_by_day,
        )
    }

    fn initialize_logging(&self, app_name: &str, force_disable: bool) {
        self.build_logger(app_name).init(force_disable);
    }

    /// Boots the tool: initializes logging, then prints the standard header and
    /// runtime-config block when `--app-header` is set. `tool_header_printer`, when
    /// provided, appends a tool-specific config section under the shared block.
    // TODO: Note for future-self: Review this. I'm not happy with this. This method, while good because centralizes the
    // boot process of all tools, it's also bad because of the boilerplate code it generates.
    // Also the whole print with CONFIG_UL_ITEM_LEVEL_1/2/3 is bothering me. Should probably have a helper function that does this.
    pub fn app_boot_up(
        &self,
        app_name: &str,
        app_version: &str,
        uses_verbose_flag: bool,
        force_disable_log: bool,
        tool_header_printer: Option<impl FnOnce()>,
    ) {
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
        println!(
            "{} Log level: {}",
            CONFIG_UL_ITEM_LEVEL_2, self.default_logging_level
        );
        println!(
            "{} Log to stdout: {}",
            CONFIG_UL_ITEM_LEVEL_2, self.log_to_stdout
        );
        println!(
            "{} Log to file: {}",
            CONFIG_UL_ITEM_LEVEL_2, self.log_to_file
        );
        println!(
            "{} Rotate log file by day: {}",
            CONFIG_UL_ITEM_LEVEL_2, self.rotate_log_file_by_day
        );

        if let Some(printer) = tool_header_printer {
            println!("{} Tool Runtime Config", CONFIG_UL_ITEM_LEVEL_1);
            printer();
        }

        println!();
    }
}

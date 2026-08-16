use std::ptr::null;
use crate::models::CatConfig;
use clap::Parser;
use common_cli::common_tool_args::CommonToolArgs;

/// Concatenate files and print on the standard output.
///
/// Mimics the behavior of CAT (the Linux tool). Concatenate FILE(s) to standard output.
/// With no FILE, or when FILE is -, read standard input.
#[derive(Parser, Debug)]
#[command(about, long_about, version)]
pub struct CliArgs {
    /// Files to display
    #[arg(num_args = 0..)]
    pub files: Vec<String>,

    /// Equivalent to -vET
    #[arg(short = 'A', long = "show-all", default_value_t = false)]
    pub show_all: bool,

    /// The number of nonempty output lines, overrides -n
    #[arg(short = 'b', long = "number-nonblank", default_value_t = false)]
    pub number_nonblank: bool,

    /// Equivalent to -vE
    #[arg(short = 'e', default_value_t = false)]
    pub e: bool,

    /// Display $ at the end of each line
    #[arg(short = 'E', long = "show-ends", default_value_t = false)]
    pub show_ends: bool,

    /// Show number all output lines
    #[arg(short = 'n', long = "number", default_value_t = false)]
    pub number: bool,

    /// Suppress repeated empty output lines
    #[arg(short = 's', long = "squeeze-blank", default_value_t = false)]
    pub squeeze_blank: bool,

    /// Equivalent to -vT
    #[arg(short = 't', default_value_t = false)]
    pub t: bool,

    /// Display TAB characters as ^I
    #[arg(short = 'T', long = "show-tabs", default_value_t = false)]
    pub show_tabs: bool,

    /// (ignored)
    #[arg(short = 'u', default_value_t = false)]
    pub u: bool,

    /// Use ^ and M- notation, except for LFD and TAB
    #[arg(short = 'v', long = "show-nonprinting", default_value_t = false)]
    pub show_nonprinting: bool,

    #[command(flatten)]
    pub common: CommonToolArgs,
}

pub fn initialize() -> CatConfig {
    let args = CliArgs::parse();

    args.common.app_boot_up_headerless(env!("CARGO_PKG_NAME"), false);

    CatConfig::from_args(&args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_has_no_conflicting_flags() {
        CliArgs::command().debug_assert();
    }
}

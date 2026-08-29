use crate::models::TsArgs;
use clap::Parser;
use common_cli::common_tool_args::CommonToolArgs;
use common_utils::constants::CONFIG_UL_ITEM_LEVEL_2;

/// Convert Unix timestamps to datetime and vice versa.
///
/// This tool receives a Unix timestamp and converts it to a datetime (ISO 8601) or vice versa.
#[derive(Parser, Debug)]
#[command(about, long_about, version)]
struct CliArgs {
    /// Input to process: a Unix timestamp or a datetime string. Only one input is used, and no quotes are needed
    #[arg(num_args = 0.., value_name = "INPUT")]
    pub input: Vec<String>,

    #[command(flatten)]
    pub common: CommonToolArgs,
}

/// Parses command-line arguments and returns the runtime configuration.
///
/// Multiple input tokens are joined with spaces; an empty input means "current time".
pub fn initialize() -> TsArgs {
    let args = CliArgs::parse();

    let config = TsArgs {
        input: args.input.join(" "),
    };

    args.common.app_boot_up(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        false,
        false,
        Some(|| {
            print_header(&config);
        }),
    );

    config
}

/// Prints the tool's runtime configuration, shown under `--app-header`.
fn print_header(args: &TsArgs) {
    let input = if args.input.is_empty() {
        "(Current time)"
    } else {
        &args.input
    };

    println!("{} Input: {}", CONFIG_UL_ITEM_LEVEL_2, input);
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_has_no_conflicting_flags() {
        CliArgs::command().debug_assert();
    }

    #[test]
    fn input_tokens_are_joined_with_spaces() {
        let args = CliArgs::try_parse_from(["ts", "2024-01-01", "12:00:00"]).unwrap();
        assert_eq!(args.input.join(" "), "2024-01-01 12:00:00");
    }

    #[test]
    fn print_header_smoke() {
        print_header(&TsArgs {
            input: String::new(),
        });
        print_header(&TsArgs {
            input: "1700000000".to_string(),
        });
    }
}

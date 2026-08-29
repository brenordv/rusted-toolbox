use crate::models::{JwtConfig, JwtPrint};
use clap::Parser;
use common_cli::common_tool_args::CommonToolArgs;
use common_cli::tool_exit_helpers::exit_error;
use common_utils::constants::CONFIG_UL_ITEM_LEVEL_2;
use tracing::error;

/// Decodes and prints public JWT tokens data.
///
/// Decodes all public info in the JWT tokens, and can either pretty-print, print the csv or json format. Optionally, can copy one of the claims to the clipboard.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about)]
pub struct CliArgs {
    /// Defines how to print the decoded data. Valid values: pretty, csv, JSON.
    #[arg(short='p', long="print", required=false, default_value_t=JwtPrint::Pretty)]
    pub print: JwtPrint,

    /// If set, will copy the value to the clipboard.
    #[arg(
        short = 'c',
        long = "copy-to-clipboard",
        num_args = 1,
        required = false
    )]
    pub copy_to_clipboard: Option<String>,

    /// The token that will be decoded. It does not matter if it has the word Bearer or any line breaks.
    #[arg(num_args = 1.., required = true)]
    pub token: Vec<String>,

    #[command(flatten)]
    pub common: CommonToolArgs,
}

fn print_runtime_info(args: &JwtConfig) {
    println!(
        "{} Token length: {}",
        CONFIG_UL_ITEM_LEVEL_2,
        &args.token.len()
    );

    if let Some(copy_to_clipboard) = &args.claim_to_clipboard {
        println!(
            "{} Claim to Clipboard: {}",
            CONFIG_UL_ITEM_LEVEL_2, copy_to_clipboard
        );
    }

    println!("{} Print format: {}", CONFIG_UL_ITEM_LEVEL_2, &args.print);
}

pub fn initialize() -> JwtConfig {
    let args = CliArgs::parse();

    let config = JwtConfig {
        token: normalize_token(&args.token),
        print: args.print,
        claim_to_clipboard: args.copy_to_clipboard,
    };

    args.common.app_boot_up(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        false,
        false,
        Some(|| {
            print_runtime_info(&config);
        }),
    );

    if config.token.is_empty() {
        error!("Token cannot be empty.");
        exit_error();
    }

    config
}

/// Joins the token fragments, collapses all whitespace, and drops a leading `Bearer`.
fn normalize_token(fragments: &[String]) -> String {
    let collapsed: String = fragments.join(" ").split_whitespace().collect();
    collapsed
        .strip_prefix("Bearer")
        .unwrap_or(&collapsed)
        .to_string()
}

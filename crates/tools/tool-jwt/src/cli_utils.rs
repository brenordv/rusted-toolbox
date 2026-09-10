use crate::models::{JwtConfig, JwtPrint};
use clap::Parser;
use common_cli::common_tool_args::CommonToolArgs;
use common_cli::header_format::format_config_item;
use common_cli::tool_exit_helpers::exit_error;
use tracing::error;

/// Decodes and prints public JWT tokens data.
///
/// Decodes all public info in the JWT tokens, and can either pretty-print, print the csv or json format. Optionally, can copy one of the claims to the clipboard.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about)]
pub struct CliArgs {
    /// Defines how to print the decoded data. Valid values: pretty, csv, JSON.
    ///
    /// The default is the parser-facing value ("pretty"), not the Display
    /// form ("Pretty"): clap runs defaults through the ValueEnum parser,
    /// which only accepts the lowercase names.
    #[arg(
        short = 'p',
        long = "print",
        required = false,
        default_value = "pretty"
    )]
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
    println!("{}", format_config_item("Token length", args.token.len()));

    if let Some(copy_to_clipboard) = &args.claim_to_clipboard {
        println!(
            "{}",
            format_config_item("Claim to Clipboard", copy_to_clipboard)
        );
    }

    println!("{}", format_config_item("Print format", &args.print));
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

/// Joins the token fragments, removes all whitespace, and drops a leading
/// `Bearer` spelled exactly that way; other casings are kept as part of the token.
fn normalize_token(fragments: &[String]) -> String {
    let collapsed: String = fragments.join(" ").split_whitespace().collect();
    collapsed
        .strip_prefix("Bearer")
        .unwrap_or(&collapsed)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::{normalize_token, CliArgs};
    use crate::models::JwtPrint;
    use clap::{CommandFactory, Parser};

    fn fragments(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|part| part.to_string()).collect()
    }

    #[test]
    fn cli_definition_is_consistent() {
        CliArgs::command().debug_assert();
    }

    #[test]
    fn parsing_without_print_flag_uses_the_pretty_default() {
        let args = CliArgs::try_parse_from(["jwt", "some.token.here"]).unwrap();

        assert_eq!(args.print, JwtPrint::Pretty);
    }

    #[test]
    fn normalize_token_passes_plain_token_through() {
        assert_eq!(normalize_token(&fragments(&["abc.def.ghi"])), "abc.def.ghi");
    }

    #[test]
    fn normalize_token_strips_bearer_prefix_fragment() {
        assert_eq!(
            normalize_token(&fragments(&["Bearer", "abc.def.ghi"])),
            "abc.def.ghi"
        );
    }

    #[test]
    fn normalize_token_strips_bearer_prefix_inside_single_fragment() {
        assert_eq!(
            normalize_token(&fragments(&["Bearer abc.def.ghi"])),
            "abc.def.ghi"
        );
    }

    #[test]
    fn normalize_token_keeps_bearer_prefix_with_other_casing() {
        assert_eq!(
            normalize_token(&fragments(&["bearer", "abc.def.ghi"])),
            "bearerabc.def.ghi"
        );
        assert_eq!(
            normalize_token(&fragments(&["BEARER", "abc.def.ghi"])),
            "BEARERabc.def.ghi"
        );
    }

    #[test]
    fn normalize_token_removes_embedded_whitespace_and_newlines() {
        assert_eq!(
            normalize_token(&fragments(&["abc\n.def", "  ghi\t.jkl \r\n"])),
            "abc.defghi.jkl"
        );
    }
}

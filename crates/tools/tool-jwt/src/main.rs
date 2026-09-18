use crate::cli_utils::initialize;
use crate::jwt_app::{
    copy_claim_to_clipboard, decode_jwt_token, print_token_csv, print_token_json,
    print_token_pretty,
};
use crate::models::JwtPrint;
use common_cli::broken_pipe::BrokenPipe;
use common_cli::tool_exit_helpers::{exit_error, exit_success};
use tracing::{debug, error, warn};

mod cli_utils;
mod jwt_app;
mod models;

/// JWT decoding and processing tool.
///
/// Decodes JWT tokens, validates expiration, and outputs claims in various formats.
/// Optionally copies specific claims to clipboard. Exits gracefully on empty tokens,
/// and a consumer closing the output pipe ends the printing quietly (the clipboard
/// copy, which does not depend on stdout, still runs).
///
/// # Exit Codes
/// - 0: Success, empty claims, or a consumer-closed output pipe
/// - 1: Decoding failure or errors
fn main() {
    let args = initialize();

    let token_info = match decode_jwt_token(&args.token) {
        Ok(info) => info,
        Err(e) => {
            error!("{:#}", e);
            exit_error();
        }
    };

    if token_info.claims.is_empty() {
        warn!("Token claims are empty");
        exit_success();
    }

    let mut stdout = std::io::stdout();
    let print_result = match args.print {
        JwtPrint::Pretty => print_token_pretty(
            &token_info.claims,
            &token_info.expiration_status,
            &mut stdout,
        ),
        JwtPrint::Csv => print_token_csv(&token_info.claims, &mut stdout),
        JwtPrint::Json => print_token_json(&token_info.claims, &mut stdout),
    };

    if let Err(e) = print_result {
        if e.is::<BrokenPipe>() {
            debug!("stopping early: output pipe closed by the consumer");
        } else {
            error!("{:#}", e);
            exit_error();
        }
    }

    if let Some(argument_to_clipboard) = args.claim_to_clipboard
        && let Err(e) = copy_claim_to_clipboard(argument_to_clipboard, &token_info.claims)
    {
        error!("{:#}", e);
        exit_error();
    }

    exit_success();
}

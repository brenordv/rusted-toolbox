use crate::cli_utils::initialize;
use crate::jwt_app::{
    copy_claim_to_clipboard, decode_jwt_token, print_token_csv, print_token_json,
    print_token_pretty,
};
use crate::models::JwtPrint;
use common_cli::tool_exit_helpers::{exit_error, exit_success};
use tracing::error;

mod cli_utils;
mod jwt_app;
mod models;

/// JWT decoding and processing tool.
///
/// Decodes JWT tokens, validates expiration, and outputs claims in various formats.
/// Optionally copies specific claims to clipboard. Exits gracefully on empty tokens.
///
/// # Exit Codes
/// - 0: Success or empty claims
/// - 1: Decoding failure or errors
fn main() {
    let args = initialize();

    let token_info = match decode_jwt_token(&args.token) {
        Ok(info) => info,
        Err(e) => {
            error!("Error decoding token: {}", e);
            exit_error();
            unreachable!();
        }
    };

    if token_info.claims.is_empty() {
        error!("Token claims are empty");
        exit_success();
    }

    match args.print {
        JwtPrint::Pretty => print_token_pretty(&token_info.claims, &token_info.expiration_status),
        JwtPrint::Csv => print_token_csv(&token_info.claims),
        JwtPrint::Json => print_token_json(&token_info.claims),
    }

    if let Some(argument_to_clipboard) = args.claim_to_clipboard {
        copy_claim_to_clipboard(argument_to_clipboard, &token_info.claims);
    }

    exit_success();
}

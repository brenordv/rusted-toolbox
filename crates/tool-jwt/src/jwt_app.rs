use crate::models::{Claims, ExpirationStatus, TokenInfo};
use anyhow::{anyhow, Result};
use colored::Colorize;
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde_json::{Map, Value};
use shared::utils::copy_string_to_clipboard::copy_to_clipboard;
use std::borrow::Cow;
use std::process;

/// Decodes JWT token without signature verification.
///
/// Extracts claims and expiration status from JWT payload.
/// Disables all security validations for public data extraction.
///
/// # Errors
/// Returns error if JWT structure is invalid or decoding fails
pub fn decode_jwt_token(token: &str) -> Result<TokenInfo> {
    // Create validation that doesn't verify the signature
    let mut validation = Validation::new(Algorithm::HS256);
    validation.insecure_disable_signature_validation();
    validation.validate_exp = false;
    validation.validate_nbf = false;
    validation.validate_aud = false;
    validation.required_spec_claims.clear();

    // Set all possible algorithms since we're not verifying signatures anyway
    validation.algorithms = vec![
        Algorithm::HS256,
        Algorithm::HS384,
        Algorithm::HS512,
        Algorithm::RS256,
        Algorithm::RS384,
        Algorithm::RS512,
        Algorithm::PS256,
        Algorithm::PS384,
        Algorithm::PS512,
        Algorithm::ES256,
        Algorithm::ES384,
        Algorithm::EdDSA,
    ];

    // Use a fake key since we're not verifying signatures
    let key = DecodingKey::from_secret("no-key-just-public-data".as_ref());

    match decode::<Claims>(token, &key, &validation) {
        Ok(token_data) => {
            let expiration_status = token_data.claims.get_expiration_status();

            Ok(TokenInfo {
                claims: token_data.claims.extra,
                expiration_status,
            })
        }
        Err(e) => Err(anyhow!("Error decoding token: {}", e)),
    }
}

/// Prints JWT claims as CSV format.
///
/// Outputs headers and values as CSV rows with proper escaping.
/// Sorts keys alphabetically for consistent output.
pub fn print_token_csv(claims: &Map<String, Value>) {
    // Exit early if there are no claims
    if claims.is_empty() {
        println!("No claims found");
        return;
    }

    // Some values might need to be escaped for CSV
    let escape = |s: &str| {
        if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
            let doubled = s.replace('"', "\"\"");
            format!("\"{}\"", doubled)
        } else {
            s.to_owned()
        }
    };

    let mut keys: Vec<&String> = claims.keys().collect();

    // Sort keys for deterministic column order
    keys.sort();

    // Printing the headers
    let header = keys.iter().map(|k| escape(k)).collect::<Vec<_>>().join(",");
    println!("{}", header);

    // Printing the values/rows
    let row = keys
        .iter()
        .map(|k| {
            let v = &claims[*k];
            // If it's a JSON string, drop the surrounding quotes; otherwise serialize
            let s = match v {
                Value::String(s) => s.clone(),
                _ => v.to_string(),
            };
            escape(&s)
        })
        .collect::<Vec<_>>()
        .join(",");
    println!("{}", row);
}

/// Prints JWT claims as formatted JSON.
///
/// Outputs pretty-printed JSON to stdout.
/// Exits with code 1 if JSON serialization fails.
pub fn print_token_json(claims: &Map<String, Value>) {
    match serde_json::to_string_pretty(&claims) {
        Ok(json_output) => println!("{}", json_output),
        Err(e) => {
            eprintln!("Error formatting JSON: {}", e);
            process::exit(1);
        }
    }
}

/// Copies specific claim value to clipboard.
///
/// Searches for claim key in token and copies its string value.
/// Exits with error if claim not found or clipboard operation fails.
pub fn copy_claim_to_clipboard(argument_to_copy: String, claims: &Map<String, Value>) {
    let mut value: &Value = &Value::Null;

    for (key, claim_value) in claims {
        if key.to_lowercase() != argument_to_copy.to_lowercase().trim() {
            continue;
        }
        value = claim_value;
    }

    if value == &Value::Null {
        eprintln!("Claim not found: {}", argument_to_copy);
        return;
    }

    let text_to_copy: Cow<'_, str> = match value {
        // Added this treatment to avoid strings being copied to the clipboard with quotes.
        Value::String(s) => Cow::Borrowed(s.as_str()),
        _ => Cow::Owned(value.to_string()),
    };

    match copy_to_clipboard(text_to_copy.as_ref()) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("📋 Error copying to clipboard: {}", e);
            process::exit(1);
        }
    };
}

/// Prints JWT claims in human-readable format.
///
/// Shows expiration status with colors and lists all claims with formatted values.
pub fn print_token_pretty(claims: &Map<String, Value>, expiration_status: &ExpirationStatus) {
    println!("{}", expiration_status.format_colored());
    println!("----{}Claims:{}------------", "".bold(), "".normal());

    for (key, value) in claims {
        println!("{}: {}", key, format_claim_value(value));
    }
}

/// Formats JSON value for display.
///
/// Handles arrays by joining elements with commas.
/// Removes surrounding quotes from non-string values.
fn format_claim_value(value: &Value) -> String {
    match value {
        Value::Array(arr) => arr
            .iter()
            .map(|v| match v {
                Value::String(s) => s.clone(),
                _ => v.to_string().trim_matches('"').to_string(),
            })
            .collect::<Vec<_>>()
            .join(","),
        Value::String(s) => s.clone(),
        _ => value.to_string().trim_matches('"').to_string(),
    }
}

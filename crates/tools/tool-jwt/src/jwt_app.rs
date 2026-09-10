use crate::models::{Claims, ExpirationStatus, TokenInfo};
use anyhow::{anyhow, Result};
use colored::Colorize;
use common_cli::tool_exit_helpers::exit_error;
use common_utils_ext::copy_string_to_clipboard::copy_to_clipboard;
use jsonwebtoken::dangerous::insecure_decode;
use serde_json::{Map, Value};
use std::process;
use tracing::error;

/// Decodes JWT token without signature verification.
///
/// Extracts claims and expiration status from JWT payload.
/// Disables all security validations for public data extraction.
///
/// # Errors
/// Returns error if JWT structure is invalid or decoding fails
pub fn decode_jwt_token(token: &str) -> Result<TokenInfo> {
    match insecure_decode::<Claims>(token) {
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
/// Prints "No claims found" when there are no claims.
pub fn print_token_csv(claims: &Map<String, Value>) {
    // Exit early if there are no claims
    if claims.is_empty() {
        println!("No claims found");
        return;
    }

    println!("{}", format_csv(claims));
}

/// Formats JWT claims as a two-line CSV document: a header row and a value row.
///
/// Sorts keys alphabetically for deterministic column order. Fields containing
/// commas, double quotes, or line breaks are quoted, with embedded quotes doubled.
/// String values are rendered without their JSON quotes; other values serialize
/// as JSON.
fn format_csv(claims: &Map<String, Value>) -> String {
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

    let header = keys.iter().map(|k| escape(k)).collect::<Vec<_>>().join(",");

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

    format!("{}\n{}", header, row)
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
/// Resolves the claim through [`resolve_claim_value`] and copies the result.
/// Reports on stderr when the claim is not found; exits with error if the
/// clipboard operation fails.
pub fn copy_claim_to_clipboard(argument_to_copy: String, claims: &Map<String, Value>) {
    let Some(text_to_copy) = resolve_claim_value(&argument_to_copy, claims) else {
        eprintln!("Claim not found: {}", argument_to_copy);
        return;
    };

    match copy_to_clipboard(&text_to_copy) {
        Ok(_) => {}
        Err(e) => {
            error!("Error copying to clipboard: {}", e);
            exit_error();
        }
    };
}

/// Resolves the text a claim would place on the clipboard.
///
/// Matches the claim name case-insensitively, ignoring surrounding whitespace
/// in the requested name. String values are returned without their JSON quotes;
/// other values serialize as JSON. Returns None when the claim is missing or
/// its value is JSON null.
fn resolve_claim_value(claim_name: &str, claims: &Map<String, Value>) -> Option<String> {
    let wanted = claim_name.to_lowercase();
    let wanted = wanted.trim();

    let mut value: &Value = &Value::Null;

    for (key, claim_value) in claims {
        if key.to_lowercase() != wanted {
            continue;
        }
        value = claim_value;
    }

    if value == &Value::Null {
        return None;
    }

    Some(match value {
        Value::String(s) => s.clone(),
        _ => value.to_string(),
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SAMPLE_TOKEN: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";

    #[test]
    fn decode_jwt_token_extracts_public_claims() {
        let info = decode_jwt_token(SAMPLE_TOKEN).unwrap();

        assert_eq!(info.claims["sub"], json!("1234567890"));
        assert_eq!(info.claims["name"], json!("John Doe"));
        assert_eq!(info.claims["iat"], json!(1516239022));
        assert!(matches!(
            info.expiration_status,
            ExpirationStatus::NoExpiration
        ));
    }

    #[test]
    fn decode_jwt_token_rejects_malformed_token() {
        assert!(decode_jwt_token("not.a.jwt").is_err());
        assert!(decode_jwt_token("garbage").is_err());
    }

    #[test]
    fn format_claim_value_joins_array_elements() {
        assert_eq!(
            format_claim_value(&json!(["read", "write", "admin"])),
            "read,write,admin"
        );
    }

    #[test]
    fn format_claim_value_strips_surrounding_string_quotes() {
        assert_eq!(format_claim_value(&json!("alice")), "alice");
    }

    #[test]
    fn format_claim_value_renders_scalars_without_quotes() {
        assert_eq!(format_claim_value(&json!(42)), "42");
        assert_eq!(format_claim_value(&json!(true)), "true");
    }

    #[test]
    fn format_claim_value_array_mixes_strings_and_scalars() {
        assert_eq!(format_claim_value(&json!(["a", 1, false])), "a,1,false");
    }

    fn claims_from(pairs: &[(&str, Value)]) -> Map<String, Value> {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.clone()))
            .collect()
    }

    #[test]
    fn format_csv_sorts_keys_alphabetically() {
        let claims = claims_from(&[
            ("sub", json!("1234567890")),
            ("aud", json!("app")),
            ("iat", json!(1516239022)),
        ]);

        assert_eq!(
            format_csv(&claims),
            "aud,iat,sub\napp,1516239022,1234567890"
        );
    }

    #[test]
    fn format_csv_quotes_values_containing_commas() {
        let claims = claims_from(&[("scope", json!("read,write"))]);

        assert_eq!(format_csv(&claims), "scope\n\"read,write\"");
    }

    #[test]
    fn format_csv_doubles_embedded_double_quotes() {
        let claims = claims_from(&[("note", json!("say \"hi\""))]);

        assert_eq!(format_csv(&claims), "note\n\"say \"\"hi\"\"\"");
    }

    #[test]
    fn format_csv_quotes_values_containing_newlines() {
        let claims = claims_from(&[("text", json!("line1\nline2"))]);

        assert_eq!(format_csv(&claims), "text\n\"line1\nline2\"");
    }

    #[test]
    fn resolve_claim_value_matches_case_insensitively() {
        let claims = claims_from(&[("Sub", json!("1234567890"))]);

        assert_eq!(
            resolve_claim_value("SUB", &claims),
            Some("1234567890".to_string())
        );
        assert_eq!(
            resolve_claim_value("sub", &claims),
            Some("1234567890".to_string())
        );
    }

    #[test]
    fn resolve_claim_value_trims_the_requested_name() {
        let claims = claims_from(&[("sub", json!("1234567890"))]);

        assert_eq!(
            resolve_claim_value(" Sub ", &claims),
            Some("1234567890".to_string())
        );
    }

    #[test]
    fn resolve_claim_value_returns_strings_without_json_quotes() {
        let claims = claims_from(&[("name", json!("John Doe"))]);

        assert_eq!(
            resolve_claim_value("name", &claims),
            Some("John Doe".to_string())
        );
    }

    #[test]
    fn resolve_claim_value_serializes_non_strings_as_json() {
        let claims = claims_from(&[
            ("iat", json!(1516239022)),
            ("roles", json!(["admin", "user"])),
            ("nested", json!({"a": 1})),
        ]);

        assert_eq!(
            resolve_claim_value("iat", &claims),
            Some("1516239022".to_string())
        );
        assert_eq!(
            resolve_claim_value("roles", &claims),
            Some("[\"admin\",\"user\"]".to_string())
        );
        assert_eq!(
            resolve_claim_value("nested", &claims),
            Some("{\"a\":1}".to_string())
        );
    }

    #[test]
    fn resolve_claim_value_returns_none_when_claim_missing() {
        let claims = claims_from(&[("sub", json!("1234567890"))]);

        assert_eq!(resolve_claim_value("aud", &claims), None);
    }

    #[test]
    fn resolve_claim_value_treats_null_claim_as_missing() {
        let claims = claims_from(&[("nickname", Value::Null)]);

        assert_eq!(resolve_claim_value("nickname", &claims), None);
    }
}

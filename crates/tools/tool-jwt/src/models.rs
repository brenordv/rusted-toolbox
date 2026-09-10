use chrono::{DateTime, Duration, Utc};
use clap::ValueEnum;
use colored::Colorize;
use common_utils::string_utils::format_duration_to_string;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fmt;

#[derive(ValueEnum, Debug, PartialEq, Clone)]
pub enum JwtPrint {
    Pretty,
    Csv,
    Json,
}

impl fmt::Display for JwtPrint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JwtPrint::Pretty => write!(f, "Pretty"),
            JwtPrint::Csv => write!(f, "CSV"),
            JwtPrint::Json => write!(f, "JSON"),
        }
    }
}

pub struct JwtConfig {
    pub token: String,
    pub print: JwtPrint,
    pub claim_to_clipboard: Option<String>,
}

#[derive(Debug)]
pub enum ExpirationStatus {
    Valid { expires_in: Duration },
    Expired { expired_ago: Duration },
    NoExpiration,
    InvalidExpiration,
}

impl ExpirationStatus {
    /// Formats expiration status as colored string.
    ///
    /// Shows "Valid" in green, "Expired" in red, "No expiration claim" in yellow,
    /// or "Invalid expiration" in red when the exp claim is out of range or not
    /// numeric. Includes remaining time or elapsed time since expiration.
    pub fn format_colored(&self) -> String {
        match self {
            ExpirationStatus::Valid { expires_in } => {
                format!(
                    "{} - expires in {}",
                    "Valid".bright_green().bold(),
                    format_duration_to_string(*expires_in)
                )
            }
            ExpirationStatus::Expired { expired_ago } => {
                format!(
                    "{} - expired {} ago",
                    "Expired".bright_red().bold(),
                    format_duration_to_string(*expired_ago)
                )
            }
            ExpirationStatus::NoExpiration => {
                "No expiration claim".bright_yellow().bold().to_string()
            }
            ExpirationStatus::InvalidExpiration => {
                format!(
                    "{} (exp out of range or not numeric)",
                    "Invalid expiration".bright_red().bold()
                )
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Claims {
    /// Determines JWT expiration status based on 'exp' claim.
    ///
    /// Returns Valid with remaining time, Expired with elapsed time, NoExpiration
    /// when the claim is missing, or InvalidExpiration when the claim is present
    /// but not a number or falls outside the range chrono can represent. A
    /// fractional NumericDate (allowed by RFC 7519) is truncated toward zero.
    /// Uses current UTC time for comparison.
    pub fn get_expiration_status(&self) -> ExpirationStatus {
        let Some(exp_value) = self.extra.get("exp") else {
            return ExpirationStatus::NoExpiration;
        };

        let exp_timestamp = if let Some(seconds) = exp_value.as_i64() {
            seconds
        } else if let Some(seconds) = exp_value.as_f64() {
            // The saturating float-to-int cast sends out-of-range magnitudes to
            // i64::MIN/MAX, which from_timestamp rejects as InvalidExpiration.
            seconds.trunc() as i64
        } else {
            return ExpirationStatus::InvalidExpiration;
        };

        match DateTime::from_timestamp(exp_timestamp, 0) {
            Some(exp_datetime) => {
                let now = Utc::now();
                let time_diff = exp_datetime - now;

                if time_diff > Duration::zero() {
                    ExpirationStatus::Valid {
                        expires_in: time_diff,
                    }
                } else {
                    ExpirationStatus::Expired {
                        expired_ago: -time_diff,
                    }
                }
            }
            None => ExpirationStatus::InvalidExpiration,
        }
    }
}

pub struct TokenInfo {
    pub claims: Map<String, Value>,
    pub expiration_status: ExpirationStatus,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn claims_with_exp(exp: Option<Value>) -> Claims {
        let mut extra = Map::new();
        if let Some(value) = exp {
            extra.insert("exp".to_string(), value);
        }
        Claims { extra }
    }

    #[test]
    fn get_expiration_status_valid_for_future_exp() {
        let claims = claims_with_exp(Some(json!(4_102_444_800i64)));

        assert!(matches!(
            claims.get_expiration_status(),
            ExpirationStatus::Valid { .. }
        ));
    }

    #[test]
    fn get_expiration_status_expired_for_past_exp() {
        let claims = claims_with_exp(Some(json!(946_684_800i64)));

        assert!(matches!(
            claims.get_expiration_status(),
            ExpirationStatus::Expired { .. }
        ));
    }

    #[test]
    fn get_expiration_status_none_when_exp_absent() {
        let claims = claims_with_exp(None);

        assert!(matches!(
            claims.get_expiration_status(),
            ExpirationStatus::NoExpiration
        ));
    }

    #[test]
    fn get_expiration_status_invalid_when_exp_is_a_string() {
        let claims = claims_with_exp(Some(json!("not-a-number")));

        assert!(matches!(
            claims.get_expiration_status(),
            ExpirationStatus::InvalidExpiration
        ));
    }

    #[test]
    fn get_expiration_status_valid_for_fractional_future_exp() {
        let claims = claims_with_exp(Some(json!(4_102_444_800.5f64)));

        assert!(matches!(
            claims.get_expiration_status(),
            ExpirationStatus::Valid { .. }
        ));
    }

    #[test]
    fn get_expiration_status_expired_for_fractional_past_exp() {
        let claims = claims_with_exp(Some(json!(946_684_800.25f64)));

        assert!(matches!(
            claims.get_expiration_status(),
            ExpirationStatus::Expired { .. }
        ));
    }

    #[test]
    fn get_expiration_status_invalid_for_non_numeric_exp_types() {
        for value in [json!(true), json!([1]), json!({"at": 1}), json!(null)] {
            let claims = claims_with_exp(Some(value));

            assert!(matches!(
                claims.get_expiration_status(),
                ExpirationStatus::InvalidExpiration
            ));
        }
    }

    #[test]
    fn get_expiration_status_invalid_for_huge_float_exp() {
        let claims = claims_with_exp(Some(json!(1e300)));

        assert!(matches!(
            claims.get_expiration_status(),
            ExpirationStatus::InvalidExpiration
        ));
    }

    #[test]
    fn get_expiration_status_invalid_when_exp_above_timestamp_range() {
        let claims = claims_with_exp(Some(json!(i64::MAX)));

        assert!(matches!(
            claims.get_expiration_status(),
            ExpirationStatus::InvalidExpiration
        ));
    }

    #[test]
    fn get_expiration_status_invalid_when_exp_below_timestamp_range() {
        let claims = claims_with_exp(Some(json!(i64::MIN)));

        assert!(matches!(
            claims.get_expiration_status(),
            ExpirationStatus::InvalidExpiration
        ));
    }

    #[test]
    fn format_colored_reports_invalid_expiration() {
        let rendered = ExpirationStatus::InvalidExpiration.format_colored();

        assert!(rendered.contains("Invalid expiration"));
        assert!(rendered.contains("(exp out of range or not numeric)"));
    }
}

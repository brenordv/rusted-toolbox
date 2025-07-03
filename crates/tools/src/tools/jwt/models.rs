use crate::shared::utils::format_duration_to_string::format_duration_to_string;
use chrono::{DateTime, Duration, Utc};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub enum JwtPrint {
    Pretty,
    Csv,
    Json,
}

impl JwtPrint {
    pub fn from_str(s: &str) -> Result<JwtPrint, String> {
        match s.trim().to_lowercase().as_str() {
            "pretty" => Ok(JwtPrint::Pretty),
            "csv" => Ok(JwtPrint::Csv),
            "json" => Ok(JwtPrint::Json),
            _ => Err(format!("Invalid print format: {}", s)),
        }
    }
}

pub struct JwtArgs {
    pub token: String,
    pub print: JwtPrint,
    pub claim_to_clipboard: Option<String>,
}

#[derive(Debug)]
pub enum ExpirationStatus {
    Valid { expires_in: Duration },
    Expired { expired_ago: Duration },
    NoExpiration,
}

impl ExpirationStatus {
    /// Formats expiration status as colored string.
    ///
    /// Shows "Valid" in green, "Expired" in red, or "No expiration claim" in yellow.
    /// Includes remaining time or elapsed time since expiration.
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
    /// Returns Valid with remaining time, Expired with elapsed time, or NoExpiration.
    /// Uses current UTC time for comparison.
    pub fn get_expiration_status(&self) -> ExpirationStatus {
        if let Some(exp_value) = self.extra.get("exp") {
            if let Some(exp_timestamp) = exp_value.as_i64() {
                let exp_datetime =
                    DateTime::from_timestamp(exp_timestamp, 0).unwrap_or_else(Utc::now);
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
            } else {
                ExpirationStatus::NoExpiration
            }
        } else {
            ExpirationStatus::NoExpiration
        }
    }
}

pub struct TokenInfo {
    pub claims: Map<String, Value>,
    pub is_valid: bool,
    pub expiration_status: ExpirationStatus,
}

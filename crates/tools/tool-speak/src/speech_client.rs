//! The HTTP side of speak: one request to the speech server, one answer back.

use crate::config::SpeakConfig;
use anyhow::{Context, Result, anyhow, bail};
use common_utils::constants::SIZE_16KB;
use common_utils::string_utils::error_preview;
use reqwest::header::{CONTENT_TYPE, HeaderMap};
use reqwest::redirect::Policy;
use reqwest::{Client, Response};
use serde_json::Value;
use std::time::{Duration, Instant};
use tracing::info;

pub(crate) const BYTES_PER_MB: usize = 1024 * 1024;

/// The bounds on one request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// How long connecting may take.
    pub connect_timeout: Duration,
    /// How long the whole request may take, the answer's body included.
    pub total_timeout: Duration,
    /// The largest audio answer accepted.
    pub max_audio_bytes: usize,
    /// How much of an error answer's body is read.
    pub max_error_bytes: usize,
}

impl Default for Limits {
    /// 10 s to connect, 5 minutes in all, 100 MB of audio (about half an hour of 24 kHz mono
    /// WAV), 16 KB of an error body.
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(10),
            total_timeout: Duration::from_secs(300),
            max_audio_bytes: 100 * BYTES_PER_MB,
            max_error_bytes: SIZE_16KB,
        }
    }
}

/// The audio a speech server answered with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Speech {
    pub bytes: Vec<u8>,
    /// The answer's `Content-Type`, when it sent one: on one line, cut after 140 characters.
    pub content_type: Option<String>,
}

/// Sends text to a speech server. Redirects are not followed: one would send the text to a
/// host the user did not choose.
#[derive(Debug, Clone)]
pub struct SpeechClient {
    client: Client,
    limits: Limits,
}

impl SpeechClient {
    /// A client with the default [`Limits`].
    ///
    /// # Errors
    /// When the HTTP client cannot be built.
    pub fn new() -> Result<Self> {
        Self::with_limits(Limits::default())
    }

    /// A client bounded by `limits`.
    ///
    /// # Errors
    /// When the HTTP client cannot be built.
    pub fn with_limits(limits: Limits) -> Result<Self> {
        let client = Client::builder()
            .connect_timeout(limits.connect_timeout)
            .timeout(limits.total_timeout)
            .redirect(Policy::none())
            .build()
            .context("Failed to build the HTTP client")?;
        Ok(Self { client, limits })
    }

    /// Sends `config`'s payload with `input` set to `text` to `config`'s server, as JSON, and
    /// returns the audio it answers with.
    ///
    /// # Errors
    /// When `config` does not validate, the server cannot be reached or does not answer in
    /// time (the error names the host, the port, and the timeout that fired), it answers
    /// with a redirect or an error status (the error carries the status and
    /// [`server_detail`]), or its answer is larger than the audio limit. No error text holds
    /// the URL or `text`.
    pub async fn synthesize(&self, config: &SpeakConfig, text: &str) -> Result<Speech> {
        let endpoint = config.endpoint()?;
        let method = config.method()?;
        let server = Server {
            host: endpoint.host_str().unwrap_or_default().to_owned(),
            port: endpoint.port_or_known_default().unwrap_or_default(),
        };
        info!(
            host = %server.host,
            port = server.port,
            method = %method,
            model = config.model().unwrap_or_default(),
            voice = config.voice().unwrap_or_default(),
            input_chars = text.chars().count(),
            "request sent"
        );
        let started = Instant::now();
        let response = self
            .client
            .request(method, endpoint)
            .json(&config.request_body(text))
            .send()
            .await
            .map_err(|error| self.request_error(error, &server))?;

        let status = response.status();
        if status.is_redirection() {
            bail!(
                "The speech server at {server} answered with a redirect ({status}), which speak does not follow"
            );
        }
        if !status.is_success() {
            let (body, _) = self
                .read_capped(response, self.limits.max_error_bytes, &server)
                .await?;
            let detail = server_detail(&body);
            if detail.is_empty() {
                bail!("The speech server at {server} answered {status}");
            }
            bail!("The speech server at {server} answered {status}: {detail}");
        }

        let content_type = content_type(response.headers());
        let (bytes, complete) = self
            .read_capped(response, self.limits.max_audio_bytes, &server)
            .await?;
        if !complete {
            bail!(
                "The speech server at {server} answered with more than the {} MB limit",
                self.limits.max_audio_bytes / BYTES_PER_MB
            );
        }
        info!(
            status = status.as_u16(),
            content_type = content_type.as_deref().unwrap_or_default(),
            bytes = bytes.len(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            "response"
        );
        Ok(Speech {
            bytes,
            content_type,
        })
    }

    /// Reads `response`'s body up to `cap` bytes and stops there. The flag is false when the
    /// body held more than `cap`.
    async fn read_capped(
        &self,
        mut response: Response,
        cap: usize,
        server: &Server,
    ) -> Result<(Vec<u8>, bool)> {
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| self.request_error(error, server))?
        {
            let room = cap - body.len();
            if chunk.len() > room {
                body.extend_from_slice(&chunk[..room]);
                return Ok((body, false));
            }
            body.extend_from_slice(&chunk);
        }
        Ok((body, true))
    }

    /// `error` without its URL, under a line naming the server and, for a timeout, which one
    /// fired.
    fn request_error(&self, error: reqwest::Error, server: &Server) -> anyhow::Error {
        let line = if error.is_timeout() && error.is_connect() {
            format!(
                "Could not connect to the speech server at {server} within {} s",
                self.limits.connect_timeout.as_secs()
            )
        } else if error.is_timeout() {
            format!(
                "The speech server at {server} did not finish answering within {} s",
                self.limits.total_timeout.as_secs()
            )
        } else if error.is_connect() {
            format!("Could not connect to the speech server at {server}")
        } else {
            format!("The request to the speech server at {server} failed")
        };
        anyhow!(error.without_url()).context(line)
    }
}

/// The server a request goes to, as logs and error text name it.
struct Server {
    host: String,
    port: u16,
}

impl std::fmt::Display for Server {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.host, self.port)
    }
}

/// The `Content-Type` header as text that is safe to show: on one line, cut after 140
/// characters. The server controls it, like the body of an error answer.
fn content_type(headers: &HeaderMap) -> Option<String> {
    headers
        .get(CONTENT_TYPE)
        .map(|value| error_preview(&String::from_utf8_lossy(value.as_bytes())).shown)
}

/// What an error answer's body says, on one line cut after 140 characters with "…", with
/// control and bidirectional characters as spaces.
///
/// From a JSON body only `detail` is used: a text as it is, or for a list each item's `loc`
/// and `msg`. An item's `input` is never read, since a validation error can repeat the whole
/// request there, the spoken text included. Any other JSON, and a body that starts like JSON
/// but does not parse (a cut one, say), gives an empty text. A body that is not JSON is used
/// as text.
pub fn server_detail(body: &[u8]) -> String {
    let text = String::from_utf8_lossy(body);
    let detail = match serde_json::from_str::<Value>(&text) {
        Ok(json) => json_detail(&json),
        Err(_) if text.trim_start().starts_with(['{', '[']) => String::new(),
        Err(_) => text.trim().to_owned(),
    };
    error_preview(&detail).shown
}

fn json_detail(json: &Value) -> String {
    match json.get("detail") {
        Some(Value::String(detail)) => detail.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(item_detail)
            .collect::<Vec<_>>()
            .join("; "),
        _ => String::new(),
    }
}

/// One validation item as `loc: msg`, or whichever of the two it has.
fn item_detail(item: &Value) -> Option<String> {
    let loc = item
        .get("loc")
        .and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| match part {
                    Value::String(name) => Some(name.clone()),
                    Value::Number(index) => Some(index.to_string()),
                    Value::Null | Value::Bool(_) | Value::Array(_) | Value::Object(_) => None,
                })
                .collect::<Vec<_>>()
                .join(".")
        })
        .filter(|loc| !loc.is_empty());
    let msg = item.get("msg").and_then(Value::as_str);
    match (loc, msg) {
        (Some(loc), Some(msg)) => Some(format!("{loc}: {msg}")),
        (Some(loc), None) => Some(loc),
        (None, Some(msg)) => Some(msg.to_owned()),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests;

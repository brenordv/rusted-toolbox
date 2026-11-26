//! The HTTP plumbing every source shares: the client, one bounded request, the failure words,
//! the URL encoder, and the gate that stops a source.

use crate::engine::Clock;
use crate::rate_limits::block_until;
use crate::report::display_time;
use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use reqwest::header::{HeaderMap, RETRY_AFTER};
use reqwest::redirect::Policy;
use reqwest::{Client, RequestBuilder, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::error::Category;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};
use tracing::debug;
use url::Url;

const BYTES_PER_MIB: usize = 1024 * 1024;

/// The bounds on one request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HttpLimits {
    /// How long connecting may take.
    pub connect_timeout: Duration,
    /// How long the whole request may take, the answer's body included.
    pub request_timeout: Duration,
    /// The largest answer body read.
    pub max_body_bytes: usize,
}

impl Default for HttpLimits {
    /// 10 s to connect, 30 s in all, 16 MiB of body.
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(30),
            max_body_bytes: 16 * BYTES_PER_MIB,
        }
    }
}

/// A client bounded by `limits` that sends `user_agent` and `headers` on every request and
/// follows no redirect: one would send the package list to a host nobody chose.
///
/// # Errors
/// When the client cannot be built, as with a `user_agent` that is not a valid header value.
pub fn build_client(limits: &HttpLimits, user_agent: &str, headers: HeaderMap) -> Result<Client> {
    Client::builder()
        .connect_timeout(limits.connect_timeout)
        .timeout(limits.request_timeout)
        .redirect(Policy::none())
        .user_agent(user_agent)
        .default_headers(headers)
        .build()
        .context("Failed to build the HTTP client")
}

/// Why a source gave no answer for a request.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceFailure {
    /// No connection: DNS, refused, TLS. `detail` is the error chain without its URL.
    Unreachable { detail: String },
    /// The connect timeout or the whole-request timeout fired; reqwest's `is_timeout()` does
    /// not say which.
    TimedOut,
    /// The connection broke before the answer was whole.
    Interrupted,
    /// Blocked by the service's rate limit until `until`.
    RateLimited { until: DateTime<Utc> },
    /// An answer with a status this source does not handle.
    Status { code: u16 },
    /// An answer that was too large, did not parse, or did not fit what was asked. `detail`
    /// holds fixed words and positions only, never text from the answer.
    BadAnswer { detail: String },
}

impl SourceFailure {
    /// The failure as the rest of a sentence whose subject is the source: "OSV.dev did not
    /// answer in time".
    pub fn describe(&self) -> String {
        match self {
            SourceFailure::Unreachable { detail } => format!("could not be reached: {detail}"),
            SourceFailure::TimedOut => "did not answer in time".to_owned(),
            SourceFailure::Interrupted => {
                "lost the connection before the answer was complete".to_owned()
            }
            SourceFailure::RateLimited { until } => {
                format!("is rate limited until {}", display_time(*until))
            }
            SourceFailure::Status { code } => format!("answered {code}"),
            SourceFailure::BadAnswer { detail } => {
                format!("sent an answer the tool could not read: {detail}")
            }
        }
    }

    /// The failure's kind as one word, as the report's `kind` field writes it.
    pub fn kind(&self) -> &'static str {
        match self {
            SourceFailure::Unreachable { .. } => "unreachable",
            SourceFailure::TimedOut => "timed_out",
            SourceFailure::Interrupted => "interrupted",
            SourceFailure::RateLimited { .. } => "rate_limited",
            SourceFailure::Status { .. } => "status",
            SourceFailure::BadAnswer { .. } => "bad_answer",
        }
    }

    fn bad_answer(detail: &str) -> Self {
        SourceFailure::BadAnswer {
            detail: detail.to_owned(),
        }
    }
}

/// An answer whose head and body arrived whole.
#[derive(Debug)]
pub(crate) struct Fetched {
    pub status: u16,
    pub meta: ResponseMeta,
    /// The body of a 2xx answer; empty for any other status, whose body is never read.
    pub body: Vec<u8>,
}

/// The answer headers the rate rules read.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ResponseMeta {
    /// `retry-after` when it is a plain non-negative number of seconds.
    pub retry_after_seconds: Option<u64>,
}

impl ResponseMeta {
    fn from_headers(headers: &HeaderMap) -> Self {
        Self {
            retry_after_seconds: header_number(headers, RETRY_AFTER.as_str()),
        }
    }
}

/// The header `name` as a plain non-negative integer, `u64::MAX` when it is larger; anything
/// else (an HTTP date, a sign, a fraction, bytes that are not text) is `None`.
fn header_number(headers: &HeaderMap, name: &str) -> Option<u64> {
    let text = headers.get(name)?.to_str().ok()?.trim();
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    // Digits only, so the one way the parse can fail is a number past u64::MAX.
    Some(text.parse().unwrap_or(u64::MAX))
}

/// Sends `request`, reads a 2xx body up to the cap, and returns the answer with its status and
/// headers. Only a failure to send or to read, a timeout, and an over-cap body are `Err`.
/// Logs one debug line naming the method, the host, and `path_kind` (a fixed word, since the
/// path holds package names).
pub(crate) async fn fetch(
    request: RequestBuilder,
    limits: &HttpLimits,
    path_kind: &'static str,
) -> Result<Fetched, SourceFailure> {
    let (client, request) = request.build_split();
    let request = request.map_err(classify)?;
    let method = request.method().clone();
    let host = request.url().host_str().unwrap_or_default().to_owned();
    let started = Instant::now();
    let result = send(&client, request, limits).await;
    let status = match &result {
        Ok(fetched) => fetched.status.to_string(),
        Err(failure) => failure.kind().to_owned(),
    };
    debug!(
        method = %method,
        host = %host,
        path_kind,
        status = %status,
        elapsed_ms = elapsed_ms(started),
        "request"
    );
    result
}

async fn send(
    client: &Client,
    request: reqwest::Request,
    limits: &HttpLimits,
) -> Result<Fetched, SourceFailure> {
    let mut response = client.execute(request).await.map_err(classify)?;
    let status = response.status();
    let meta = ResponseMeta::from_headers(response.headers());
    let body = if status.is_success() {
        read_capped(&mut response, limits.max_body_bytes).await?
    } else {
        Vec::new()
    };
    Ok(Fetched {
        status: status.as_u16(),
        meta,
        body,
    })
}

async fn read_capped(response: &mut Response, cap: usize) -> Result<Vec<u8>, SourceFailure> {
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(classify)? {
        if chunk.len() > cap - body.len() {
            return Err(SourceFailure::BadAnswer {
                detail: format!("the answer was larger than {}", size_words(cap)),
            });
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn size_words(bytes: usize) -> String {
    if bytes >= BYTES_PER_MIB && bytes.is_multiple_of(BYTES_PER_MIB) {
        format!("{} MiB", bytes / BYTES_PER_MIB)
    } else {
        format!("{bytes} bytes")
    }
}

/// The failure a reqwest error is. Every reqwest error is classified here and nowhere else.
fn classify(error: reqwest::Error) -> SourceFailure {
    if error.is_timeout() {
        SourceFailure::TimedOut
    } else if error.is_connect() {
        SourceFailure::Unreachable {
            detail: format!("{:#}", anyhow!(error.without_url())),
        }
    } else {
        SourceFailure::Interrupted
    }
}

/// What a service answered: the parsed value, or that it does not know the thing asked about.
#[derive(Debug, PartialEq)]
pub(crate) enum Answer<T> {
    Found(T),
    NotFound,
}

/// GETs `url` and reads the answer as JSON.
pub(crate) async fn get_json<T: DeserializeOwned>(
    client: &Client,
    url: Url,
    limits: &HttpLimits,
    clock: &Clock,
    path_kind: &'static str,
) -> Result<Answer<T>, SourceFailure> {
    let fetched = fetch(client.get(url), limits, path_kind).await?;
    read_answer(fetched, clock)
}

/// POSTs `body` as JSON to `url` and reads the answer as JSON.
pub(crate) async fn post_json<B: Serialize, T: DeserializeOwned>(
    client: &Client,
    url: Url,
    body: &B,
    limits: &HttpLimits,
    clock: &Clock,
    path_kind: &'static str,
) -> Result<Answer<T>, SourceFailure> {
    let fetched = fetch(client.post(url).json(body), limits, path_kind).await?;
    read_answer(fetched, clock)
}

/// 2xx parses, 404 is `NotFound`, 429 is a block counted from `clock` read now (when the
/// answer arrived), and any other status is `Status`.
fn read_answer<T: DeserializeOwned>(
    fetched: Fetched,
    clock: &Clock,
) -> Result<Answer<T>, SourceFailure> {
    match fetched.status {
        200..=299 => serde_json::from_slice(&fetched.body)
            .map(Answer::Found)
            .map_err(|error| bad_json(&error)),
        404 => Ok(Answer::NotFound),
        429 => Err(SourceFailure::RateLimited {
            until: block_until(clock(), fetched.meta.retry_after_seconds),
        }),
        code => Err(SourceFailure::Status { code }),
    }
}

/// A parse failure described by its kind and position only: serde's own message quotes the
/// bad value, which is text from the service.
pub(crate) fn bad_json(error: &serde_json::Error) -> SourceFailure {
    let what = match error.classify() {
        Category::Io => "a read error",
        Category::Syntax => "a syntax error",
        Category::Data => "a data error",
        Category::Eof => "an answer that ended early",
    };
    SourceFailure::BadAnswer {
        detail: format!("{what} at line {}, column {}", error.line(), error.column()),
    }
}

/// A failure for an answer that parsed but does not fit the question.
pub(crate) fn mismatched_answer() -> SourceFailure {
    SourceFailure::bad_answer("the answer did not match the question")
}

/// Every byte but ASCII letters, digits, and `-._~` is percent-encoded.
const SEGMENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// `value` percent-encoded for one URL path segment or query value: everything but ASCII
/// letters, digits, and `-._~`.
pub fn encode_segment(value: &str) -> String {
    utf8_percent_encode(value, SEGMENT).to_string()
}

/// Milliseconds since `started`.
pub(crate) fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Transport failures in a row after which a source stops.
pub const STOP_AFTER_FAILURES: u32 = 3;

/// Whether one source may still send, from what its answers so far said: a rate-limit block
/// stops it for the rest of the run, and so do [`STOP_AFTER_FAILURES`] transport failures in a
/// row, even when a request already in flight answers after the stop. Shared by the source's
/// requests in flight.
#[derive(Debug, Default)]
pub(crate) struct SourceGate {
    state: Mutex<GateState>,
}

#[derive(Debug, Default)]
struct GateState {
    block: Option<DateTime<Utc>>,
    failures_in_a_row: u32,
    /// The failure that stopped the source, set at the last of the failures in a row.
    stopped: Option<SourceFailure>,
    requests: u32,
}

impl SourceGate {
    /// Counts one request when the source may send; otherwise the reason it may not, which its
    /// unsent requests fail with.
    pub fn open(&self) -> Result<(), SourceFailure> {
        let mut state = self.lock();
        if let Some(until) = state.block {
            return Err(SourceFailure::RateLimited { until });
        }
        if let Some(failure) = &state.stopped {
            return Err(failure.clone());
        }
        state.requests = state.requests.saturating_add(1);
        Ok(())
    }

    /// Takes in one request's outcome. Any answer from the service, an error status included,
    /// ends a run of transport failures that has not stopped the source yet; a block is kept,
    /// the later one when there are two.
    pub fn record<T>(&self, outcome: &Result<T, SourceFailure>) {
        let mut state = self.lock();
        match outcome {
            Ok(_) | Err(SourceFailure::Status { .. }) | Err(SourceFailure::BadAnswer { .. }) => {
                state.failures_in_a_row = 0
            }
            Err(SourceFailure::RateLimited { until }) => {
                state.failures_in_a_row = 0;
                state.block = Some(state.block.map_or(*until, |block| block.max(*until)));
            }
            Err(
                failure @ (SourceFailure::Unreachable { .. }
                | SourceFailure::TimedOut
                | SourceFailure::Interrupted),
            ) => {
                state.failures_in_a_row = state.failures_in_a_row.saturating_add(1);
                if state.failures_in_a_row >= STOP_AFTER_FAILURES && state.stopped.is_none() {
                    state.stopped = Some(failure.clone());
                }
            }
        }
    }

    /// The block this run hit, if any.
    pub fn block(&self) -> Option<DateTime<Utc>> {
        self.lock().block
    }

    /// The requests sent so far.
    pub fn requests(&self) -> u32 {
        self.lock().requests
    }

    fn lock(&self) -> MutexGuard<'_, GateState> {
        // Nothing panics while holding the lock, and the counters stay meaningful if something
        // ever did, so a poisoned lock is used as it is.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests;

//! The download: one GET of the dump into the `.part` file, with its size required up front.

use super::{ProgressFn, Stop, Throttle, UpdateFailure, UpdateProgress};
use crate::http::{classify, elapsed_ms};
use crate::offline::store::remove_if_present;
use crate::offline::update::UpdateLimits;
use common_file_utils::permissions::open_owner_only;
use reqwest::header::{CONTENT_LENGTH, ETAG, HeaderMap, HeaderValue, IF_NONE_MATCH};
use reqwest::{Client, Response};
use std::fs::{File, OpenOptions};
use std::future::Future;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use tokio::io::AsyncWriteExt;
use tracing::debug;
use url::Url;

/// Removes whatever is at `path` (a link itself, never its target) and creates a new, empty
/// file there for reading and writing. Being created new, it is never a link someone planted.
pub(super) fn fresh_part(path: &Path) -> io::Result<File> {
    remove_if_present(path)?;
    open_owner_only(
        OpenOptions::new().read(true).write(true).create_new(true),
        path,
    )
}

/// One download's inputs.
pub(super) struct Fetch<'a> {
    pub client: &'a Client,
    pub url: Url,
    /// The stored `ETag`, sent as `If-None-Match`.
    pub etag: Option<&'a str>,
    pub limits: &'a UpdateLimits,
    pub deadline: Option<Instant>,
    pub progress: &'a ProgressFn,
    pub cancel: &'a AtomicBool,
    /// The empty `.part` file the body is written to.
    pub part: File,
    /// The bytes received so far, for the finished log line.
    pub received: &'a mut u64,
}

/// What the download got.
pub(super) enum Fetched {
    /// A 304 to the `If-None-Match` sent.
    NotModified,
    /// The whole body is in `file`.
    Complete {
        file: File,
        archive_bytes: u64,
        /// The answer's `ETag`, kept for the next update.
        etag: Option<String>,
    },
}

/// GETs the dump and streams it into the `.part` file. hyper frames the body by its
/// `Content-Length`, so a body that stops short ends with an error, never as a whole file.
/// Logs one debug line for the request with its host and status, never its path.
pub(super) async fn fetch(fetch: Fetch<'_>) -> Result<Fetched, Stop> {
    let host = fetch.url.host_str().unwrap_or_default().to_owned();
    let mut request = fetch.client.get(fetch.url);
    // A stored ETag that is not a header value is not sent; the download is then whole, and a
    // 304 to it is not an answer to anything.
    let if_none_match = fetch.etag.and_then(|etag| HeaderValue::from_str(etag).ok());
    let asked_if_changed = if_none_match.is_some();
    if let Some(value) = if_none_match {
        request = request.header(IF_NONE_MATCH, value);
    }
    let started = Instant::now();
    let sent = within(fetch.deadline, request.send())
        .await?
        .map_err(classify);
    let status = match &sent {
        Ok(response) => response.status().as_u16().to_string(),
        Err(failure) => failure.kind().to_owned(),
    };
    debug!(
        method = "GET",
        host = %host,
        path_kind = "offline_archive",
        status = %status,
        elapsed_ms = elapsed_ms(started),
        "request"
    );
    let mut response = sent.map_err(|failure| Stop::Failed(UpdateFailure::Request(failure)))?;
    match response.status().as_u16() {
        200 => {}
        304 if asked_if_changed => return Ok(Fetched::NotModified),
        code => return Err(Stop::Failed(UpdateFailure::Status(code))),
    }
    let total = content_length(response.headers()).ok_or(UpdateFailure::NoSize)?;
    if total > fetch.limits.max_archive_bytes {
        return Err(Stop::Failed(UpdateFailure::TooLarge(total)));
    }
    let etag = response
        .headers()
        .get(ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let body = Body {
        total,
        deadline: fetch.deadline,
        progress: fetch.progress,
        cancel: fetch.cancel,
        received: fetch.received,
    };
    let file = body.stream(&mut response, fetch.part).await?;
    Ok(Fetched::Complete {
        file,
        archive_bytes: total,
        etag,
    })
}

/// Where the body's chunks go and what is reported about them.
struct Body<'a> {
    total: u64,
    deadline: Option<Instant>,
    progress: &'a ProgressFn,
    cancel: &'a AtomicBool,
    received: &'a mut u64,
}

impl Body<'_> {
    /// Writes every chunk to `part`, sending `Downloading` at most every 100 ms and once at the
    /// end, and stops between chunks when `cancel` is set.
    async fn stream(self, response: &mut Response, part: File) -> Result<File, Stop> {
        let mut file = tokio::fs::File::from_std(part);
        let mut throttle = Throttle::default();
        let mut received: u64 = 0;
        loop {
            let chunk = within(self.deadline, response.chunk())
                .await?
                .map_err(|error| Stop::Failed(UpdateFailure::Request(classify(error))))?;
            let Some(chunk) = chunk else {
                break;
            };
            file.write_all(&chunk)
                .await
                .map_err(|error| UpdateFailure::disk(&error))?;
            received = received.saturating_add(chunk.len() as u64);
            *self.received = received;
            if received < self.total && throttle.ready(Instant::now()) {
                (self.progress)(UpdateProgress::Downloading {
                    received,
                    total: self.total,
                });
            }
            if self.cancel.load(Ordering::SeqCst) {
                return Err(Stop::Cancelled);
            }
        }
        file.flush()
            .await
            .map_err(|error| UpdateFailure::disk(&error))?;
        (self.progress)(UpdateProgress::Downloading {
            received,
            total: self.total,
        });
        Ok(file.into_std().await)
    }
}

/// `future`, ended with `Deadline` when `deadline` passes first.
async fn within<T>(deadline: Option<Instant>, future: impl Future<Output = T>) -> Result<T, Stop> {
    match deadline {
        None => Ok(future.await),
        Some(at) => tokio::time::timeout_at(tokio::time::Instant::from_std(at), future)
            .await
            .map_err(|_| Stop::Failed(UpdateFailure::Deadline)),
    }
}

/// The `Content-Length` header as a number. reqwest's `content_length()` "does not directly
/// represents the value of the `Content-Length` header", so the header is read itself.
fn content_length(headers: &HeaderMap) -> Option<u64> {
    headers
        .get(CONTENT_LENGTH)?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()
}

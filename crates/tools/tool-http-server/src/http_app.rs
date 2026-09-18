use crate::conditional::{if_range_allows, not_modified, strong_etag};
use crate::models::{DirEntry, FileEntry, ServerConfig};
use crate::range::{RangeOutcome, parse_byte_range};
use crate::zip_stream::{serve_directory_zip, serve_selection_zip};
use common_cli::broken_pipe::{BrokenPipe, write_out};
use percent_encoding::{AsciiSet, CONTROLS, percent_decode_str, utf8_percent_encode};
use std::collections::HashSet;
use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;
use tracing::{debug, error, info, warn};
use warp::http::StatusCode;
use warp::http::header::{
    ACCEPT_RANGES, ALLOW, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, ETAG, HeaderValue,
    LAST_MODIFIED,
};
use warp::{Filter, Reply};

/// Read-buffer size for streamed file responses.
const STREAM_BUFFER_SIZE: usize = 64 * 1024;

/// Characters percent-encoded inside href path segments: everything that would
/// terminate the path early (`?`, `#`), break out of a quoted attribute
/// (`"`, `'`, `<`, `>`, backtick), or corrupt decoding (`%`, space).
const HREF_SEGMENT_ENCODE: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'\'')
    .add(b'<')
    .add(b'>')
    .add(b'`')
    .add(b'#')
    .add(b'?')
    .add(b'%');

/// The request headers that drive conditional and ranged file responses.
#[derive(Clone, Copy, Default)]
struct RequestHeaders<'a> {
    range: Option<&'a str>,
    if_range: Option<&'a str>,
    if_none_match: Option<&'a str>,
    if_modified_since: Option<&'a str>,
}

/// Upper bound on `pick` parameters per request, so a request bounds its own
/// validation work (each pick costs filesystem calls) before any zip build
/// slot is consulted.
const MAX_PICKS: usize = 512;

/// The query parameters the server understands. Parsed from the raw query
/// string so repeated `pick` keys survive; everything else in the query is
/// ignored, as it always was.
#[derive(Debug, Default, PartialEq)]
struct QueryOptions {
    download: Option<String>,
    picks: Vec<String>,
}

/// Parses the raw query string with the decoder behind warp's typed query
/// filter, giving that filter's decoding semantics: plus-as-space,
/// percent-decoding, lossy UTF-8, bare keys as empty values. `download`
/// collapses to its last occurrence, as a map extraction would; `pick` values
/// are collected in request order.
fn parse_query(raw: &str) -> Result<QueryOptions, serde_urlencoded::de::Error> {
    let pairs: Vec<(String, String)> = serde_urlencoded::from_str(raw)?;
    let mut options = QueryOptions::default();
    for (key, value) in pairs {
        match key.as_str() {
            "download" => options.download = Some(value),
            "pick" => options.picks.push(value),
            _ => {}
        }
    }
    Ok(options)
}

/// Extracts the raw query string, or an empty string when the request has no
/// query at all (warp's `query::raw` rejects that case instead of defaulting).
fn raw_query_or_empty() -> impl Filter<Extract = (String,), Error = std::convert::Infallible> + Clone
{
    warp::query::raw().or_else(|_| async { Ok::<_, std::convert::Infallible>((String::new(),)) })
}

/// Composes the server's single route: every request, whatever its path,
/// method, or query, funnels through [`handle_request`] and its security
/// checks. Exposed as a seam so tests can drive the composed route through
/// `warp::test` without binding a socket.
fn build_routes(
    root_path: PathBuf,
    serve_hidden: bool,
) -> impl Filter<Extract = (warp::reply::Response,), Error = warp::Rejection>
+ Clone
+ Send
+ Sync
+ 'static {
    warp::path::full()
        .and(warp::method())
        .and(warp::header::optional::<String>("range"))
        .and(warp::header::optional::<String>("if-range"))
        .and(warp::header::optional::<String>("if-none-match"))
        .and(warp::header::optional::<String>("if-modified-since"))
        .and(raw_query_or_empty())
        .and_then(
            move |path: warp::path::FullPath,
                  method: warp::http::Method,
                  range: Option<String>,
                  if_range: Option<String>,
                  if_none_match: Option<String>,
                  if_modified_since: Option<String>,
                  raw_query: String| {
                let root_path = root_path.clone();
                async move {
                    let query = match parse_query(&raw_query) {
                        Ok(query) => query,
                        // Mirrors warp's own mapping of query rejections.
                        Err(_) => {
                            return Ok(warp::reply::with_status(
                                "Invalid query string",
                                StatusCode::BAD_REQUEST,
                            )
                            .into_response());
                        }
                    };
                    handle_request(
                        root_path,
                        path.as_str(),
                        method,
                        serve_hidden,
                        RequestHeaders {
                            range: range.as_deref(),
                            if_range: if_range.as_deref(),
                            if_none_match: if_none_match.as_deref(),
                            if_modified_since: if_modified_since.as_deref(),
                        },
                        &query,
                    )
                    .await
                }
            },
        )
}

pub async fn start_server(config: ServerConfig) {
    // Create a filter for logging requests
    let log_filter = create_request_logger();

    let routes = build_routes(config.root_path.clone(), config.serve_hidden).with(log_filter);

    let addr: SocketAddr = (config.host, config.port).into();

    // The banner is informational; a dead stdout must not stop the server.
    let banner = format!("Server running at http://{}\n", addr);
    if let Err(error) = write_out(&mut std::io::stdout(), banner.as_bytes()) {
        if error.is::<BrokenPipe>() {
            debug!("Startup banner skipped: stdout closed by the consumer");
        } else {
            warn!("Startup banner not printed: {error:#}");
        }
    }

    warp::serve(routes)
        .bind(addr)
        .await
        .graceful(async {
            match tokio::signal::ctrl_c().await {
                Ok(()) => info!("Shutdown signal received, stopping server"),
                Err(e) => {
                    error!(error = %e, "shutdown signal listener failed; stopping the server")
                }
            }
        })
        .run()
        .await;
}

async fn handle_request(
    root_path: PathBuf,
    request_path: &str,
    method: warp::http::Method,
    serve_hidden: bool,
    headers: RequestHeaders<'_>,
    query: &QueryOptions,
) -> Result<warp::reply::Response, warp::Rejection> {
    let head_only = method == warp::http::Method::HEAD;
    if method != warp::http::Method::GET && !head_only {
        let mut response = warp::reply::with_status(
            "Method not allowed",
            warp::http::StatusCode::METHOD_NOT_ALLOWED,
        )
        .into_response();
        response
            .headers_mut()
            .insert(ALLOW, HeaderValue::from_static("GET, HEAD"));
        return Ok(response);
    }

    let mut response = resolve_and_serve(
        root_path,
        request_path,
        serve_hidden,
        head_only,
        headers,
        query,
    )
    .await?;

    // A HEAD answer is the GET answer minus the body; the security checks
    // and header production above are method-agnostic on purpose.
    if head_only {
        *response.body_mut() = Default::default();
    }
    Ok(response)
}

/// Resolves the request path inside the web root and serves the file,
/// directory listing, index file, or zip download it names. Every branch
/// runs after the traversal and hidden-segment checks.
async fn resolve_and_serve(
    root_path: PathBuf,
    request_path: &str,
    serve_hidden: bool,
    head_only: bool,
    headers: RequestHeaders<'_>,
    query: &QueryOptions,
) -> Result<warp::reply::Response, warp::Rejection> {
    // Decode URL path
    let decoded_path = percent_decode_str(request_path)
        .decode_utf8()
        .map_err(|_| warp::reject::not_found())?;

    // Remove the leading slash and resolve a path
    let relative_path = decoded_path.trim_start_matches('/');
    let file_path = if relative_path.is_empty() {
        root_path.clone()
    } else {
        root_path.join(relative_path)
    };

    // Security check: ensure the path is within the root directory
    let canonical_file_path = match file_path.canonicalize() {
        Ok(path) => path,
        Err(_) => return Err(warp::reject::not_found()),
    };

    let canonical_root_path = match root_path.canonicalize() {
        Ok(path) => path,
        Err(_) => return Err(warp::reject::not_found()),
    };

    if !canonical_file_path.starts_with(&canonical_root_path) {
        return Err(warp::reject::not_found());
    }

    // Block access to hidden files/directories unless explicitly enabled.
    // Both the URL path and the canonicalized target are screened: the URL
    // check alone would let a non-hidden symlink inside the root alias a
    // hidden sibling into view.
    if !serve_hidden {
        if contains_hidden_segment(relative_path) {
            return Err(warp::reject::not_found());
        }
        if let Ok(canonical_relative) = canonical_file_path.strip_prefix(&canonical_root_path)
            && canonical_relative
                .components()
                .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
        {
            return Err(warp::reject::not_found());
        }
    }

    if !canonical_file_path.exists() {
        return Err(warp::reject::not_found());
    }

    if canonical_file_path.is_file() {
        // Serve the file
        serve_file(&canonical_file_path, headers).await
    } else if canonical_file_path.is_dir() {
        // An explicit download request wins over index files, so a directory
        // carrying an index.html can still be fetched as an archive. A request
        // with `pick` parameters serves exactly the validated selection or
        // fails whole: the access log records only the URI path, so a silent
        // fallback to the full directory would be indistinguishable from an
        // intended whole-directory download.
        if query.download.as_deref() == Some("zip") {
            if query.picks.is_empty() {
                return serve_directory_zip(&canonical_file_path, serve_hidden, head_only).await;
            }
            let roots = validate_picks(&canonical_file_path, &query.picks, serve_hidden)?;
            return serve_selection_zip(&canonical_file_path, roots, serve_hidden, head_only).await;
        }

        // Check for index files
        let index_files = ["index.html", "index.htm"];
        for index_file in &index_files {
            let index_path = canonical_file_path.join(index_file);
            if index_path.exists() && index_path.is_file() {
                return serve_file(&index_path, headers).await;
            }
        }

        // No index file found, serve directory listing
        serve_directory_listing(
            &canonical_file_path,
            &canonical_root_path,
            request_path,
            serve_hidden,
        )
        .await
    } else {
        Err(warp::reject::not_found())
    }
}

/// Resolves the request's `pick` parameters against the canonicalized listed
/// directory into the canonical paths a selection archive will read.
/// All-or-nothing: any invalid pick fails the whole request, so a selection
/// is never served silently lighter than asked. One warning names the
/// offenders (capped) before the rejection, since the access log records only
/// the URI path and would otherwise show a bare 404.
fn validate_picks(
    listed_dir: &Path,
    picks: &[String],
    serve_hidden: bool,
) -> Result<Vec<PathBuf>, warp::Rejection> {
    if picks.len() > MAX_PICKS {
        warn!(
            dir = %listed_dir.display(),
            picks = picks.len(),
            limit = MAX_PICKS,
            "Selection rejected: too many pick parameters"
        );
        return Err(warp::reject::not_found());
    }

    let mut invalid: Vec<&str> = Vec::new();
    let mut seen = HashSet::new();
    let mut roots = Vec::new();
    let mut duplicates = 0usize;

    for pick in picks {
        match resolve_pick(listed_dir, pick, serve_hidden) {
            Some(path) => {
                if seen.insert(path.clone()) {
                    roots.push(path);
                } else {
                    duplicates += 1;
                }
            }
            None => invalid.push(pick.as_str()),
        }
    }

    if !invalid.is_empty() {
        let shown: Vec<&str> = invalid.iter().take(5).copied().collect();
        warn!(
            dir = %listed_dir.display(),
            invalid = invalid.len(),
            picks = ?shown,
            "Selection rejected: invalid pick parameters"
        );
        return Err(warp::reject::not_found());
    }
    if duplicates > 0 {
        debug!(
            dir = %listed_dir.display(),
            duplicates,
            "Dropped duplicate picks from the selection"
        );
    }

    Ok(roots)
}

/// Resolves one pick to its canonical path, or `None` when it is invalid.
/// Picks name direct children of the listed directory only: a single path
/// component, never `.` or `..`, no separators. A pick that is a symlink is
/// refused, since the archive walk never records symlinks. The canonicalized
/// result must stay under the listed directory, and its relative form is
/// screened for hidden segments the same way URL paths are (the pick itself
/// is lstat'ed as a non-link, but the two canonical checks stay as the
/// backstop should the entry change underneath this function).
fn resolve_pick(listed_dir: &Path, pick: &str, serve_hidden: bool) -> Option<PathBuf> {
    if pick.is_empty() || pick == "." || pick == ".." || pick.contains(['/', '\\']) {
        return None;
    }
    // The pick must parse as exactly one normal path component. This also
    // rejects Windows drive-relative forms like `C:name`, which carry a
    // prefix component without any separator and would otherwise resolve
    // against the process's per-drive working directory in `Path::join`.
    let mut components = Path::new(pick).components();
    if !matches!(
        (components.next(), components.next()),
        (Some(Component::Normal(_)), None)
    ) {
        return None;
    }
    if !serve_hidden && pick.starts_with('.') {
        return None;
    }

    let candidate = listed_dir.join(pick);
    let metadata = std::fs::symlink_metadata(&candidate).ok()?;
    if metadata.file_type().is_symlink() {
        return None;
    }

    let canonical = candidate.canonicalize().ok()?;
    if !canonical.starts_with(listed_dir) {
        return None;
    }
    if !serve_hidden {
        let relative = canonical.strip_prefix(listed_dir).ok()?;
        if relative
            .components()
            .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
        {
            return None;
        }
    }

    Some(canonical)
}

/// Serves a file as a streamed response. Without a `Range` header the whole
/// file goes out as a 200; a single satisfiable `bytes=` range goes out as a
/// 206 with `Content-Range`; an unsatisfiable one answers 416. Every
/// success response advertises `Accept-Ranges: bytes`, carries an exact
/// `Content-Length`, and (when the filesystem reports a modification time)
/// `Last-Modified` plus a strong metadata-based `ETag`. A request whose
/// `If-None-Match` or `If-Modified-Since` validator is still current is
/// answered with a bodyless 304 carrying those same validators, and that
/// check runs before any range processing because RFC 9110 §13.2.2 orders
/// the conditionals ahead of `Range`. A `Range` request
/// carrying an `If-Range` validator that no longer matches is served whole,
/// so a file replaced between range requests cannot splice inconsistent
/// bytes into a resumed download. The body is read in [`STREAM_BUFFER_SIZE`]
/// chunks, so large files never buffer whole in memory.
async fn serve_file(
    file_path: &Path,
    headers: RequestHeaders<'_>,
) -> Result<warp::reply::Response, warp::Rejection> {
    // The caller already screened this path with exists(), so a failure here
    // is a permission or I/O fault worth surfacing, even though the client
    // still just gets a 404.
    let mut file = match tokio::fs::File::open(file_path).await {
        Ok(file) => file,
        Err(error) => {
            warn!(path = %file_path.display(), error = %error, "Failed to open an existing file");
            return Err(warp::reject::not_found());
        }
    };
    let metadata = match file.metadata().await {
        Ok(metadata) => metadata,
        Err(error) => {
            warn!(path = %file_path.display(), error = %error, "Failed to stat an existing file");
            return Err(warp::reject::not_found());
        }
    };
    let len = metadata.len();
    // Validators come from this one stat of the open handle, so the bytes
    // streamed below can never belong to a different file than the one the
    // validators describe. A pre-epoch modification time is dropped here
    // because httpdate cannot format it; such a file serves without
    // validators.
    let modified = metadata
        .modified()
        .ok()
        .filter(|modified| modified.duration_since(UNIX_EPOCH).is_ok());
    let etag = modified.and_then(|modified| strong_etag(modified, len));

    if not_modified(
        headers.if_none_match,
        headers.if_modified_since,
        etag.as_deref(),
        modified,
    ) {
        let mut response = warp::reply::Response::default();
        *response.status_mut() = StatusCode::NOT_MODIFIED;
        insert_validators(&mut response, modified, etag.as_deref());
        return Ok(response);
    }
    if headers.if_none_match.is_some() || headers.if_modified_since.is_some() {
        debug!(path = %file_path.display(), "Conditional validators stale; serving the full response");
    }

    let range_header = if if_range_allows(
        headers.if_range,
        etag.as_deref(),
        modified,
        SystemTime::now(),
    ) {
        headers.range
    } else {
        if headers.range.is_some() {
            debug!(path = %file_path.display(), "If-Range validator mismatch; serving the whole file");
        }
        None
    };

    let (status, span) = match parse_byte_range(range_header, len) {
        RangeOutcome::Full => (StatusCode::OK, 0..len),
        RangeOutcome::Partial(span) => (StatusCode::PARTIAL_CONTENT, span),
        RangeOutcome::Unsatisfiable => {
            let mut response =
                warp::reply::with_status("", StatusCode::RANGE_NOT_SATISFIABLE).into_response();
            insert_header_checked(&mut response, CONTENT_RANGE, &format!("bytes */{len}"));
            response
                .headers_mut()
                .insert(ACCEPT_RANGES, HeaderValue::from_static("bytes"));
            return Ok(response);
        }
    };

    if span.start != 0
        && let Err(error) = file.seek(std::io::SeekFrom::Start(span.start)).await
    {
        warn!(path = %file_path.display(), error = %error, "Failed to seek to the requested range");
        return Err(warp::reject::not_found());
    }

    let mime_type = mime_guess::from_path(file_path)
        .first_or_octet_stream()
        .to_string();

    let stream = ReaderStream::with_capacity(file.take(span.end - span.start), STREAM_BUFFER_SIZE);
    let mut response = warp::reply::stream(stream).into_response();
    *response.status_mut() = status;

    insert_header_checked(&mut response, CONTENT_TYPE, &mime_type);
    response
        .headers_mut()
        .insert(CONTENT_LENGTH, HeaderValue::from(span.end - span.start));
    response
        .headers_mut()
        .insert(ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    insert_validators(&mut response, modified, etag.as_deref());
    if status == StatusCode::PARTIAL_CONTENT {
        insert_header_checked(
            &mut response,
            CONTENT_RANGE,
            &format!("bytes {}-{}/{}", span.start, span.end - 1, len),
        );
    }

    Ok(response)
}

/// Inserts the `Last-Modified` and `ETag` validators shared by the 200/206
/// file responses and the 304 revalidation answer.
fn insert_validators(
    response: &mut warp::reply::Response,
    modified: Option<SystemTime>,
    etag: Option<&str>,
) {
    if let Some(modified) = modified {
        insert_header_checked(response, LAST_MODIFIED, &httpdate::fmt_http_date(modified));
    }
    if let Some(etag) = etag {
        insert_header_checked(response, ETAG, etag);
    }
}

/// Inserts a header whose value is built at runtime. The values produced
/// here (mime strings, `bytes ...` ranges) are always valid; an invalid one
/// would only mean a missing header on the response, so it is logged rather
/// than panicked on.
fn insert_header_checked(
    response: &mut warp::reply::Response,
    name: warp::http::header::HeaderName,
    value: &str,
) {
    match HeaderValue::from_str(value) {
        Ok(header_value) => {
            response.headers_mut().insert(name, header_value);
        }
        Err(error) => {
            warn!(header = %name, value, error = %error, "Skipping a header value that failed validation");
        }
    }
}

/// Escapes the five HTML-special characters so untrusted names and paths are
/// inert in both element and attribute contexts. `&` is replaced first so the
/// entities produced by the other replacements are not escaped again.
fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Percent-encodes one path segment for use inside an href attribute, so file
/// names containing `#`, `?`, quotes, or spaces produce working links.
fn encode_href_segment(segment: &str) -> String {
    utf8_percent_encode(segment, HREF_SEGMENT_ENCODE).to_string()
}

/// Collects directory entries, optionally including hidden files/directories.
///
/// Returns `(directories, files)` where directories are `(name, relative_path)` tuples
/// and files are `(name, relative_path, size)` tuples, both sorted alphabetically by name.
/// The name is the raw file name; the relative path appends the name to the request
/// path as a percent-encoded href segment.
async fn collect_directory_entries(
    dir_path: &Path,
    request_path: &str,
    serve_hidden: bool,
) -> std::io::Result<(Vec<DirEntry>, Vec<FileEntry>)> {
    let mut entries = tokio::fs::read_dir(dir_path).await?;

    let mut files = Vec::new();
    let mut directories = Vec::new();

    // An entry that fails mid-walk ends the walk with what was collected, so
    // one bad entry renders a partial listing rather than a 404 for the
    // whole directory. Only a directory that cannot be opened at all errors.
    loop {
        let entry = match entries.next_entry().await {
            Ok(Some(entry)) => entry,
            Ok(None) => break,
            Err(e) => {
                warn!(
                    "Directory listing truncated: reading an entry of [{}] failed: {}",
                    dir_path.display(),
                    e
                );
                break;
            }
        };
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_string();

        // Skip hidden files and directories unless explicitly enabled
        if !serve_hidden && file_name.starts_with('.') {
            continue;
        }

        let encoded_name = encode_href_segment(&file_name);
        let relative_path = if request_path.ends_with('/') || request_path.is_empty() {
            format!(
                "{}{}",
                if request_path == "/" {
                    ""
                } else {
                    request_path
                },
                encoded_name
            )
        } else {
            format!("{}/{}", request_path, encoded_name)
        };

        // tokio::fs::metadata follows symlinks, so a link to a directory
        // lists as a directory; an unreadable entry is listed as a
        // zero-sized file rather than dropped.
        match tokio::fs::metadata(&path).await {
            Ok(metadata) if metadata.is_dir() => directories.push((file_name, relative_path)),
            Ok(metadata) => files.push((file_name, relative_path, metadata.len())),
            Err(_) => files.push((file_name, relative_path, 0)),
        }
    }

    directories.sort_by(|a, b| a.0.cmp(&b.0));
    files.sort_by(|a, b| a.0.cmp(&b.0));

    Ok((directories, files))
}

async fn serve_directory_listing(
    dir_path: &Path,
    _root_path: &Path,
    request_path: &str,
    serve_hidden: bool,
) -> Result<warp::reply::Response, warp::Rejection> {
    let (directories, files) = collect_directory_entries(dir_path, request_path, serve_hidden)
        .await
        .map_err(|_| warp::reject::not_found())?;

    let html = render_directory_listing(request_path, &directories, &files);

    // The explicit length is what a HEAD response reports once its body is
    // stripped; hyper would otherwise see an empty body and claim 0.
    let content_length = html.len() as u64;
    let mut response =
        warp::reply::with_header(html, "content-type", "text/html; charset=utf-8").into_response();
    response
        .headers_mut()
        .insert(CONTENT_LENGTH, HeaderValue::from(content_length));

    Ok(response)
}

/// Renders the directory-listing page. Every untrusted value (the request path
/// in the title and heading, entry names, and every href) is HTML-escaped
/// before interpolation. The page links the zip form of the listed directory,
/// and each subdirectory row links its own; entry paths arrive with their
/// segments already percent-encoded, so a `?` in a name cannot start the query.
/// The table is a GET form: each entry row carries a `pick` checkbox whose
/// value is the raw entry name (the browser form-encodes it on submit;
/// pre-encoding here would double-encode), and the submit button contributes
/// `download=zip`, so a submission with picks downloads exactly the selection
/// and one with nothing checked downloads the whole directory, same as the
/// header link. The select column's header holds a nameless select-all
/// checkbox driven by the page's one script: the script is a fixed string
/// with no interpolated values, so entry names can never reach a script
/// context, and the checkbox contributes no form field. It renders hidden
/// and only the script reveals it, so without JavaScript (or in an empty
/// directory) the dead control never shows.
fn render_directory_listing(
    request_path: &str,
    directories: &[DirEntry],
    files: &[FileEntry],
) -> String {
    let title = if request_path == "/" || request_path.is_empty() {
        "Index of /".to_string()
    } else {
        format!("Index of {}", html_escape(request_path))
    };

    let zip_href = if request_path.is_empty() {
        "/?download=zip".to_string()
    } else {
        format!("{}?download=zip", request_path)
    };

    let mut html = format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>{}</title>
    <style>
        body {{
            font-family: Arial, sans-serif;
            margin: 40px;
            background-color: #f5f5f5;
        }}
        .container {{
            background-color: white;
            padding: 20px;
            border-radius: 8px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
        }}
        h1 {{
            color: #333;
            border-bottom: 2px solid #ddd;
            padding-bottom: 10px;
        }}
        table {{
            width: 100%;
            border-collapse: collapse;
            margin-top: 20px;
        }}
        th, td {{
            padding: 12px;
            text-align: left;
            border-bottom: 1px solid #ddd;
        }}
        th {{
            background-color: #f8f9fa;
            font-weight: bold;
        }}
        tr:hover {{
            background-color: #f8f9fa;
        }}
        a {{
            color: #007bff;
            text-decoration: none;
        }}
        a:hover {{
            text-decoration: underline;
        }}
        .directory {{
            color: #6f42c1;
        }}
        .file {{
            color: #28a745;
        }}
        .size {{
            text-align: right;
            font-family: monospace;
        }}
        .download {{
            margin: 8px 0 0 0;
            font-size: 0.9em;
        }}
        .zip {{
            color: #6c757d;
        }}
        .select {{
            width: 1%;
        }}
        button.zip {{
            font: inherit;
            font-size: 0.9em;
            cursor: pointer;
        }}
    </style>
</head>
<body>
    <div class="container">
        <h1>{}</h1>
        <p class="download"><a href="{}" class="zip">📦 Download this directory as .zip</a></p>
        <form method="get">
        <table>
            <thead>
                <tr>
                    <th class="select"><input type="checkbox" id="pick-all" title="Select all" hidden></th>
                    <th>Name</th>
                    <th>Type</th>
                    <th>Size</th>
                </tr>
            </thead>
            <tbody>"#,
        title,
        title,
        html_escape(&zip_href)
    );

    // Add a parent directory link if not at the root. The trailing slash a
    // browser normalizes onto directory URLs ("/sub/") is stripped first, so
    // the parent link goes up one level instead of back to the same page.
    if request_path != "/" && !request_path.is_empty() {
        let trimmed = request_path.trim_end_matches('/');
        let parent_path = match trimmed.rfind('/') {
            Some(0) | None => "/".to_string(),
            Some(idx) => trimmed[..idx].to_string(),
        };

        html.push_str(&format!(
            r#"<tr>
                <td></td>
                <td><a href="{}" class="directory">📁 ..</a></td>
                <td>Directory</td>
                <td>-</td>
            </tr>"#,
            html_escape(&parent_path)
        ));
    }

    // Add directories
    for (name, path) in directories {
        html.push_str(&format!(
            r#"<tr>
                <td class="select"><input type="checkbox" name="pick" value="{}"></td>
                <td><a href="{}" class="directory">📁 {}</a></td>
                <td>Directory</td>
                <td class="size"><a href="{}" class="zip">zip</a></td>
            </tr>"#,
            html_escape(name),
            html_escape(path),
            html_escape(name),
            html_escape(&format!("{}?download=zip", path))
        ));
    }

    // Add files
    for (name, path, size) in files {
        let size_str = format_file_size(*size);
        html.push_str(&format!(
            r#"<tr>
                <td class="select"><input type="checkbox" name="pick" value="{}"></td>
                <td><a href="{}" class="file">📄 {}</a></td>
                <td>File</td>
                <td class="size">{}</td>
            </tr>"#,
            html_escape(name),
            html_escape(path),
            html_escape(name),
            size_str
        ));
    }

    html.push_str(
        r#"        </tbody>
        </table>
        <p class="download"><button type="submit" name="download" value="zip" class="zip">📦 Download selected as .zip</button></p>
        </form>
    </div>
    <script>
        (function () {
            "use strict";
            var all = document.getElementById("pick-all");
            var picks = Array.prototype.slice.call(document.querySelectorAll('input[name="pick"]'));
            if (all === null || picks.length === 0) {
                return;
            }
            var sync = function () {
                var checked = picks.filter(function (pick) { return pick.checked; }).length;
                all.checked = checked === picks.length;
                all.indeterminate = checked > 0 && checked < picks.length;
            };
            all.hidden = false;
            all.addEventListener("change", function () {
                picks.forEach(function (pick) {
                    pick.checked = all.checked;
                });
                all.indeterminate = false;
            });
            picks.forEach(function (pick) {
                pick.addEventListener("change", sync);
            });
            sync();
        })();
    </script>
</body>
</html>"#,
    );

    html
}

/// Returns `true` if any segment of the given relative path starts with a dot,
/// indicating a hidden file or directory. Segments split on both separators:
/// `PathBuf::join` honors `\` on Windows, so an encoded backslash in the URL
/// must not slip a dotted segment past this check.
fn contains_hidden_segment(path: &str) -> bool {
    path.split(['/', '\\']).any(|s| s.starts_with('.'))
}

fn format_file_size(size: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = size as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", size as u64, UNITS[unit_index])
    } else {
        format!("{:.1} {}", size, UNITS[unit_index])
    }
}

// Create a function that returns a warp log configuration (not a filter)
fn create_request_logger() -> warp::log::Log<impl Fn(warp::log::Info) + Copy> {
    warp::log::custom(|info| {
        let headers = info.request_headers();
        let user_agent = headers
            .get("user-agent")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("unknown");
        let content_length = headers
            .get("content-length")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("0");

        info!(
            target: "http_server::access_log",
            "HTTP request completed - {} {} {} - {}ms - {} bytes - UA: {}",
            info.method(),
            info.path(),
            info.status(),
            info.elapsed().as_millis(),
            content_length,
            user_agent
        );
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_contains_hidden_segment_dotfile() {
        assert!(contains_hidden_segment(".hidden"));
    }

    #[test]
    fn test_contains_hidden_segment_nested_dotdir() {
        assert!(contains_hidden_segment("public/.secret/file.txt"));
    }

    #[test]
    fn test_contains_hidden_segment_dotdir_at_start() {
        assert!(contains_hidden_segment(".config/settings.json"));
    }

    #[test]
    fn test_contains_hidden_segment_normal_path() {
        assert!(!contains_hidden_segment("public/index.html"));
    }

    #[test]
    fn test_contains_hidden_segment_empty_path() {
        assert!(!contains_hidden_segment(""));
    }

    #[test]
    fn test_contains_hidden_segment_deep_nested() {
        assert!(contains_hidden_segment("a/b/c/.env"));
    }

    #[test]
    fn test_contains_hidden_segment_dot_in_filename() {
        assert!(!contains_hidden_segment("archive.tar.gz"));
    }

    #[test]
    fn test_format_file_size_bytes() {
        assert_eq!(format_file_size(0), "0 B");
        assert_eq!(format_file_size(512), "512 B");
        assert_eq!(format_file_size(1023), "1023 B");
    }

    #[test]
    fn test_format_file_size_kilobytes() {
        assert_eq!(format_file_size(1024), "1.0 KB");
        assert_eq!(format_file_size(1536), "1.5 KB");
    }

    #[test]
    fn test_format_file_size_megabytes() {
        assert_eq!(format_file_size(1024 * 1024), "1.0 MB");
    }

    #[test]
    fn test_format_file_size_gigabytes() {
        assert_eq!(format_file_size(1024 * 1024 * 1024), "1.0 GB");
    }

    #[test]
    fn test_format_file_size_terabytes() {
        assert_eq!(format_file_size(1024u64 * 1024 * 1024 * 1024), "1.0 TB");
    }

    #[tokio::test]
    async fn test_handle_request_hidden_file_blocked_by_default() {
        let dir = tempdir().unwrap();
        let hidden_dir = dir.path().join(".secret");
        fs::create_dir(&hidden_dir).unwrap();
        fs::write(hidden_dir.join("data.txt"), "secret").unwrap();

        let result = handle_request(
            dir.path().to_path_buf(),
            "/.secret/data.txt",
            warp::http::Method::GET,
            false,
            RequestHeaders::default(),
            &QueryOptions::default(),
        )
        .await;

        assert!(result.is_err(), "hidden file should be blocked");
    }

    #[tokio::test]
    async fn test_handle_request_hidden_file_allowed_when_enabled() {
        let dir = tempdir().unwrap();
        let hidden_dir = dir.path().join(".secret");
        fs::create_dir(&hidden_dir).unwrap();
        fs::write(hidden_dir.join("data.txt"), "secret").unwrap();

        let result = handle_request(
            dir.path().to_path_buf(),
            "/.secret/data.txt",
            warp::http::Method::GET,
            true,
            RequestHeaders::default(),
            &QueryOptions::default(),
        )
        .await;

        assert!(result.is_ok(), "hidden file should be served when enabled");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_handle_request_symlink_alias_of_hidden_dir_stays_blocked() {
        let dir = tempdir().unwrap();
        let hidden_dir = dir.path().join(".secret");
        fs::create_dir(&hidden_dir).unwrap();
        fs::write(hidden_dir.join("data.txt"), "secret").unwrap();
        std::os::unix::fs::symlink(&hidden_dir, dir.path().join("alias")).unwrap();

        let result = handle_request(
            dir.path().to_path_buf(),
            "/alias/data.txt",
            warp::http::Method::GET,
            false,
            RequestHeaders::default(),
            &QueryOptions::default(),
        )
        .await;

        assert!(
            result.is_err(),
            "a symlink alias of a hidden directory should stay blocked"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_handle_request_symlink_alias_of_hidden_dir_served_when_enabled() {
        let dir = tempdir().unwrap();
        let hidden_dir = dir.path().join(".secret");
        fs::create_dir(&hidden_dir).unwrap();
        fs::write(hidden_dir.join("data.txt"), "secret").unwrap();
        std::os::unix::fs::symlink(&hidden_dir, dir.path().join("alias")).unwrap();

        let result = handle_request(
            dir.path().to_path_buf(),
            "/alias/data.txt",
            warp::http::Method::GET,
            true,
            RequestHeaders::default(),
            &QueryOptions::default(),
        )
        .await;

        assert!(
            result.is_ok(),
            "the alias should serve once hidden entries are enabled"
        );
    }

    #[tokio::test]
    async fn test_handle_request_serves_regular_file() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("hello.txt"), "hello world").unwrap();

        let result = handle_request(
            dir.path().to_path_buf(),
            "/hello.txt",
            warp::http::Method::GET,
            false,
            RequestHeaders::default(),
            &QueryOptions::default(),
        )
        .await;

        assert!(result.is_ok(), "regular file should be served");
    }

    #[tokio::test]
    async fn test_handle_request_rejects_non_get() {
        let dir = tempdir().unwrap();

        let result = handle_request(
            dir.path().to_path_buf(),
            "/",
            warp::http::Method::POST,
            false,
            RequestHeaders::default(),
            &QueryOptions::default(),
        )
        .await;

        assert!(result.is_ok()); // Returns 405, not a rejection
        let response = result.unwrap();
        assert_eq!(response.status(), 405);
        assert_eq!(response.headers().get("allow").unwrap(), "GET, HEAD");
    }

    #[tokio::test]
    async fn test_collect_entries_hides_dotfiles_by_default() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("visible.txt"), "hi").unwrap();
        fs::write(dir.path().join(".hidden"), "secret").unwrap();
        fs::create_dir(dir.path().join(".secret_dir")).unwrap();
        fs::create_dir(dir.path().join("public_dir")).unwrap();

        let (dirs, files) = collect_directory_entries(dir.path(), "/", false)
            .await
            .unwrap();

        let file_names: Vec<&str> = files.iter().map(|f| f.0.as_str()).collect();
        let dir_names: Vec<&str> = dirs.iter().map(|d| d.0.as_str()).collect();

        assert!(file_names.contains(&"visible.txt"));
        assert!(!file_names.contains(&".hidden"));
        assert!(dir_names.contains(&"public_dir"));
        assert!(!dir_names.contains(&".secret_dir"));
    }

    #[tokio::test]
    async fn test_collect_entries_shows_dotfiles_when_enabled() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("visible.txt"), "hi").unwrap();
        fs::write(dir.path().join(".hidden"), "secret").unwrap();
        fs::create_dir(dir.path().join(".secret_dir")).unwrap();
        fs::create_dir(dir.path().join("public_dir")).unwrap();

        let (dirs, files) = collect_directory_entries(dir.path(), "/", true)
            .await
            .unwrap();

        let file_names: Vec<&str> = files.iter().map(|f| f.0.as_str()).collect();
        let dir_names: Vec<&str> = dirs.iter().map(|d| d.0.as_str()).collect();

        assert!(file_names.contains(&"visible.txt"));
        assert!(file_names.contains(&".hidden"));
        assert!(dir_names.contains(&"public_dir"));
        assert!(dir_names.contains(&".secret_dir"));
    }

    #[tokio::test]
    async fn test_collect_entries_sorted_alphabetically() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("zebra.txt"), "z").unwrap();
        fs::write(dir.path().join("apple.txt"), "a").unwrap();
        fs::create_dir(dir.path().join("beta")).unwrap();
        fs::create_dir(dir.path().join("alpha")).unwrap();

        let (dirs, files) = collect_directory_entries(dir.path(), "/", false)
            .await
            .unwrap();

        assert_eq!(dirs[0].0, "alpha");
        assert_eq!(dirs[1].0, "beta");
        assert_eq!(files[0].0, "apple.txt");
        assert_eq!(files[1].0, "zebra.txt");
    }

    #[tokio::test]
    async fn test_handle_request_rejects_path_traversal() {
        let parent = tempdir().unwrap();
        let root = parent.path().join("root");
        fs::create_dir(&root).unwrap();
        fs::write(parent.path().join("outside.txt"), "outside").unwrap();

        let result = handle_request(
            root,
            "/../outside.txt",
            warp::http::Method::GET,
            false,
            RequestHeaders::default(),
            &QueryOptions::default(),
        )
        .await;

        assert!(
            result.is_err(),
            "traversal outside the root must be blocked"
        );
    }

    #[tokio::test]
    async fn test_handle_request_serves_index_html_for_directory() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("index.html"), "<h1>home</h1>").unwrap();
        fs::write(dir.path().join("other.txt"), "x").unwrap();

        let response = handle_request(
            dir.path().to_path_buf(),
            "/",
            warp::http::Method::GET,
            false,
            RequestHeaders::default(),
            &QueryOptions::default(),
        )
        .await
        .unwrap();

        assert_eq!(response.status(), 200);
        // serve_file sets the bare mime-guessed type; a generated directory
        // listing would carry the "; charset=utf-8" suffix instead.
        assert_eq!(response.headers().get("content-type").unwrap(), "text/html");
    }

    #[tokio::test]
    async fn test_handle_request_serves_file_with_hash_via_encoded_href() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a#b.txt"), "x").unwrap();

        let result = handle_request(
            dir.path().to_path_buf(),
            "/a%23b.txt",
            warp::http::Method::GET,
            false,
            RequestHeaders::default(),
            &QueryOptions::default(),
        )
        .await;

        assert!(result.is_ok(), "encoded hash link should resolve the file");
    }

    #[tokio::test]
    async fn test_collect_entries_percent_encodes_href_segments() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a#b.txt"), "x").unwrap();

        let (_, files) = collect_directory_entries(dir.path(), "/", false)
            .await
            .unwrap();

        assert_eq!(files[0].0, "a#b.txt");
        assert_eq!(files[0].1, "a%23b.txt");
    }

    #[test]
    fn hidden_segment_detected_across_both_separators() {
        assert!(contains_hidden_segment(".secret/data.txt"));
        assert!(contains_hidden_segment("sub/.secret/data.txt"));
        assert!(contains_hidden_segment("sub\\.secret\\data.txt"));
        assert!(!contains_hidden_segment("sub/visible.txt"));
    }

    #[tokio::test]
    async fn test_handle_request_blocks_hidden_file_via_backslash_segments() {
        let dir = tempdir().unwrap();
        let hidden_dir = dir.path().join(".secret");
        fs::create_dir(&hidden_dir).unwrap();
        fs::write(hidden_dir.join("data.txt"), "secret").unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();

        let result = handle_request(
            dir.path().to_path_buf(),
            "/sub\\..\\.secret\\data.txt",
            warp::http::Method::GET,
            false,
            RequestHeaders::default(),
            &QueryOptions::default(),
        )
        .await;

        assert!(
            result.is_err(),
            "backslash segments must not reach the hidden file"
        );
    }

    #[test]
    fn parent_link_goes_up_one_level_despite_trailing_slash() {
        let html = render_directory_listing("/sub/", &[], &[]);

        assert!(html.contains(r#"<a href="/" class="directory">📁 ..</a>"#));
    }

    #[test]
    fn parent_link_from_nested_path_points_to_parent() {
        let html = render_directory_listing("/a/b", &[], &[]);

        assert!(html.contains(r#"<a href="/a" class="directory">📁 ..</a>"#));
    }

    #[test]
    fn test_encode_href_segment_encodes_hash_and_question_mark() {
        assert_eq!(encode_href_segment("a#b.txt"), "a%23b.txt");
        assert_eq!(encode_href_segment("a?b.txt"), "a%3Fb.txt");
    }

    #[test]
    fn test_encode_href_segment_encodes_quotes_space_and_percent() {
        assert_eq!(encode_href_segment(r#"a"b"#), "a%22b");
        assert_eq!(encode_href_segment("a'b c"), "a%27b%20c");
        assert_eq!(encode_href_segment("100%.txt"), "100%25.txt");
    }

    #[test]
    fn test_render_listing_escapes_element_context() {
        let files = vec![(
            "<script>alert(1)</script>.txt".to_string(),
            "/%3Cscript%3Ealert(1)%3C/script%3E.txt".to_string(),
            3,
        )];

        let html = render_directory_listing("/", &[], &files);

        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;.txt"));
        // The page carries its own static script tag; the injected name must
        // never appear as live markup.
        assert!(!html.contains("<script>alert(1)</script>"));
    }

    #[test]
    fn test_render_listing_escapes_attribute_context() {
        let name = r#"x" onmouseover="x"#;
        let files = vec![(name.to_string(), format!("/{}", name), 1)];

        let html = render_directory_listing("/", &[], &files);

        assert!(html.contains("x&quot; onmouseover=&quot;x"));
        assert!(!html.contains(r#"" onmouseover=""#));
    }

    #[test]
    fn test_render_listing_escapes_title_and_heading() {
        let html = render_directory_listing("/<script>x</script>", &[], &[]);

        assert!(html.contains("Index of /&lt;script&gt;x&lt;/script&gt;"));
        assert!(!html.contains("<script>x</script>"));
    }

    #[test]
    fn test_render_listing_escapes_parent_href() {
        let html = render_directory_listing(r#"/a"b/c"#, &[], &[]);

        assert!(html.contains(r#"href="/a&quot;b""#));
        assert!(!html.contains(r#"href="/a"b""#));
    }

    #[test]
    fn test_render_listing_uses_emoji_markers() {
        let dirs = vec![("sub".to_string(), "/sub".to_string())];
        let files = vec![("f.txt".to_string(), "/f.txt".to_string(), 1)];

        let root_html = render_directory_listing("/", &dirs, &files);
        assert!(root_html.contains("📁 sub"));
        assert!(root_html.contains("📄 f.txt"));
        assert!(!root_html.contains("📁 .."), "the root has no parent link");

        let nested_html = render_directory_listing("/sub", &[], &[]);
        assert!(nested_html.contains("📁 .."));
    }

    #[test]
    fn test_render_listing_links_zip_downloads() {
        let dirs = vec![("sub".to_string(), "/sub".to_string())];

        let root_html = render_directory_listing("/", &dirs, &[]);
        assert!(
            root_html.contains(r#"href="/?download=zip""#),
            "the listed directory gets its own download link"
        );
        assert!(
            root_html.contains(r#"href="/sub?download=zip""#),
            "each subdirectory row gets a zip link"
        );

        let nested_html = render_directory_listing("/sub/", &[], &[]);
        assert!(nested_html.contains(r#"href="/sub/?download=zip""#));
    }

    #[test]
    fn test_render_listing_zip_link_keeps_encoded_question_mark_inert() {
        // The entry path arrives with its segments already percent-encoded, so
        // a `?` in a directory name must not become the query separator.
        let dirs = vec![("a?b".to_string(), "/a%3Fb".to_string())];

        let html = render_directory_listing("/", &dirs, &[]);

        assert!(html.contains(r#"href="/a%3Fb?download=zip""#));
    }

    async fn body_bytes(response: warp::reply::Response) -> Vec<u8> {
        use http_body_util::BodyExt;
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec()
    }

    fn range_of(range: &str) -> RequestHeaders<'_> {
        RequestHeaders {
            range: Some(range),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn serve_file_without_range_streams_the_whole_file_with_headers() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("data.txt");
        fs::write(&path, "0123456789").unwrap();

        let response = serve_file(&path, RequestHeaders::default()).await.unwrap();

        assert_eq!(response.status(), 200);
        assert_eq!(response.headers().get("accept-ranges").unwrap(), "bytes");
        assert_eq!(response.headers().get("content-length").unwrap(), "10");
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "text/plain"
        );
        assert!(response.headers().contains_key("etag"));
        assert!(response.headers().contains_key("last-modified"));
        assert_eq!(body_bytes(response).await, b"0123456789");
    }

    #[tokio::test]
    async fn serve_file_with_range_answers_206_with_the_requested_bytes() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("data.txt");
        fs::write(&path, "0123456789").unwrap();

        let response = serve_file(&path, range_of("bytes=2-5")).await.unwrap();

        assert_eq!(response.status(), 206);
        assert_eq!(
            response.headers().get("content-range").unwrap(),
            "bytes 2-5/10"
        );
        assert_eq!(response.headers().get("content-length").unwrap(), "4");
        assert_eq!(response.headers().get("accept-ranges").unwrap(), "bytes");
        assert_eq!(body_bytes(response).await, b"2345");
    }

    #[tokio::test]
    async fn serve_file_open_failure_maps_to_not_found() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("gone.txt");

        let result = serve_file(&missing, RequestHeaders::default()).await;

        assert!(result.is_err(), "an unopenable path must reject as 404");
    }

    #[tokio::test]
    async fn serve_file_with_unsatisfiable_range_answers_416() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("data.txt");
        fs::write(&path, "0123456789").unwrap();

        let response = serve_file(&path, range_of("bytes=999-")).await.unwrap();

        assert_eq!(response.status(), 416);
        assert_eq!(
            response.headers().get("content-range").unwrap(),
            "bytes */10"
        );
    }

    #[tokio::test]
    async fn serve_file_with_stale_if_range_serves_the_whole_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("data.txt");
        fs::write(&path, "0123456789").unwrap();

        let headers = RequestHeaders {
            range: Some("bytes=2-5"),
            if_range: Some("\"deadbeef.0-a\""),
            ..Default::default()
        };
        let response = serve_file(&path, headers).await.unwrap();

        assert_eq!(response.status(), 200);
        assert_eq!(response.headers().get("content-length").unwrap(), "10");
        assert_eq!(body_bytes(response).await, b"0123456789");
    }

    #[tokio::test]
    async fn serve_file_with_current_if_range_keeps_the_206() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("data.txt");
        fs::write(&path, "0123456789").unwrap();

        let first = serve_file(&path, RequestHeaders::default()).await.unwrap();
        let etag = first
            .headers()
            .get("etag")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        let headers = RequestHeaders {
            range: Some("bytes=2-5"),
            if_range: Some(&etag),
            ..Default::default()
        };
        let response = serve_file(&path, headers).await.unwrap();

        assert_eq!(response.status(), 206);
        assert_eq!(body_bytes(response).await, b"2345");
    }

    async fn etag_of(path: &Path) -> String {
        let response = serve_file(path, RequestHeaders::default()).await.unwrap();
        response
            .headers()
            .get("etag")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string()
    }

    #[tokio::test]
    async fn serve_file_with_current_if_none_match_answers_304() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("data.txt");
        fs::write(&path, "0123456789").unwrap();
        let etag = etag_of(&path).await;

        let headers = RequestHeaders {
            if_none_match: Some(&etag),
            ..Default::default()
        };
        let response = serve_file(&path, headers).await.unwrap();

        assert_eq!(response.status(), 304);
        assert_eq!(response.headers().get("etag").unwrap(), etag.as_str());
        assert!(response.headers().contains_key("last-modified"));
        assert!(body_bytes(response).await.is_empty());
    }

    #[tokio::test]
    async fn serve_file_range_with_current_if_none_match_answers_304_not_206() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("data.txt");
        fs::write(&path, "0123456789").unwrap();
        let etag = etag_of(&path).await;

        let headers = RequestHeaders {
            range: Some("bytes=2-5"),
            if_none_match: Some(&etag),
            ..Default::default()
        };
        let response = serve_file(&path, headers).await.unwrap();

        assert_eq!(response.status(), 304);
        assert!(response.headers().get("content-range").is_none());
        assert!(body_bytes(response).await.is_empty());
    }

    #[tokio::test]
    async fn serve_file_with_stale_if_none_match_keeps_the_206() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("data.txt");
        fs::write(&path, "0123456789").unwrap();

        let headers = RequestHeaders {
            range: Some("bytes=2-5"),
            if_none_match: Some("\"deadbeef.0-a\""),
            ..Default::default()
        };
        let response = serve_file(&path, headers).await.unwrap();

        assert_eq!(response.status(), 206);
        assert_eq!(body_bytes(response).await, b"2345");
    }

    #[tokio::test]
    async fn download_zip_query_beats_the_index_file() {
        let _lock = crate::zip_stream::ZIP_TEST_LOCK.lock().await;
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("index.html"), "<h1>home</h1>").unwrap();

        let query = QueryOptions {
            download: Some("zip".to_string()),
            picks: Vec::new(),
        };
        let response = handle_request(
            dir.path().to_path_buf(),
            "/",
            warp::http::Method::GET,
            false,
            RequestHeaders::default(),
            &query,
        )
        .await
        .unwrap();

        assert_eq!(response.status(), 200);
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "application/zip"
        );
        // Drain the body so the builder task shuts down cleanly.
        let _ = body_bytes(response).await;
    }

    #[tokio::test]
    async fn download_zip_on_a_hidden_dir_stays_blocked() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join(".secret")).unwrap();

        let query = QueryOptions {
            download: Some("zip".to_string()),
            picks: Vec::new(),
        };
        let result = handle_request(
            dir.path().to_path_buf(),
            "/.secret",
            warp::http::Method::GET,
            false,
            RequestHeaders::default(),
            &query,
        )
        .await;

        assert!(result.is_err(), "hidden dir must not be downloadable");
    }

    // The tests below drive the composed route through warp::test, pinning
    // the filter wiring (headers, query, method dispatch) that the
    // handler-level tests above bypass.

    fn routes_for(
        dir: &Path,
    ) -> impl Filter<Extract = (warp::reply::Response,), Error = warp::Rejection>
    + Clone
    + Send
    + Sync
    + 'static
    + use<> {
        build_routes(dir.to_path_buf(), false)
    }

    #[tokio::test]
    async fn route_get_without_query_string_serves_the_file() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("hello.txt"), "hello world").unwrap();

        let response = warp::test::request()
            .method("GET")
            .path("/hello.txt")
            .reply(&routes_for(dir.path()))
            .await;

        assert_eq!(response.status(), 200);
        assert_eq!(response.body().as_ref(), b"hello world");
    }

    #[tokio::test]
    async fn route_pre_epoch_mtime_serves_200_without_validators() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("old.txt");
        fs::write(&path, "ancient").unwrap();
        filetime::set_file_mtime(&path, filetime::FileTime::from_unix_time(-1_000, 0)).unwrap();

        let response = warp::test::request()
            .method("GET")
            .path("/old.txt")
            .reply(&routes_for(dir.path()))
            .await;

        assert_eq!(response.status(), 200);
        assert_eq!(response.body().as_ref(), b"ancient");
        assert!(
            response.headers().get("etag").is_none(),
            "a pre-epoch mtime cannot produce a validator"
        );
        assert!(response.headers().get("last-modified").is_none());
    }

    #[tokio::test]
    async fn route_range_request_answers_206_with_the_slice() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("data.txt"), "0123456789").unwrap();

        let response = warp::test::request()
            .method("GET")
            .path("/data.txt")
            .header("range", "bytes=2-5")
            .reply(&routes_for(dir.path()))
            .await;

        assert_eq!(response.status(), 206);
        assert_eq!(
            response.headers().get("content-range").unwrap(),
            "bytes 2-5/10"
        );
        assert_eq!(response.body().as_ref(), b"2345");
    }

    #[tokio::test]
    async fn route_head_matches_get_headers_with_an_empty_body() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("data.txt"), "0123456789").unwrap();
        let routes = routes_for(dir.path());

        let get = warp::test::request()
            .method("GET")
            .path("/data.txt")
            .reply(&routes)
            .await;
        let head = warp::test::request()
            .method("HEAD")
            .path("/data.txt")
            .reply(&routes)
            .await;

        assert_eq!(head.status(), 200);
        assert!(head.body().is_empty());
        for name in [
            "content-length",
            "content-type",
            "accept-ranges",
            "etag",
            "last-modified",
        ] {
            assert_eq!(head.headers().get(name), get.headers().get(name), "{name}");
        }
    }

    #[tokio::test]
    async fn route_head_with_range_reports_206_without_a_body() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("data.txt"), "0123456789").unwrap();

        let response = warp::test::request()
            .method("HEAD")
            .path("/data.txt")
            .header("range", "bytes=2-5")
            .reply(&routes_for(dir.path()))
            .await;

        assert_eq!(response.status(), 206);
        assert_eq!(
            response.headers().get("content-range").unwrap(),
            "bytes 2-5/10"
        );
        assert!(response.body().is_empty());
    }

    #[tokio::test]
    async fn route_post_answers_405_with_allow() {
        let dir = tempdir().unwrap();

        let response = warp::test::request()
            .method("POST")
            .path("/")
            .reply(&routes_for(dir.path()))
            .await;

        assert_eq!(response.status(), 405);
        assert_eq!(response.headers().get("allow").unwrap(), "GET, HEAD");
    }

    #[tokio::test]
    async fn route_if_range_with_current_etag_keeps_the_206() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("data.txt"), "0123456789").unwrap();
        let routes = routes_for(dir.path());

        let first = warp::test::request()
            .method("GET")
            .path("/data.txt")
            .reply(&routes)
            .await;
        let etag = first
            .headers()
            .get("etag")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        let response = warp::test::request()
            .method("GET")
            .path("/data.txt")
            .header("range", "bytes=2-5")
            .header("if-range", &etag)
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 206);
        assert_eq!(response.body().as_ref(), b"2345");
    }

    #[tokio::test]
    async fn route_if_range_with_stale_validator_downgrades_to_200() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("data.txt"), "0123456789").unwrap();

        let response = warp::test::request()
            .method("GET")
            .path("/data.txt")
            .header("range", "bytes=2-5")
            .header("if-range", "\"deadbeef.0-a\"")
            .reply(&routes_for(dir.path()))
            .await;

        assert_eq!(response.status(), 200);
        assert_eq!(response.body().as_ref(), b"0123456789");
    }

    #[tokio::test]
    async fn route_get_with_current_etag_answers_304() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("data.txt"), "0123456789").unwrap();
        let routes = routes_for(dir.path());

        let first = warp::test::request()
            .method("GET")
            .path("/data.txt")
            .reply(&routes)
            .await;
        let etag = first
            .headers()
            .get("etag")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        let response = warp::test::request()
            .method("GET")
            .path("/data.txt")
            .header("if-none-match", &etag)
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 304);
        assert!(response.body().is_empty());
        assert_eq!(response.headers().get("etag").unwrap(), etag.as_str());
    }

    #[tokio::test]
    async fn route_weak_if_none_match_answers_304_with_the_server_etag() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("data.txt"), "0123456789").unwrap();
        let routes = routes_for(dir.path());

        let first = warp::test::request()
            .method("GET")
            .path("/data.txt")
            .reply(&routes)
            .await;
        let etag = first
            .headers()
            .get("etag")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        let response = warp::test::request()
            .method("GET")
            .path("/data.txt")
            .header("if-none-match", format!("W/{etag}"))
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 304);
        // The 304 echoes the server's strong tag, never the client's bytes.
        assert_eq!(response.headers().get("etag").unwrap(), etag.as_str());
    }

    #[tokio::test]
    async fn route_get_with_current_last_modified_answers_304() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("data.txt"), "0123456789").unwrap();
        let routes = routes_for(dir.path());

        let first = warp::test::request()
            .method("GET")
            .path("/data.txt")
            .reply(&routes)
            .await;
        let last_modified = first
            .headers()
            .get("last-modified")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        let response = warp::test::request()
            .method("GET")
            .path("/data.txt")
            .header("if-modified-since", &last_modified)
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 304);
        assert!(response.body().is_empty());
        assert_eq!(
            response.headers().get("last-modified").unwrap(),
            last_modified.as_str()
        );
    }

    #[tokio::test]
    async fn route_head_with_current_etag_answers_304() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("data.txt"), "0123456789").unwrap();
        let routes = routes_for(dir.path());

        let first = warp::test::request()
            .method("GET")
            .path("/data.txt")
            .reply(&routes)
            .await;
        let etag = first
            .headers()
            .get("etag")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        let response = warp::test::request()
            .method("HEAD")
            .path("/data.txt")
            .header("if-none-match", &etag)
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 304);
        assert!(response.body().is_empty());
    }

    #[tokio::test]
    async fn route_garbage_if_modified_since_serves_the_full_file() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("data.txt"), "0123456789").unwrap();

        let response = warp::test::request()
            .method("GET")
            .path("/data.txt")
            .header("if-modified-since", "not-a-date")
            .reply(&routes_for(dir.path()))
            .await;

        assert_eq!(response.status(), 200);
        assert_eq!(response.body().as_ref(), b"0123456789");
    }

    #[tokio::test]
    async fn route_head_on_hidden_path_stays_404() {
        let dir = tempdir().unwrap();
        let hidden_dir = dir.path().join(".secret");
        fs::create_dir(&hidden_dir).unwrap();
        fs::write(hidden_dir.join("data.txt"), "secret").unwrap();

        let response = warp::test::request()
            .method("HEAD")
            .path("/.secret/data.txt")
            .reply(&routes_for(dir.path()))
            .await;

        assert_eq!(response.status(), 404);
    }

    #[tokio::test]
    async fn route_head_traversal_stays_404() {
        let parent = tempdir().unwrap();
        let root = parent.path().join("root");
        fs::create_dir(&root).unwrap();
        fs::write(parent.path().join("outside.txt"), "outside").unwrap();

        let response = warp::test::request()
            .method("HEAD")
            .path("/../outside.txt")
            .reply(&routes_for(&root))
            .await;

        assert_eq!(response.status(), 404);
    }

    #[tokio::test]
    async fn route_head_on_hidden_zip_stays_404() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join(".secret")).unwrap();

        let response = warp::test::request()
            .method("HEAD")
            .path("/.secret?download=zip")
            .reply(&routes_for(dir.path()))
            .await;

        assert_eq!(response.status(), 404);
    }

    #[tokio::test]
    async fn route_get_zip_streams_an_archive() {
        let _lock = crate::zip_stream::ZIP_TEST_LOCK.lock().await;
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "alpha").unwrap();

        let response = warp::test::request()
            .method("GET")
            .path("/?download=zip")
            .reply(&routes_for(dir.path()))
            .await;

        assert_eq!(response.status(), 200);
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "application/zip"
        );
        assert!(!response.body().is_empty());
    }

    #[tokio::test]
    async fn route_head_zip_answers_headers_without_building() {
        let _lock = crate::zip_stream::ZIP_TEST_LOCK.lock().await;
        let dir = tempdir().unwrap();
        let routes = routes_for(dir.path());

        // The GET here is only the header-parity baseline; HEAD itself must
        // not take a build slot, which the unit test on serve_directory_zip
        // pins by running without the lock.
        let get = warp::test::request()
            .method("GET")
            .path("/?download=zip")
            .reply(&routes)
            .await;
        let head = warp::test::request()
            .method("HEAD")
            .path("/?download=zip")
            .reply(&routes)
            .await;

        assert_eq!(head.status(), 200);
        for name in ["content-type", "content-disposition"] {
            assert_eq!(head.headers().get(name), get.headers().get(name), "{name}");
        }
        assert!(head.body().is_empty());
    }

    #[tokio::test]
    async fn route_directory_listing_carries_content_length_for_get_and_head() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("file.txt"), "x").unwrap();
        let routes = routes_for(dir.path());

        let get = warp::test::request()
            .method("GET")
            .path("/")
            .reply(&routes)
            .await;
        let head = warp::test::request()
            .method("HEAD")
            .path("/")
            .reply(&routes)
            .await;

        let content_length = get
            .headers()
            .get("content-length")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert_eq!(content_length, get.body().len().to_string());
        assert_eq!(
            head.headers().get("content-length").unwrap(),
            &content_length
        );
        assert!(head.body().is_empty());
    }

    // Selection downloads: query parsing, pick validation, markup, and the
    // composed route.

    fn zip_entry_names(bytes: &[u8]) -> Vec<String> {
        let archive = zip::ZipArchive::new(std::io::Cursor::new(bytes.to_vec())).unwrap();
        archive.file_names().map(|name| name.to_string()).collect()
    }

    #[test]
    fn parse_query_collects_repeated_picks_in_order() {
        let query = parse_query("download=zip&pick=a.txt&pick=sub&pick=b%20c").unwrap();

        assert_eq!(query.download.as_deref(), Some("zip"));
        assert_eq!(query.picks, vec!["a.txt", "sub", "b c"]);
    }

    #[test]
    fn parse_query_download_keeps_the_last_occurrence() {
        let query = parse_query("download=tar&download=zip").unwrap();

        assert_eq!(query.download.as_deref(), Some("zip"));
    }

    #[test]
    fn parse_query_matches_warps_decoder_semantics() {
        // A bare key maps to an empty value.
        let query = parse_query("download").unwrap();
        assert_eq!(query.download.as_deref(), Some(""));

        // A plus decodes to a space; %2B decodes to a literal plus.
        let query = parse_query("pick=a+b&pick=a%2Bb").unwrap();
        assert_eq!(query.picks, vec!["a b", "a+b"]);

        // The decoder passes malformed escapes through literally and decodes
        // invalid UTF-8 lossily; neither rejects.
        assert!(parse_query("junk=%zz").is_ok());
        assert!(parse_query("junk=%FF").is_ok());

        // Unknown keys are ignored.
        assert_eq!(parse_query("foo=bar").unwrap(), QueryOptions::default());
    }

    #[test]
    fn resolve_pick_rejects_malformed_names() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "x").unwrap();
        let listed = dir.path().canonicalize().unwrap();

        for pick in ["", ".", "..", "a/b", "a\\b", "../a.txt", "/etc/hosts"] {
            assert!(
                resolve_pick(&listed, pick, true).is_none(),
                "pick {pick:?} must be rejected"
            );
        }
    }

    #[test]
    fn resolve_pick_screens_hidden_names_by_flag() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join(".hidden"), "h").unwrap();
        let listed = dir.path().canonicalize().unwrap();

        assert!(resolve_pick(&listed, ".hidden", false).is_none());
        assert!(resolve_pick(&listed, ".hidden", true).is_some());
    }

    #[test]
    fn resolve_pick_requires_existence_and_returns_canonical_paths() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "x").unwrap();
        let listed = dir.path().canonicalize().unwrap();

        assert_eq!(
            resolve_pick(&listed, "a.txt", false),
            Some(listed.join("a.txt"))
        );
        assert!(resolve_pick(&listed, "missing.txt", false).is_none());
    }

    #[cfg(windows)]
    #[test]
    fn resolve_pick_rejects_drive_relative_names() {
        let dir = tempdir().unwrap();
        let listed = dir.path().canonicalize().unwrap();

        assert!(resolve_pick(&listed, "C:name.txt", false).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn resolve_pick_rejects_symlinks() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("real.txt"), "r").unwrap();
        std::os::unix::fs::symlink(dir.path().join("real.txt"), dir.path().join("link.txt"))
            .unwrap();
        let listed = dir.path().canonicalize().unwrap();

        assert!(resolve_pick(&listed, "link.txt", false).is_none());
        assert!(resolve_pick(&listed, "real.txt", false).is_some());
    }

    #[test]
    fn validate_picks_dedupes_and_caps() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "x").unwrap();
        let listed = dir.path().canonicalize().unwrap();

        let picks = vec!["a.txt".to_string(), "a.txt".to_string()];
        let roots = validate_picks(&listed, &picks, false).unwrap();
        assert_eq!(roots, vec![listed.join("a.txt")]);

        let too_many: Vec<String> = (0..=MAX_PICKS).map(|i| format!("f{i}")).collect();
        assert!(validate_picks(&listed, &too_many, false).is_err());
    }

    #[test]
    fn validate_picks_fails_whole_on_any_invalid_pick() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "x").unwrap();
        let listed = dir.path().canonicalize().unwrap();

        let picks = vec!["a.txt".to_string(), "missing.txt".to_string()];
        assert!(validate_picks(&listed, &picks, false).is_err());
    }

    #[test]
    fn test_render_listing_form_carries_pick_checkboxes() {
        let dirs = vec![("sub".to_string(), "/sub".to_string())];
        let files = vec![("f.txt".to_string(), "/f.txt".to_string(), 1)];

        let html = render_directory_listing("/sub", &dirs, &files);

        assert!(html.contains(r#"<form method="get">"#));
        assert!(html.contains(r#"<input type="checkbox" name="pick" value="sub">"#));
        assert!(html.contains(r#"<input type="checkbox" name="pick" value="f.txt">"#));
        assert!(html.contains(r#"<button type="submit" name="download" value="zip""#));
        // Two entries, two pick checkboxes: the parent ".." row is not
        // selectable, and the select-all control is not a pick.
        assert_eq!(
            html.matches(r#"<input type="checkbox" name="pick""#)
                .count(),
            2
        );
    }

    #[test]
    fn test_render_listing_select_all_is_nameless_hidden_and_static() {
        let files = vec![(r#"a"b&c.txt"#.to_string(), "/a%22b%26c.txt".to_string(), 1)];
        let html = render_directory_listing("/", &[], &files);

        // Nameless, so it can never submit as a form field; hidden until the
        // script reveals it, so without JavaScript it never shows.
        assert!(html.contains(
            r#"<th class="select"><input type="checkbox" id="pick-all" title="Select all" hidden></th>"#
        ));

        // The page's one script is a fixed string: a listing with different
        // (hostile) entry names renders byte-identical script content, so no
        // entry name can reach a script context.
        let script_of = |page: &str| {
            let start = page.find("<script>").unwrap();
            let end = page.find("</script>").unwrap();
            page[start..end].to_string()
        };
        let dirs = vec![(r#"<sub>'"x"#.to_string(), "/%3Csub%3E'%22x".to_string())];
        let other = render_directory_listing("/nested", &dirs, &[]);
        assert_eq!(script_of(&html), script_of(&other));
        assert_eq!(html.matches("<script>").count(), 1);
    }

    #[test]
    fn test_render_listing_escapes_checkbox_values_without_preencoding() {
        let name = r#"a"b&c.txt"#;
        let files = vec![(name.to_string(), "/a%22b&c.txt".to_string(), 1)];

        let html = render_directory_listing("/", &[], &files);

        // The value is the raw name, attribute-escaped; the browser
        // form-encodes it on submit, so pre-encoding would double-encode.
        assert!(html.contains(r#"value="a&quot;b&amp;c.txt""#));
        assert!(!html.contains(r#"value="a"b"#));
    }

    #[tokio::test]
    async fn route_selection_zip_archives_exactly_the_picks() {
        let _lock = crate::zip_stream::ZIP_TEST_LOCK.lock().await;
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("picked.txt"), "p").unwrap();
        fs::write(dir.path().join("unpicked.txt"), "u").unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("sub").join("inner.txt"), "i").unwrap();

        let response = warp::test::request()
            .method("GET")
            .path("/?download=zip&pick=picked.txt&pick=sub")
            .reply(&routes_for(dir.path()))
            .await;

        assert_eq!(response.status(), 200);
        let disposition = response
            .headers()
            .get("content-disposition")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(disposition.contains("-selection.zip"));
        let mut names = zip_entry_names(response.body());
        names.sort();
        assert_eq!(names, vec!["picked.txt", "sub/inner.txt"]);
    }

    #[tokio::test]
    async fn route_selection_round_trips_special_characters_in_names() {
        let _lock = crate::zip_stream::ZIP_TEST_LOCK.lock().await;
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a&b c.txt"), "x").unwrap();

        // The pick arrives form-encoded, as a browser submits the checkbox
        // value: `&` percent-encoded, the space as `+`.
        let response = warp::test::request()
            .method("GET")
            .path("/?download=zip&pick=a%26b+c.txt")
            .reply(&routes_for(dir.path()))
            .await;

        assert_eq!(response.status(), 200);
        assert_eq!(zip_entry_names(response.body()), vec!["a&b c.txt"]);
    }

    #[tokio::test]
    async fn route_selection_with_any_invalid_pick_answers_404_not_a_full_zip() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "x").unwrap();
        let routes = routes_for(dir.path());

        for path in [
            "/?download=zip&pick=missing.txt",
            "/?download=zip&pick=a.txt&pick=missing.txt",
            "/?download=zip&pick=../a.txt",
            "/?download=zip&pick=sub%2Finner.txt",
            "/?download=zip&pick=%2Fetc%2Fhosts",
            "/?download=zip&pick=.",
            "/?download=zip&pick=",
        ] {
            let response = warp::test::request()
                .method("GET")
                .path(path)
                .reply(&routes)
                .await;
            assert_eq!(response.status(), 404, "{path}");
        }
    }

    #[tokio::test]
    async fn route_hidden_pick_is_404_by_default_and_served_with_the_flag() {
        let _lock = crate::zip_stream::ZIP_TEST_LOCK.lock().await;
        let dir = tempdir().unwrap();
        fs::write(dir.path().join(".hidden"), "h").unwrap();

        let blocked = warp::test::request()
            .method("GET")
            .path("/?download=zip&pick=.hidden")
            .reply(&routes_for(dir.path()))
            .await;
        assert_eq!(blocked.status(), 404);

        let allowed = warp::test::request()
            .method("GET")
            .path("/?download=zip&pick=.hidden")
            .reply(&build_routes(dir.path().to_path_buf(), true))
            .await;
        assert_eq!(allowed.status(), 200);
        assert_eq!(zip_entry_names(allowed.body()), vec![".hidden"]);
    }

    #[tokio::test]
    async fn route_pick_without_download_serves_the_listing() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "x").unwrap();

        let response = warp::test::request()
            .method("GET")
            .path("/?pick=a.txt")
            .reply(&routes_for(dir.path()))
            .await;

        assert_eq!(response.status(), 200);
        let content_type = response
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(content_type.starts_with("text/html"));
    }

    #[tokio::test]
    async fn route_head_selection_validates_picks_and_answers_headers_only() {
        // No zip lock: a selection HEAD takes no build slot.
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "x").unwrap();
        let routes = routes_for(dir.path());

        let ok = warp::test::request()
            .method("HEAD")
            .path("/?download=zip&pick=a.txt")
            .reply(&routes)
            .await;
        assert_eq!(ok.status(), 200);
        assert!(ok.body().is_empty());
        let disposition = ok
            .headers()
            .get("content-disposition")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(disposition.contains("-selection.zip"));

        let bad = warp::test::request()
            .method("HEAD")
            .path("/?download=zip&pick=missing.txt")
            .reply(&routes)
            .await;
        assert_eq!(bad.status(), 404);
    }
}

use crate::conditional::{if_range_allows, not_modified, strong_etag};
use crate::models::{DirEntry, FileEntry, ServerConfig};
use crate::range::{parse_byte_range, RangeOutcome};
use crate::zip_stream::serve_directory_zip;
use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, CONTROLS};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;
use tracing::{debug, error, info, warn};
use warp::http::header::{
    HeaderValue, ACCEPT_RANGES, ALLOW, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, ETAG,
    LAST_MODIFIED,
};
use warp::http::StatusCode;
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
        .and(warp::query::<HashMap<String, String>>())
        .and_then(
            move |path: warp::path::FullPath,
                  method: warp::http::Method,
                  range: Option<String>,
                  if_range: Option<String>,
                  if_none_match: Option<String>,
                  if_modified_since: Option<String>,
                  query: HashMap<String, String>| {
                let root_path = root_path.clone();
                async move {
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

    println!("Server running at http://{}", addr);

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
    query: &HashMap<String, String>,
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
    query: &HashMap<String, String>,
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

    // Block access to hidden files/directories unless explicitly enabled
    if !serve_hidden && contains_hidden_segment(relative_path) {
        return Err(warp::reject::not_found());
    }

    if !canonical_file_path.exists() {
        return Err(warp::reject::not_found());
    }

    if canonical_file_path.is_file() {
        // Serve the file
        serve_file(&canonical_file_path, headers).await
    } else if canonical_file_path.is_dir() {
        // An explicit download request wins over index files, so a directory
        // carrying an index.html can still be fetched as an archive.
        if query.get("download").map(String::as_str) == Some("zip") {
            return serve_directory_zip(&canonical_file_path, serve_hidden, head_only).await;
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

    if span.start != 0 {
        if let Err(error) = file.seek(std::io::SeekFrom::Start(span.start)).await {
            warn!(path = %file_path.display(), error = %error, "Failed to seek to the requested range");
            return Err(warp::reject::not_found());
        }
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
    while let Some(entry) = entries.next_entry().await.unwrap_or(None) {
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
/// before interpolation.
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
    </style>
</head>
<body>
    <div class="container">
        <h1>{}</h1>
        <table>
            <thead>
                <tr>
                    <th>Name</th>
                    <th>Type</th>
                    <th>Size</th>
                </tr>
            </thead>
            <tbody>"#,
        title, title
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
                <td><a href="{}" class="directory">[DIR] ..</a></td>
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
                <td><a href="{}" class="directory">[DIR] {}</a></td>
                <td>Directory</td>
                <td>-</td>
            </tr>"#,
            html_escape(path),
            html_escape(name)
        ));
    }

    // Add files
    for (name, path, size) in files {
        let size_str = format_file_size(*size);
        html.push_str(&format!(
            r#"<tr>
                <td><a href="{}" class="file">[FILE] {}</a></td>
                <td>File</td>
                <td class="size">{}</td>
            </tr>"#,
            html_escape(path),
            html_escape(name),
            size_str
        ));
    }

    html.push_str(
        r#"        </tbody>
        </table>
    </div>
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
            &HashMap::new(),
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
            &HashMap::new(),
        )
        .await;

        assert!(result.is_ok(), "hidden file should be served when enabled");
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
            &HashMap::new(),
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
            &HashMap::new(),
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
            &HashMap::new(),
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
            &HashMap::new(),
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
            &HashMap::new(),
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
            &HashMap::new(),
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

        assert!(html.contains(r#"<a href="/" class="directory">[DIR] ..</a>"#));
    }

    #[test]
    fn parent_link_from_nested_path_points_to_parent() {
        let html = render_directory_listing("/a/b", &[], &[]);

        assert!(html.contains(r#"<a href="/a" class="directory">[DIR] ..</a>"#));
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
        assert!(!html.contains("<script>"));
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
    fn test_render_listing_uses_plain_text_markers() {
        let dirs = vec![("sub".to_string(), "/sub".to_string())];
        let files = vec![("f.txt".to_string(), "/f.txt".to_string(), 1)];

        let root_html = render_directory_listing("/", &dirs, &files);
        assert!(root_html.contains("[DIR] sub"));
        assert!(root_html.contains("[FILE] f.txt"));
        assert!(
            !root_html.contains("[DIR] .."),
            "the root has no parent link"
        );

        let nested_html = render_directory_listing("/sub", &[], &[]);
        assert!(nested_html.contains("[DIR] .."));
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

        let query: HashMap<String, String> =
            HashMap::from([("download".to_string(), "zip".to_string())]);
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

        let query: HashMap<String, String> =
            HashMap::from([("download".to_string(), "zip".to_string())]);
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
           + 'static {
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
}

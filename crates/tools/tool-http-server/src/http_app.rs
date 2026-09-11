use crate::models::{DirEntry, FileEntry, ServerConfig};
use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, CONTROLS};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use tracing::{error, info};
use warp::{Filter, Reply};

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

pub async fn start_server(config: ServerConfig) {
    let root_path = config.root_path.clone();
    let serve_hidden = config.serve_hidden;

    // Create a filter for logging requests
    let log_filter = create_request_logger();

    // Create the main route handler
    let routes = warp::path::full()
        .and(warp::method())
        .and_then(
            move |path: warp::path::FullPath, method: warp::http::Method| {
                let root_path = root_path.clone();
                async move { handle_request(root_path, path.as_str(), method, serve_hidden).await }
            },
        )
        .with(log_filter);

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
) -> Result<warp::reply::Response, warp::Rejection> {
    if method != warp::http::Method::GET {
        return Ok(warp::reply::with_status(
            "Method not allowed",
            warp::http::StatusCode::METHOD_NOT_ALLOWED,
        )
        .into_response());
    }

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
        serve_file(&canonical_file_path).await
    } else if canonical_file_path.is_dir() {
        // Check for index files
        let index_files = ["index.html", "index.htm"];
        for index_file in &index_files {
            let index_path = canonical_file_path.join(index_file);
            if index_path.exists() && index_path.is_file() {
                return serve_file(&index_path).await;
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

async fn serve_file(file_path: &Path) -> Result<warp::reply::Response, warp::Rejection> {
    let contents = match tokio::fs::read(file_path).await {
        Ok(contents) => contents,
        Err(_) => return Err(warp::reject::not_found()),
    };

    let mime_type = mime_guess::from_path(file_path)
        .first_or_octet_stream()
        .to_string();

    Ok(warp::reply::with_header(contents, "content-type", mime_type).into_response())
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

    Ok(warp::reply::with_header(html, "content-type", "text/html; charset=utf-8").into_response())
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
        )
        .await;

        assert!(result.is_ok()); // Returns 405, not a rejection
        let response = result.unwrap();
        assert_eq!(response.status(), 405);
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

        let result = handle_request(root, "/../outside.txt", warp::http::Method::GET, false).await;

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
}

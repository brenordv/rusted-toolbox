use crate::models::{DirEntry, FileEntry, ServerArgs};
use percent_encoding::percent_decode_str;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use tracing::info;
use warp::{Filter, Reply};

pub async fn start_server(config: ServerArgs) {
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

    warp::serve(routes).run(addr).await;
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
    let contents = match fs::read(file_path) {
        Ok(contents) => contents,
        Err(_) => return Err(warp::reject::not_found()),
    };

    let mime_type = mime_guess::from_path(file_path)
        .first_or_octet_stream()
        .to_string();

    Ok(warp::reply::with_header(contents, "content-type", mime_type).into_response())
}

/// Collects directory entries, optionally including hidden files/directories.
///
/// Returns `(directories, files)` where directories are `(name, relative_path)` tuples
/// and files are `(name, relative_path, size)` tuples, both sorted alphabetically by name.
fn collect_directory_entries(
    dir_path: &Path,
    request_path: &str,
    serve_hidden: bool,
) -> std::io::Result<(Vec<DirEntry>, Vec<FileEntry>)> {
    let entries = fs::read_dir(dir_path)?;

    let mut files = Vec::new();
    let mut directories = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_string();

        // Skip hidden files and directories unless explicitly enabled
        if !serve_hidden && file_name.starts_with('.') {
            continue;
        }

        let relative_path = if request_path.ends_with('/') || request_path.is_empty() {
            format!(
                "{}{}",
                if request_path == "/" {
                    ""
                } else {
                    request_path
                },
                file_name
            )
        } else {
            format!("{}/{}", request_path, file_name)
        };

        if path.is_dir() {
            directories.push((file_name, relative_path));
        } else {
            let size = path.metadata().map(|m| m.len()).unwrap_or(0);
            files.push((file_name, relative_path, size));
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
        .map_err(|_| warp::reject::not_found())?;

    // Generate HTML
    let title = if request_path == "/" || request_path.is_empty() {
        "Index of /".to_string()
    } else {
        format!("Index of {}", request_path)
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

    // Add a parent directory link if not at the root
    if request_path != "/" && !request_path.is_empty() {
        let parent_path = if request_path.contains('/') {
            let mut parts: Vec<&str> = request_path.split('/').collect();
            parts.pop();
            if parts.len() <= 1 {
                "/".to_string()
            } else {
                parts.join("/")
            }
        } else {
            "/".to_string()
        };

        html.push_str(&format!(
            r#"<tr>
                <td><a href="{}" class="directory">📁 ..</a></td>
                <td>Directory</td>
                <td>-</td>
            </tr>"#,
            parent_path
        ));
    }

    // Add directories
    for (name, path) in directories {
        html.push_str(&format!(
            r#"<tr>
                <td><a href="{}" class="directory">📁 {}</a></td>
                <td>Directory</td>
                <td>-</td>
            </tr>"#,
            path, name
        ));
    }

    // Add files
    for (name, path, size) in files {
        let size_str = format_file_size(size);
        html.push_str(&format!(
            r#"<tr>
                <td><a href="{}" class="file">📄 {}</a></td>
                <td>File</td>
                <td class="size">{}</td>
            </tr>"#,
            path, name, size_str
        ));
    }

    html.push_str(
        r#"        </tbody>
        </table>
    </div>
</body>
</html>"#,
    );

    Ok(warp::reply::with_header(html, "content-type", "text/html; charset=utf-8").into_response())
}

/// Returns `true` if any segment of the given relative path starts with a dot,
/// indicating a hidden file or directory.
fn contains_hidden_segment(path: &str) -> bool {
    path.split('/').any(|s| s.starts_with('.'))
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

    #[test]
    fn test_collect_entries_hides_dotfiles_by_default() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("visible.txt"), "hi").unwrap();
        fs::write(dir.path().join(".hidden"), "secret").unwrap();
        fs::create_dir(dir.path().join(".secret_dir")).unwrap();
        fs::create_dir(dir.path().join("public_dir")).unwrap();

        let (dirs, files) = collect_directory_entries(dir.path(), "/", false).unwrap();

        let file_names: Vec<&str> = files.iter().map(|f| f.0.as_str()).collect();
        let dir_names: Vec<&str> = dirs.iter().map(|d| d.0.as_str()).collect();

        assert!(file_names.contains(&"visible.txt"));
        assert!(!file_names.contains(&".hidden"));
        assert!(dir_names.contains(&"public_dir"));
        assert!(!dir_names.contains(&".secret_dir"));
    }

    #[test]
    fn test_collect_entries_shows_dotfiles_when_enabled() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("visible.txt"), "hi").unwrap();
        fs::write(dir.path().join(".hidden"), "secret").unwrap();
        fs::create_dir(dir.path().join(".secret_dir")).unwrap();
        fs::create_dir(dir.path().join("public_dir")).unwrap();

        let (dirs, files) = collect_directory_entries(dir.path(), "/", true).unwrap();

        let file_names: Vec<&str> = files.iter().map(|f| f.0.as_str()).collect();
        let dir_names: Vec<&str> = dirs.iter().map(|d| d.0.as_str()).collect();

        assert!(file_names.contains(&"visible.txt"));
        assert!(file_names.contains(&".hidden"));
        assert!(dir_names.contains(&"public_dir"));
        assert!(dir_names.contains(&".secret_dir"));
    }

    #[test]
    fn test_collect_entries_sorted_alphabetically() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("zebra.txt"), "z").unwrap();
        fs::write(dir.path().join("apple.txt"), "a").unwrap();
        fs::create_dir(dir.path().join("beta")).unwrap();
        fs::create_dir(dir.path().join("alpha")).unwrap();

        let (dirs, files) = collect_directory_entries(dir.path(), "/", false).unwrap();

        assert_eq!(dirs[0].0, "alpha");
        assert_eq!(dirs[1].0, "beta");
        assert_eq!(files[0].0, "apple.txt");
        assert_eq!(files[1].0, "zebra.txt");
    }
}

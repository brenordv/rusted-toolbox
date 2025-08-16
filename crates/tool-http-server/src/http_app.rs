use crate::models::ServerArgs;
use chrono::Utc;
use percent_encoding::percent_decode_str;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use warp::{Filter, Reply};

pub async fn start_server(config: ServerArgs) {
    let root_path = config.root_path.clone();

    // Create a filter for logging requests
    let log_filter = warp::log::custom(|info| {
        println!(
            "{} {} {} {} - {} bytes",
            Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
            info.method(),
            info.path(),
            info.status(),
            info.elapsed().as_millis(),
        );
    });

    // Create the main route handler
    let routes = warp::path::full()
        .and(warp::method())
        .and_then(
            move |path: warp::path::FullPath, method: warp::http::Method| {
                let root_path = root_path.clone();
                async move { handle_request(root_path, path.as_str(), method).await }
            },
        )
        .with(log_filter);

    let addr: SocketAddr = ([127, 0, 0, 1], config.port).into();

    println!("Server running at http://{}", addr);

    warp::serve(routes).run(addr).await;
}

async fn handle_request(
    root_path: PathBuf,
    request_path: &str,
    method: warp::http::Method,
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

    // Remove leading slash and resolve path
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
        serve_directory_listing(&canonical_file_path, &canonical_root_path, request_path).await
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

async fn serve_directory_listing(
    dir_path: &Path,
    root_path: &Path,
    request_path: &str,
) -> Result<warp::reply::Response, warp::Rejection> {
    let entries = match fs::read_dir(dir_path) {
        Ok(entries) => entries,
        Err(_) => return Err(warp::reject::not_found()),
    };

    let mut files = Vec::new();
    let mut directories = Vec::new();

    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            let file_name = entry.file_name().to_string_lossy().to_string();

            // Skip hidden files and directories
            if file_name.starts_with('.') {
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
    }

    // Sort entries
    directories.sort_by(|a, b| a.0.cmp(&b.0));
    files.sort_by(|a, b| a.0.cmp(&b.0));

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

    // Add parent directory link if not at root
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

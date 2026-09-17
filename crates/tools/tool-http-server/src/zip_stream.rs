use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use tokio::sync::mpsc;
use tokio::sync::Semaphore;
use tracing::{debug, warn};
use walkdir::WalkDir;
use warp::http::header::{HeaderValue, CONTENT_DISPOSITION, CONTENT_TYPE, RETRY_AFTER};
use warp::http::StatusCode;
use warp::Reply;
use zip::result::ZipError;
use zip::write::{SimpleFileOptions, StreamWriter};
use zip::ZipWriter;

/// Upper bound on concurrently running archive builders; each one occupies a
/// blocking-pool thread for the archive's whole duration.
const MAX_CONCURRENT_ZIPS: usize = 4;

/// Chunks buffered between the archive builder and the HTTP response body;
/// a slow client backpressures the builder through this bound.
const CHANNEL_CAPACITY: usize = 8;

/// Write coalescing for the zip writer's many small header/data writes, so
/// the channel carries fewer, larger chunks.
const WRITE_BUFFER_SIZE: usize = 64 * 1024;

static ZIP_SLOTS: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(MAX_CONCURRENT_ZIPS));

/// Serializes zip-draining tests across the crate so they cannot exhaust the
/// shared zip-slot semaphore when the suite runs in parallel. Async-aware,
/// since the guard lives across await points.
#[cfg(test)]
pub(crate) static ZIP_TEST_LOCK: LazyLock<tokio::sync::Mutex<()>> =
    LazyLock::new(|| tokio::sync::Mutex::new(()));

/// Characters percent-encoded in the RFC 8187 `filename*` ext-value: every
/// byte outside the attr-char set (ALPHA / DIGIT / `!#$&+-.^_` backtick
/// `|~`); non-ASCII bytes are always encoded.
const EXT_VALUE_ENCODE: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'!')
    .remove(b'#')
    .remove(b'$')
    .remove(b'&')
    .remove(b'+')
    .remove(b'-')
    .remove(b'.')
    .remove(b'^')
    .remove(b'_')
    .remove(b'`')
    .remove(b'|')
    .remove(b'~');

/// Streams `dir_path` as a zip archive. The archive is produced on a
/// blocking-pool thread and flows to the client through a bounded channel, so
/// no more than a few chunks are ever in memory. Symlinks are not followed
/// and not archived; entries that cannot be read before their local header is
/// written are skipped with a warning; a failure after that point aborts the
/// connection, the only signal left once the 200 has gone out.
///
/// With `head_only` the response is built from the headers alone: no build
/// slot is taken and no builder thread spawns, so probes are free. The
/// trade-off is that a HEAD reports 200 even while GET traffic is being
/// told 503.
pub async fn serve_directory_zip(
    dir_path: &Path,
    serve_hidden: bool,
    head_only: bool,
) -> Result<warp::reply::Response, warp::Rejection> {
    let roots = vec![dir_path.to_path_buf()];
    serve_zip(dir_path, roots, serve_hidden, head_only, "").await
}

/// Streams a selection of `base`'s entries as a zip archive. `roots` are the
/// already-validated canonical paths of the picked entries; a picked file
/// becomes one entry, a picked directory is archived recursively, and entry
/// names stay relative to `base`, exactly as the whole-directory form names
/// them. The archive downloads as `<dirname>-selection.zip`. Streaming,
/// concurrency slots, and HEAD behavior are those of [`serve_directory_zip`].
pub async fn serve_selection_zip(
    base: &Path,
    roots: Vec<PathBuf>,
    serve_hidden: bool,
    head_only: bool,
) -> Result<warp::reply::Response, warp::Rejection> {
    serve_zip(base, roots, serve_hidden, head_only, "-selection").await
}

async fn serve_zip(
    base: &Path,
    roots: Vec<PathBuf>,
    serve_hidden: bool,
    head_only: bool,
    name_suffix: &'static str,
) -> Result<warp::reply::Response, warp::Rejection> {
    if head_only {
        let mut response = warp::reply::reply().into_response();
        response
            .headers_mut()
            .insert(CONTENT_TYPE, HeaderValue::from_static("application/zip"));
        response.headers_mut().insert(
            CONTENT_DISPOSITION,
            content_disposition_for(base, name_suffix),
        );
        return Ok(response);
    }

    let permit = match ZIP_SLOTS.try_acquire() {
        Ok(permit) => permit,
        Err(_) => {
            debug!(dir = %base.display(), "Zip download slots exhausted; answering 503");
            let mut response = warp::reply::with_status(
                "zip download limit reached, try again later",
                StatusCode::SERVICE_UNAVAILABLE,
            )
            .into_response();
            response
                .headers_mut()
                .insert(RETRY_AFTER, HeaderValue::from_static("10"));
            return Ok(response);
        }
    };

    let (tx, mut rx) = mpsc::channel::<io::Result<Vec<u8>>>(CHANNEL_CAPACITY);
    let base_owned = base.to_path_buf();

    tokio::task::spawn_blocking(move || {
        // The permit rides with the builder so a slot frees only when the
        // archive is done (or the client is gone), not at handler return.
        let _permit = permit;
        build_zip(&base_owned, &roots, serve_hidden, &tx);
    });

    let body = futures_util::stream::poll_fn(move |cx| rx.poll_recv(cx));
    let mut response = warp::reply::stream(body).into_response();
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("application/zip"));
    response.headers_mut().insert(
        CONTENT_DISPOSITION,
        content_disposition_for(base, name_suffix),
    );

    Ok(response)
}

/// Sends each written chunk to the response-body channel. A dropped receiver
/// (the client disconnected) surfaces as `BrokenPipe`, which unwinds the
/// whole archive build.
struct ChannelWriter {
    tx: mpsc::Sender<io::Result<Vec<u8>>>,
}

impl Write for ChannelWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        self.tx.blocking_send(Ok(buf.to_vec())).map_err(|_| {
            io::Error::new(io::ErrorKind::BrokenPipe, "response body receiver dropped")
        })?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn build_zip(
    base: &Path,
    roots: &[PathBuf],
    serve_hidden: bool,
    tx: &mpsc::Sender<io::Result<Vec<u8>>>,
) {
    debug!(dir = %base.display(), roots = roots.len(), "Building zip archive");
    if let Err(error) = write_archive(base, roots, serve_hidden, tx) {
        if error.kind() == io::ErrorKind::BrokenPipe {
            debug!(dir = %base.display(), roots = roots.len(), "Client disconnected during zip stream");
            return;
        }
        warn!(dir = %base.display(), roots = roots.len(), error = %error, "Zip production failed");
        // The 200 already went out; erroring the body stream aborts the
        // connection so the client at least sees a truncated transfer.
        let _ = tx.blocking_send(Err(error));
    }
}

type StreamArchive = ZipWriter<StreamWriter<BufWriter<ChannelWriter>>>;

fn write_archive(
    base: &Path,
    roots: &[PathBuf],
    serve_hidden: bool,
    tx: &mpsc::Sender<io::Result<Vec<u8>>>,
) -> io::Result<()> {
    let writer = BufWriter::with_capacity(WRITE_BUFFER_SIZE, ChannelWriter { tx: tx.clone() });
    let mut archive = ZipWriter::new_stream(writer);
    // large_file keeps zip64 on from the first byte: without it, an entry
    // crossing 4 GiB errors after start_file, which aborts the whole stream.
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .large_file(true);

    for root in roots {
        // lstat, not stat: a root that has become a symlink since validation
        // must not be followed, matching the walk below, which never records
        // symlinks either.
        let metadata = match std::fs::symlink_metadata(root) {
            Ok(metadata) => metadata,
            Err(error) => {
                warn!(dir = %base.display(), entry = %root.display(), error = %error, "Skipping unreadable entry while building zip");
                continue;
            }
        };

        if metadata.is_file() {
            write_file_entry(&mut archive, base, root, options)?;
            continue;
        }
        if !metadata.is_dir() {
            warn!(dir = %base.display(), entry = %root.display(), "Skipping non-regular entry while building zip");
            continue;
        }

        let walker = WalkDir::new(root)
            .sort_by_file_name()
            .into_iter()
            .filter_entry(|entry| {
                serve_hidden
                    || entry.depth() == 0
                    || !entry.file_name().to_string_lossy().starts_with('.')
            });

        for entry in walker {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    warn!(dir = %base.display(), error = %error, "Skipping unreadable entry while building zip");
                    continue;
                }
            };
            // Symlinks (and anything else that is not a plain file) stay out
            // of the archive, so a link cannot smuggle content from outside
            // the served tree.
            if !entry.file_type().is_file() {
                continue;
            }
            write_file_entry(&mut archive, base, entry.path(), options)?;
        }
    }

    let mut inner = archive.finish().map_err(zip_to_io_error)?;
    inner.flush()
}

/// Writes one file into the archive under its `base`-relative name. A file
/// that cannot be opened is skipped with a warning, which is only safe before
/// its local header is written; from `start_file` on, any failure aborts the
/// stream, so those errors propagate.
fn write_file_entry(
    archive: &mut StreamArchive,
    base: &Path,
    path: &Path,
    options: SimpleFileOptions,
) -> io::Result<()> {
    let Some(entry_name) = archive_entry_name(base, path) else {
        return Ok(());
    };
    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) => {
            warn!(dir = %base.display(), entry = %path.display(), error = %error, "Skipping unreadable entry while building zip");
            return Ok(());
        }
    };

    archive
        .start_file(&entry_name, options)
        .map_err(zip_to_io_error)?;
    io::copy(&mut file, archive)?;
    Ok(())
}

fn zip_to_io_error(error: ZipError) -> io::Error {
    match error {
        ZipError::Io(io_error) => io_error,
        other => io::Error::other(other),
    }
}

/// Builds the archive's entry name: the path below the served directory with
/// `/` separators, as the zip format expects. The base directory itself maps
/// to no entry.
fn archive_entry_name(base: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(base).ok()?;
    let name = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// Builds the `Content-Disposition` header for the archive download. The
/// quoted `filename` keeps only ASCII graphic characters and spaces (quotes,
/// backslashes, and all control characters stripped) and falls back to
/// `archive`; a name carrying non-ASCII additionally gets the RFC 8187
/// `filename*` form so browsers restore the real name. `name_suffix` is a
/// static ASCII marker appended before `.zip` ("-selection" for selection
/// downloads, empty for whole-directory ones).
fn content_disposition_for(dir_path: &Path, name_suffix: &str) -> HeaderValue {
    let dir_name = dir_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();

    let ascii_name: String = dir_name
        .chars()
        .filter(|c| (c.is_ascii_graphic() && *c != '"' && *c != '\\') || *c == ' ')
        .collect();
    let ascii_name = if ascii_name.trim().is_empty() {
        "archive".to_string()
    } else {
        ascii_name
    };

    let mut value = format!("attachment; filename=\"{ascii_name}{name_suffix}.zip\"");
    if !dir_name.is_ascii() {
        let encoded = utf8_percent_encode(&dir_name, EXT_VALUE_ENCODE);
        value.push_str(&format!("; filename*=UTF-8''{encoded}{name_suffix}.zip"));
    }

    HeaderValue::from_str(&value)
        .unwrap_or_else(|_| HeaderValue::from_static("attachment; filename=\"archive.zip\""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;
    use std::fs;
    use std::io::Cursor;
    use std::io::Read;
    use tempfile::tempdir;

    async fn zip_bytes(dir: &Path, serve_hidden: bool) -> (warp::http::StatusCode, Vec<u8>) {
        let response = serve_directory_zip(dir, serve_hidden, false).await.unwrap();
        let status = response.status();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        (status, body.to_vec())
    }

    fn entry_names(bytes: &[u8]) -> Vec<String> {
        let archive = zip::ZipArchive::new(Cursor::new(bytes.to_vec())).unwrap();
        archive.file_names().map(|name| name.to_string()).collect()
    }

    #[tokio::test]
    async fn zips_a_nested_tree_with_forward_slash_names() {
        let _lock = ZIP_TEST_LOCK.lock().await;
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "alpha").unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("sub").join("b.txt"), "beta").unwrap();

        let (status, bytes) = zip_bytes(dir.path(), false).await;

        assert_eq!(status, 200);
        let mut names = entry_names(&bytes);
        names.sort();
        assert_eq!(names, vec!["a.txt", "sub/b.txt"]);

        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut content = String::new();
        archive
            .by_name("sub/b.txt")
            .unwrap()
            .read_to_string(&mut content)
            .unwrap();
        assert_eq!(content, "beta");
    }

    #[tokio::test]
    async fn hidden_entries_stay_out_unless_enabled() {
        let _lock = ZIP_TEST_LOCK.lock().await;
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("visible.txt"), "v").unwrap();
        fs::write(dir.path().join(".hidden"), "h").unwrap();
        fs::create_dir(dir.path().join(".secret")).unwrap();
        fs::write(dir.path().join(".secret").join("s.txt"), "s").unwrap();

        let (_, without_hidden) = zip_bytes(dir.path(), false).await;
        let (_, with_hidden) = zip_bytes(dir.path(), true).await;

        assert_eq!(entry_names(&without_hidden), vec!["visible.txt"]);
        let mut names = entry_names(&with_hidden);
        names.sort();
        assert_eq!(names, vec![".hidden", ".secret/s.txt", "visible.txt"]);
    }

    #[tokio::test]
    async fn empty_directory_yields_a_valid_empty_archive() {
        let _lock = ZIP_TEST_LOCK.lock().await;
        let dir = tempdir().unwrap();

        let (status, bytes) = zip_bytes(dir.path(), false).await;

        assert_eq!(status, 200);
        assert!(entry_names(&bytes).is_empty());
    }

    #[tokio::test]
    async fn response_carries_zip_headers() {
        let _lock = ZIP_TEST_LOCK.lock().await;
        let dir = tempdir().unwrap();

        let response = serve_directory_zip(dir.path(), false, false).await.unwrap();

        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "application/zip"
        );
        let disposition = response
            .headers()
            .get("content-disposition")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(disposition.starts_with("attachment; filename=\""));
        assert!(disposition.ends_with(".zip\""));
        // Drain the body so the builder task shuts down cleanly.
        let _ = response.into_body().collect().await;
    }

    #[tokio::test]
    async fn head_answers_zip_headers_with_an_empty_body() {
        // No lock: the headers-only path takes no build slot.
        let dir = tempdir().unwrap();

        let response = serve_directory_zip(dir.path(), false, true).await.unwrap();

        assert_eq!(response.status(), 200);
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "application/zip"
        );
        assert!(response.headers().contains_key("content-disposition"));
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert!(body.is_empty());
    }

    #[test]
    fn channel_writer_reports_broken_pipe_once_the_receiver_is_gone() {
        // This BrokenPipe is what unwinds a build whose client disconnected,
        // which in turn ends the blocking task and releases its zip slot.
        let (tx, rx) = mpsc::channel::<io::Result<Vec<u8>>>(CHANNEL_CAPACITY);
        drop(rx);

        let mut writer = ChannelWriter { tx };
        let error = writer.write(b"data").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
        // An empty write never touches the channel.
        assert_eq!(writer.write(b"").unwrap(), 0);
        writer.flush().unwrap();
    }

    #[test]
    fn archive_entry_name_uses_forward_slashes_and_skips_the_base() {
        let base = Path::new("root");
        assert_eq!(
            archive_entry_name(base, Path::new("root/sub/file.txt")),
            Some("sub/file.txt".to_string())
        );
        assert_eq!(archive_entry_name(base, Path::new("root")), None);
    }

    #[test]
    fn content_disposition_strips_quotes_and_controls() {
        // A backslash cannot appear inside a path component portably (it is
        // a separator on Windows), so the filter's backslash strip is covered
        // by the quote/control cases sharing the same predicate.
        let value = content_disposition_for(Path::new("a\"b\u{1}d"), "");
        assert_eq!(value.to_str().unwrap(), "attachment; filename=\"abd.zip\"");
    }

    #[test]
    fn content_disposition_falls_back_when_nothing_survives() {
        let value = content_disposition_for(Path::new("\"\""), "");
        assert_eq!(
            value.to_str().unwrap(),
            "attachment; filename=\"archive.zip\""
        );
    }

    #[test]
    fn content_disposition_adds_ext_value_for_non_ascii_names() {
        let value = content_disposition_for(Path::new("café"), "");
        let text = value.to_str().unwrap();
        assert!(text.starts_with("attachment; filename=\"caf.zip\""));
        assert!(text.contains("filename*=UTF-8''caf%C3%A9.zip"));
    }

    #[test]
    fn content_disposition_appends_the_selection_suffix_to_both_forms() {
        let value = content_disposition_for(Path::new("café"), "-selection");
        let text = value.to_str().unwrap();
        assert!(text.starts_with("attachment; filename=\"caf-selection.zip\""));
        assert!(text.contains("filename*=UTF-8''caf%C3%A9-selection.zip"));
    }

    #[tokio::test]
    async fn selection_zip_archives_picked_file_and_directory_only() {
        let _lock = ZIP_TEST_LOCK.lock().await;
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("picked.txt"), "p").unwrap();
        fs::write(dir.path().join("unpicked.txt"), "u").unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("sub").join("inner.txt"), "i").unwrap();

        let roots = vec![dir.path().join("picked.txt"), dir.path().join("sub")];
        let response = serve_selection_zip(dir.path(), roots, false, false)
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let disposition = response
            .headers()
            .get("content-disposition")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();

        assert!(disposition.contains("-selection.zip"));
        let mut names = entry_names(&bytes);
        names.sort();
        assert_eq!(names, vec!["picked.txt", "sub/inner.txt"]);
    }

    #[tokio::test]
    async fn selection_zip_of_an_empty_directory_has_no_entries() {
        let _lock = ZIP_TEST_LOCK.lock().await;
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("empty")).unwrap();

        let roots = vec![dir.path().join("empty")];
        let response = serve_selection_zip(dir.path(), roots, false, false)
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();

        assert!(entry_names(&bytes).is_empty());
    }
}

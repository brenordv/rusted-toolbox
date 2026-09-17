# Changelog

## 2.7.0
- The selection form's header row carries a select-all checkbox: ticking it selects every entry
  in the listed directory, unticking clears them, and it tracks manual changes (indeterminate
  while only some rows are ticked). It is wired by the page's one script, which is a fixed
  string with nothing interpolated into it, so entry names cannot reach a script context. The
  checkbox has no `name` and so never submits, and it renders hidden until the script reveals
  it, so with JavaScript off (or in an empty directory) it never shows. The
  `?download=zip&pick=<name>` query contract is unchanged.

## 2.6.0
- Directory listings can download a selection: each entry row carries a checkbox and the page a
  "📦 Download selected as .zip" button, which submits as a plain GET
  (`?download=zip&pick=<name>&pick=<name>`), so the same form works from curl. The archive holds
  exactly the picked files and folders (folders recursively, entry names relative to the listed
  directory) and downloads as `<dirname>-selection.zip`. Submitting with nothing ticked equals
  the existing whole-directory download.
- Picks name direct children only and are validated all-or-nothing before the response starts:
  a malformed, missing, traversing, or symlink pick fails the request as 404 with one warn
  naming the offenders (capped at five), never a silently lighter archive and never a fallback
  to the whole directory. Hidden entries follow `--serve-hidden`; at most 512 picks per request;
  duplicates collapse on canonical paths, which also keeps zip entry names collision-free (the
  zip writer aborts a stream on duplicate names).
- The query string is now parsed from its raw form so repeated `pick` keys survive; the decoder
  is serde_urlencoded, the same one warp's typed query filter uses, so existing query behavior
  (lossy UTF-8, `+` as space, bare keys, unknown keys ignored, last `download` wins) is
  unchanged. `?download=zip` without picks behaves exactly as before.
- HEAD on a selection validates the picks and answers the headers with no body and no build
  slot, matching the whole-directory zip. The zip builder's log lines now carry the root count.

## 2.5.0
- The directory listing shows 📁/📄 glyphs again instead of the `[DIR]`/`[FILE]` markers 2.1.0
  introduced. The 1.0.1 no-emoji rule was about terminal rendering; the listing is HTML for a
  browser, where the glyphs render fine.
- Zip downloads no longer require typing the query string: each listing page links a download of
  the directory it shows ("📦 Download this directory as .zip"), and every subdirectory row
  carries a `zip` link in its size column. `?download=zip` works as before and stays the form for
  curl and scripts. The links reuse the listing's escaping: entry hrefs are built from
  percent-encoded segments, so a `?` in a folder name cannot start the query early.

## 2.4.3
- Two of the recorded test gaps closed: a route test pins that a file with a pre-Unix-epoch
  mtime serves 200 with no `ETag`/`Last-Modified` (the 2.4.0 panic guard, previously
  unit-tested only), and a unit test pins that `ChannelWriter` reports `BrokenPipe` once the
  response receiver is gone, the signal that unwinds a zip build and frees its slot when the
  client disconnects. New dev-dependency: `filetime` (workspace version) for the mtime test.
  The zip 503 slot-exhaustion branch stays untested by decision, recorded in the backlog.

## 2.4.2
- The startup prints (the "Server running at ..." banner and the `--app-header` runtime-config
  lines) go through `common_cli::broken_pipe`: a consumer that closes stdout before boot gets
  a debug note instead of a panic, any other stdout failure a single warning, and the server
  starts either way. Pairs with common-cli 1.7.0, which made the shared header block around
  those lines broken-pipe safe.

## 2.4.1
- Security: a non-hidden symlink inside the root could serve a hidden sibling without
  `--serve-hidden`, because only the URL path was screened for dotted segments. The
  canonicalized target, taken relative to the root, is screened too now, so `link -> .secret`
  answers 404 the same as the direct path. With `--serve-hidden` the alias serves as before.
- The `Range` unit matches case-insensitively per RFC 9110 §14.1; `Bytes=0-5` used to get the
  whole file as a 200, now it gets the 206.
- A directory entry that fails to read mid-listing still truncates the page to what was
  collected, but now logs a warning naming the directory; it was silent, so a permissions
  hiccup produced a shorter page with no operator-visible trace.

## 2.4.0
- Conditional GET: a GET or HEAD whose `If-None-Match` or `If-Modified-Since` validator is
  still current is answered with a bodyless 304 carrying the same `ETag` and `Last-Modified`
  the 200 would have carried, so browser refreshes of large assets stop re-transferring them.
  `If-None-Match` uses the weak entity-tag comparison over the whole listed set (a `W/`
  prefix on a candidate is ignored), `*` counts only as the entire field value, and a present
  `If-None-Match` makes `If-Modified-Since` ignored, per RFC 9110 §13. The conditionals are
  evaluated before `Range`/`If-Range`, so a current validator answers 304 even on a ranged
  request. An unparseable `If-Modified-Since` date is ignored and the full file is served.
- A file whose modification time predates the Unix epoch now serves without validators
  instead of hitting httpdate's formatter, which panics on such times; previously any GET of
  such a file could kill the connection task.

## 2.3.0
- HEAD support: a HEAD runs the same path resolution and security checks as GET and answers
  with GET's status and headers (Content-Length, Content-Type, Content-Range, validators) and
  no body. Other methods still get a 405, which now carries `Allow: GET, HEAD` as RFC 9110
  requires. HEAD on a directory `?download=zip` answers from the headers alone: no archive is
  built and none of the 4 build slots is taken, so probes are free; the flip side is that a
  HEAD says 200 even at the moment a GET would get the 503.
- File responses carry `Last-Modified` and a strong `ETag` built from the file's modification
  time and length (not a content hash), and `If-Range` is validated per RFC 9110 §13.1.5: a
  weak, stale, or unparseable validator downgrades the ranged request to a full 200, so a file
  replaced between range requests can no longer splice inconsistent bytes into a resumed
  download. A date validator only matches while the modification time is at least one second
  old, since a younger `Last-Modified` cannot prove the file was not modified twice within the
  same second. Both validators come from the same stat as the streamed bytes, and both reveal
  the file's modification time; see the readme note before binding beyond localhost.
- Directory listings set an explicit `Content-Length`, so HEAD on a listing reports the page
  size instead of nothing.
- Route construction moved into a `build_routes` seam, and a `warp::test` pass now pins the
  composed route in-tree: empty-query and Range wiring, HEAD parity with GET (including hidden
  paths, traversal attempts, and hidden zip downloads), If-Range match and mismatch, the 405
  `Allow` header, and both zip flavors. Conditional GET (`If-None-Match`/`If-Modified-Since`
  answering 304) is recorded in the backlog, not built; browsers revalidating against these new
  validators still get full 200s.

## 2.2.0
- File responses stream in 64 KiB chunks instead of buffering whole files, and HTTP Range
  requests are supported: a single `bytes=` range answers 206 with `Content-Range`, an
  unsatisfiable one answers 416, and every file response advertises `Accept-Ranges: bytes` with an
  exact `Content-Length`. Multi-range and malformed `Range` headers are ignored and the whole file
  is served, which RFC 9110 permits a server to do.
- New `?download=zip` on directory URLs: streams the folder as a deflate-compressed zip archive,
  produced on a blocking thread and fed through a bounded channel, so large folders never buffer
  whole in memory (there is also no `Content-Length`, so browsers show no progress bar). Hidden
  entries stay out unless `--serve-hidden` is on; symlinks are not followed and not archived;
  empty directories are not recorded. An entry that cannot be read before its header is written
  is skipped and logged at warn level; a failure after that point aborts the download, because
  the 200 is already on the wire. At most 4 archives build concurrently; requests beyond that get
  a 503. The zip dependency is trimmed to the deflate write path (pure-Rust zlib-rs backend).
- Opening a file that exists but cannot be read now logs a warning before the 404; it used to be
  indistinguishable from a missing file.
- Note on the access log: it records the status at time-to-reply and the request's content-length.
  For streamed file and zip responses it reflects neither mid-stream failures nor response sizes;
  a zip that breaks mid-transfer still logs as 200, and the warn-level entry from the archive
  builder is the durable record of what went wrong.

## 2.1.1
- Security (Windows): the hidden-file block now checks backslash-separated path segments too.
  `PathBuf::join` honors `\` on Windows, so an URL like `/sub%5C..%5C.secret%5Cdata.txt` could
  reach a dotfile that the same path spelled with `/` correctly got a 404 for.
- The directory listing's `[DIR] ..` link goes up one level even when the request path carries
  the trailing slash browsers normalize onto directory URLs; `/sub/` previously linked back to
  itself.
- `serve_file` and `collect_directory_entries` do their file IO through `tokio::fs`, so a large
  file or a slow directory read no longer blocks a runtime worker thread. Responses still buffer
  whole files in memory; streaming and HTTP Range support remain recorded ideas, not built.

## 2.1.0
- Fixed an XSS hole in the directory listing: the page title and heading, the parent link, and every entry name and href are now HTML-escaped, and href path segments are percent-encoded so file names containing `#`, `?`, quotes, or spaces produce working, non-injectable links.
- Added graceful shutdown: the server drains in-flight connections on Ctrl+C, logs one info line, and exits with code 0.
- BREAKING: `-h` now prints help instead of being a shorthand for `--host`; `--host` is long-only.
- Validation failures are now reported on stderr before the logging subscriber boots; previously they went through `error!` before any subscriber existed and were dropped.
- Extracted `build_and_validate` from `initialize` as a testable seam and covered it (invalid host IP, port 0, nonexistent path, non-directory path), plus regression tests for path traversal rejection, index.html fallback, listing escaping in element and attribute contexts, and href percent-encoding.
- Replaced the folder/file emoji glyphs in the directory listing with plain `[DIR]`/`[FILE]` markers.
- Rewrote the readme: the command is `https` (not `http`), the web root is `-a/--path` (not positional), the flag table matches the real CLI, and the sample outputs no longer show fabricated header and log lines.

## 2.0.0
- Refactored to fit the new tooling.
- Renamed the tool to `https` to avoid conflicting with the crate `http`.

## 1.1.0
- Added `--serve-hidden` flag to optionally serve hidden files and directories (names starting with `.`).
- Hidden files are now also blocked from direct URL access by default, not just hidden from directory listings.
- Updated dependencies.

## 1.0.2
- Updated dependencies.

## 1.0.1
- Removed emojis. They don't render properly on every terminal.

## 1.0.0
Initial release
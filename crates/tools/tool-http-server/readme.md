# HTTP Server Tool (`https`)

## What it does

`https` is a lightweight, development-focused web server that serves static files from a local
directory. It provides instant file serving with automatic directory browsing. It's handy for
serving static websites, testing frontends, or sharing files during development.

**Key features:**
- Serves static files from any directory with automatic MIME type detection
- Streamed file responses with HTTP Range support (206 partial content), so downloads can resume
  and large files never load whole into memory
- `Last-Modified` and `ETag` on file responses, with `If-Range` validation so resumed downloads
  restart cleanly when the file changed in between
- Conditional GET: revalidation with `If-None-Match` or `If-Modified-Since` answers 304, so
  browser refreshes of unchanged assets transfer no bytes
- HEAD requests answer with the same headers as GET and no body, so download managers can probe
  before fetching
- Download any directory as a zip archive: every listing links a download of itself and of each
  subdirectory, and `?download=zip` does the same from scripts
- Select entries in a listing with checkboxes, one by one or all at once, and download just those
  as one zip (`?download=zip&pick=<name>&pick=<name>` from scripts)
- Directory browsing with file size display and navigation
- Automatic index file serving (index.html, index.htm)
- Request logging with detailed access information
- Configurable host, port, and root directory
- Graceful shutdown on Ctrl+C: in-flight requests are drained before the process exits
- Async HTTP server powered by Warp

## Command-line options
- `-a, --path <PATH>`: Directory to serve as web root (default: current directory)
- `-p, --port <PORT>`: Port number to listen on (default: 4200)
- `--host <HOST>`: Host address to bind the server to (default: 127.0.0.1). Long-only, since `-h` prints help
- `-s, --serve-hidden`: Serve hidden files and directories (names starting with `.`). Off by default

The shared tool flags are also available: `--app-header` prints a header with the tool name,
version, and runtime configuration before the server starts, and `--log-level`, `--log-to-console`,
`--log-to-file`, and `--rotate-log-file-by-day` control logging.

## Examples
### Basic usage - serve current directory
**Command:**
```bash
https
```
**Output:**
```
Server running at http://127.0.0.1:4200
```

### Serve a specific directory
**Command:**
```bash
https -a /path/to/website
```
**Output:**
```
Server running at http://127.0.0.1:4200
```

### Custom port
**Command:**
```bash
https --port 8080
```
**Output:**
```
Server running at http://127.0.0.1:8080
```

### Serve a specific directory on a custom port
**Command:**
```bash
https -a ./dist -p 3000
```
**Output:**
```
Server running at http://127.0.0.1:3000
```

### Custom host (bind to all interfaces)
**Command:**
```bash
https --host 0.0.0.0
```
**Output:**
```
Server running at http://0.0.0.0:4200
```

### Specific host and port
**Command:**
```bash
https --host 192.168.1.100 --port 3000
```
**Output:**
```
Server running at http://192.168.1.100:3000
```

## Features in detail
### Directory browsing
When accessing a directory without an index file, the server generates an HTML listing showing:
- 📁 entries for subdirectories with navigation links and a `zip` download link per folder
- 📄 entries with human-readable sizes (B, KB, MB, GB, TB)
- A checkbox per entry plus a "📦 Download selected as .zip" button, so a hand-picked set of
  files and folders downloads as one archive
- A select-all checkbox in the table header that ticks or clears every entry at once (it needs
  JavaScript and stays hidden without it)
- A "📦 Download this directory as .zip" link for the directory being listed
- Parent directory navigation (..)
- Clean design with hover effects

File and directory names are HTML-escaped in the listing, and link targets are percent-encoded, so
names containing characters like `#`, `?`, or quotes render safely and link correctly.

### Index file serving
The server automatically looks for and serves these index files in order:
1. `index.html`
2. `index.htm`

### Range requests and streaming
Files are streamed in 64 KiB chunks and every file response carries `Accept-Ranges: bytes` and an
exact `Content-Length`. A request with a single `Range: bytes=...` header gets a 206 with the
matching `Content-Range`; a range past the end of the file gets a 416. Multi-range requests and
malformed range headers are ignored and the whole file is served. This is what lets download
managers and media players resume or seek.

File responses also carry `Last-Modified` and a strong `ETag` derived from the file's
modification time and length (not a content hash). A ranged request that presents an `If-Range`
validator only gets its 206 while the validator still matches; once the file changes, the
response downgrades to a full 200 instead of splicing bytes from two different file versions
into one resumed download. A date-form validator additionally only matches while the file's
modification time is at least a second old.

A request revalidating with `If-None-Match` or `If-Modified-Since` gets a 304 with no body
(carrying the same `ETag` and `Last-Modified` the 200 would carry) while the validator is still
current, so refreshes of unchanged files cost a round-trip and nothing more. When both headers
are present, `If-None-Match` alone decides, and the conditionals are checked before any `Range`
header, both per RFC 9110. A stale or unparseable validator gets the full response.

One privacy note: the validators expose file modification times to every client. Irrelevant on
localhost, worth knowing before binding to other interfaces with `--host`.

HEAD works everywhere GET does, including ranged requests, and returns the same status and
headers with an empty body. The one exception worth knowing: a HEAD on `?download=zip` does not
build an archive, so it reports no meaningful size and answers 200 even while concurrent GET
downloads are being told 503.

### Download a folder as zip
Every directory listing links a zip download of the folder it shows, and each subdirectory row
carries a `zip` link, so a browser needs no URL editing. From a script, append `?download=zip` to
any directory URL:

```bash
curl -o project.zip "http://127.0.0.1:4200/project?download=zip"
```

### Download a selection as zip
Tick the checkboxes next to the entries you want and press "Download selected as .zip"; the
archive contains exactly those files and folders (folders recursively), named
`<dirname>-selection.zip`. The checkbox column's header holds a select-all box that ticks or
clears the whole listing at once and shows an indeterminate mark while only some rows are ticked
(it needs JavaScript and stays hidden without it). Submitting with nothing ticked downloads the
whole directory, same as the header link. From a script, repeat the `pick` parameter:

```bash
curl -o parts.zip "http://127.0.0.1:4200/project?download=zip&pick=src&pick=readme.md"
```

Selection rules: each `pick` names a direct child of the directory in the URL (no paths, no
`..`; both `/` and `\` count as separators, so a Unix file name containing a backslash cannot be
selected), symlinks cannot be selected, hidden entries follow `--serve-hidden` like everywhere
else, and at most 512 picks are accepted per request. Validation is all-or-nothing: one invalid pick
fails the whole request with a 404 (the offending names are logged at warn level), so an archive
is never silently lighter than what was asked. Duplicate picks collapse to one entry.

The archive is deflate-compressed and streamed while it is being built, so large folders do not
buffer in memory; the trade-off is that there is no `Content-Length`, so browsers cannot show a
progress bar. The explicit query wins over index files, so a folder containing an `index.html` can
still be downloaded. Hidden entries follow the same policy as the rest of the server: excluded
unless `--serve-hidden` is on, and a hidden directory itself cannot be downloaded without it.
Symlinks are not followed and not archived, and empty subdirectories are not recorded. A file the
server cannot read is skipped and logged at warn level, so check the log if an archive comes back
lighter than expected.

At most 4 archives build at the same time; further requests get a 503 until a slot frees up. Each
build costs real CPU (compression), which is worth keeping in mind before exposing the server
beyond localhost with `--host`.

### Serving hidden files
By default, files and directories starting with `.` are hidden from directory listings and blocked
from direct access. Use `--serve-hidden` to include them in listings and allow direct access.

```bash
https -a /path/to/website --serve-hidden
```

### Security features
- **Path traversal protection**: Prevents access to files outside the root directory
- **Hidden file protection**: Files and directories starting with `.` are hidden and inaccessible by default. Both directory listings and direct URL access are blocked unless `--serve-hidden` is enabled
- **Listing output escaping**: Directory listings HTML-escape untrusted names and percent-encode link targets
- **Method restriction**: Only GET and HEAD requests are allowed; everything else gets a 405
  with `Allow: GET, HEAD`

### Request logging
Each request is logged at info level. The default log level is warn, so pass `--log-level info` to
see access logs:
```
HTTP request completed - GET /path/file.html 200 - 15ms - 0 bytes - UA: Mozilla/5.0...
```

### Graceful shutdown
Press Ctrl+C to stop the server. The listener closes, in-flight requests finish, one info line is
logged, and the process exits with code 0.

## Real-world use cases

- **Frontend development**: Serve static websites, SPAs, or build outputs locally
- **File sharing**: Quick way to share files on a local network during development
- **Testing**: Serve test data, mock APIs, or static assets for testing
- **Documentation**: Serve generated documentation sites (Hugo, Jekyll, etc.)
- **Prototyping**: Quickly serve HTML prototypes or design mockups
- **Educational**: Demonstrate web concepts or teach static site development

## Comparison with other tools

| Feature             | `http-server` (Node.js) | `python -m http.server` | This tool         |
|---------------------|-------------------------|-------------------------|-------------------|
| Setup required      | npm install needed      | Python installation     | Single binary     |
| Default port        | 8080                    | 8000                    | 4200              |
| Directory browsing  | Yes                     | Yes                     | Yes (styled)      |
| Index file support  | Yes                     | Yes                     | Yes               |
| MIME type detection | Yes                     | Yes                     | Yes               |
| Request logging     | Yes                     | Yes                     | Yes (detailed)    |
| Path security       | Yes                     | Yes                     | Yes               |
| Performance         | Good                    | Basic                   | High (async Rust) |

### Key advantages

1. **Zero configuration**: Works out of the box without any setup or dependencies
2. **File serving**: Directory browsing with modern styling
3. **Detailed logging**: Request information for debugging
4. **High performance**: Built with Rust's async capabilities
5. **Security first**: Built-in protection against common web server vulnerabilities

## Known issues

1. **HTTP only**: Does not support HTTPS/TLS encryption for secure connections
2. **Single directory**: Cannot serve multiple root directories simultaneously
3. **No authentication**: No built-in authentication or access control mechanisms
4. **Static only**: Does not support server-side processing or dynamic content generation
5. **Zip downloads have no progress bar**: the archive is streamed while it is built, so the
   response carries no `Content-Length`. A zip that fails mid-transfer (whole-directory and
   selection downloads alike) also still shows as 200 in the access log, because the status was
   already sent; the warn-level log entry is the record
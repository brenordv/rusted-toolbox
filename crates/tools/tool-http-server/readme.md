# HTTP Server Tool (`https`)

## What it does

`https` is a lightweight, development-focused web server that serves static files from a local
directory. It provides instant file serving with automatic directory browsing. It's handy for
serving static websites, testing frontends, or sharing files during development.

**Key features:**
- Serves static files from any directory with automatic MIME type detection
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
- `[DIR]` entries for subdirectories with navigation links
- `[FILE]` entries with human-readable sizes (B, KB, MB, GB, TB)
- Parent directory navigation (..)
- Clean design with hover effects

File and directory names are HTML-escaped in the listing, and link targets are percent-encoded, so
names containing characters like `#`, `?`, or quotes render safely and link correctly.

### Index file serving
The server automatically looks for and serves these index files in order:
1. `index.html`
2. `index.htm`

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
- **Method restriction**: Only GET requests are allowed

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
5. **Whole-file reads**: Files are read fully into memory before being sent; no Range/206 support
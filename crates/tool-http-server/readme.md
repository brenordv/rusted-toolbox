# HTTP Server Tool

## What it does

The HTTP Server tool is a lightweight, development-focused web server that serves static files from a local directory. 
It provides instant file serving with automatic directory browsing, making it perfect for quickly serving static 
websites, testing frontend applications, or sharing files locally during development.

**Key Features:**
- Serves static files from any directory with automatic MIME type detection
- Beautiful directory browsing with file size display and navigation
- Security-conscious path traversal protection
- Automatic index file serving (index.html, index.htm)
- Real-time request logging with detailed access information
- Configurable port and root directory
- High-performance async HTTP server powered by Warp

## Command-Line Options
- `path`: Directory to serve as web root (defaults to current directory)
- `-p, --port`: Port number to listen on (default: 4200)

## Examples

### Basic Usage - Serve Current Directory
**Command:**
```bash
http
```
**Output:**
```
🚀 Simple HTTP Server v1.0.0
---------------------------
📂 Root directory: /current/working/directory
🚪 Port: 4200
Server running at http://127.0.0.1:4200
```

### Serve Specific Directory
**Command:**
```bash
http /path/to/website
```
**Output:**
```
🚀 Simple HTTP Server v1.0.0
---------------------------
📂 Root directory: /path/to/website
🚪 Port: 4200
Server running at http://127.0.0.1:4200
```

### Custom Port
**Command:**
```bash
http --port 8080
```
**Output:**
```
🚀 Simple HTTP Server v1.0.0
---------------------------
📂 Root directory: /current/working/directory
🚪 Port: 8080
Server running at http://127.0.0.1:8080
```

### Serve Specific Directory on Custom Port
**Command:**
```bash
http ./dist -p 3000
```
**Output:**
```
🚀 Simple HTTP Server v1.0.0
---------------------------
📂 Root directory: ./dist
🚪 Port: 3000
Server running at http://127.0.0.1:3000
```

## Features in Detail

### Directory Browsing
When accessing a directory without an index file, the server generates a beautiful HTML listing showing:
- 📁 Subdirectories with navigation links
- 📄 Files with human-readable sizes (B, KB, MB, GB, TB)
- Parent directory navigation (..)
- Clean, responsive design with hover effects

### Index File Serving
The server automatically looks for and serves these index files in order:
1. `index.html`
2. `index.htm`

### Security Features
- **Path Traversal Protection**: Prevents access to files outside the root directory
- **Hidden File Protection**: Automatically hides files and directories starting with `.`
- **Method Restriction**: Only GET requests are allowed

### Request Logging
All requests are logged with detailed information:
```
HTTP request completed - GET /path/file.html 200 - 15ms - 1024 bytes - UA: Mozilla/5.0... - Remote: 127.0.0.1
```

## Real-World Use Cases

- **Frontend Development**: Serve static websites, SPAs, or build outputs locally
- **File Sharing**: Quick way to share files on local network during development
- **Testing**: Serve test data, mock APIs, or static assets for testing
- **Documentation**: Serve generated documentation sites (Hugo, Jekyll, etc.)
- **Prototyping**: Quickly serve HTML prototypes or design mockups
- **Educational**: Demonstrate web concepts or teach static site development

## Comparison with Other Tools

This tool provides functionality similar to other development servers:

### Comparison with Common Development Servers

| Feature                 | `http-server` (Node.js) | `python -m http.server` | This Tool            |
|-------------------------|-------------------------|-------------------------|----------------------|
| **Setup Required**      | npm install needed      | Python installation     | Single binary        |
| **Default Port**        | 8080                    | 8000                    | 4200                 |
| **Directory Browsing**  | ✅                       | ✅                       | ✅ (Enhanced styling) |
| **Index File Support**  | ✅                       | ✅                       | ✅                    |
| **MIME Type Detection** | ✅                       | ✅                       | ✅                    |
| **Request Logging**     | ✅                       | ✅                       | ✅ (Detailed format)  |
| **Path Security**       | ✅                       | ✅                       | ✅                    |
| **Performance**         | Good                    | Basic                   | High (Async Rust)    |

### Key Advantages

1. **Zero Configuration**: Works out of the box without any setup or dependencies
2. **File Serving**: Enhanced directory browsing with modern styling
3. **Detailed Logging**: Comprehensive request information for debugging
4. **High Performance**: Built with Rust's async capabilities for excellent performance
5. **Security First**: Built-in protection against common web server vulnerabilities

## Known Issues

1. **HTTP Only**: Does not support HTTPS/TLS encryption for secure connections
2. **Single Directory**: Cannot serve multiple root directories simultaneously
3. **No Authentication**: No built-in authentication or access control mechanisms
4. **Static Only**: Does not support server-side processing or dynamic content generation
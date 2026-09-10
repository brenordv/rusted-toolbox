# Basic
[X] Migrate cli to the new pattern.
[X] Add more test coverage to the tool.
    Outcome (2.1.0): added regression tests for path traversal rejection, index.html fallback, listing escaping in element and attribute contexts, href percent-encoding, and `build_and_validate` failures (invalid host IP, port 0, nonexistent path, non-directory path). 19 tests before, 37 after.
[X] Research improvements to the tool.
    Outcome (2.1.0): three findings. The directory listing interpolated unescaped names and paths into HTML at five sites (fixed with escaping plus href percent-encoding); `-h` was bound to `--host` and shadowed clap's conventional help short (fixed, `--host` is long-only now); `serve_file` and `collect_directory_entries` do synchronous file IO on tokio workers (recorded below, not built).
[ ] Async file IO in serve_file/collect_directory_entries (tokio::fs) to stop blocking runtime workers on large files.

## Open questions
1. [X] Should we add the "setup graceful shutdown", like the other apps have?
   Outcome (2.1.0): yes, implemented. The server runs through warp's graceful chain (`serve(...).bind(addr).await.graceful(signal).run().await`) driven by `tokio::signal::ctrl_c`; it drains in-flight connections, logs one info line, and exits 0.
2. [X] How hard would it be to add a "Compact folder and download" feature?
   Outcome: feasible as a new branch in `handle_request` (for example a `?download=zip` query on directories); needs a zip dependency and a streaming write so large folders do not buffer whole in memory. Recorded, not built.
3. [X] How hard would it be to add a very basic video streaming feature for supported media? Doesn't need to be fancy or support every single video file.
   Outcome: requires HTTP Range/206 support, which `serve_file` lacks today (it reads the whole file with `std::fs::read` and never sends `Accept-Ranges`). Range parsing, partial reads, and 206/416 responses would have to be added to the file-serving path. Recorded, not built.

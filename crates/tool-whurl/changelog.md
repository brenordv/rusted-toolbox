# 1.1.0 (2025-11-11)
- Added support for per-API `_global.hurlvars` files that always load alongside the selected environment.
- Automatically load env/global variable files for cross-API includes to keep shared requests in sync.
- Emit warnings (when not in silent mode) whenever variable sources collide during merge.
- Documented global variables and clarified `--file-root` usage with practical examples in the README.

# 1.0.0 (2025-11-09)
- Initial release.
# 2.0.0 (2025-10-31)
- Introduced subcommands: `text` and `files`.
  - `text` retains previous behavior and adds a concise positional form: `lookup text "your text"`.
  - Prior flags like `--text/-t`, `--extension/-e`, `--path/-p`, `--current-only/-c`, `--line-only/-l`, and `--no-header` remain supported under the `text` subcommand.
- New `files` subcommand to search for files by filename.
  - Supports wildcard (default) or regex patterns; case-insensitive by default.
  - Recursive by default; `--current-only` limits search to the current directory.
  - Progress displays the folder being read on a single updating line; line is cleared before updates to avoid overlapping.
  - On errors while traversing, clears the progress line, then prints a brief error message and continues. Errors can be suppressed with `--no-errors`.
  - On match, prints the absolute path (Windows verbatim prefixes like `\\?\` stripped for readability).
  - Final summary printed at the end (dirs, files, matches, elapsed); can be suppressed with `--no-summary`.
  - Other controls: `--no-progress`, `--no-header`, `--regex`, `--wildcard`, `--case-sensitive`.
- CLI UX improvement: if no subcommand is provided, the app now prints usage/help and exits.
- Help examples updated, including a regex that matches `mydoc.pdf`, `mydoc.epub`, or `mydoc.mobi`: `lookup files --regex "^mydoc\.(pdf|epub|mobi)$"`.
- Internal refactor: shared, tool-specific helpers extracted to `lookup_shared.rs`; `lookup_text_app.rs` and `lookup_files_app.rs` contain subcommand-specific logic.
- Dependencies added for filename search: `regex`, `globset`, `walkdir`.

# 1.0.1 (2025-10-03)
- Added support for matching by exact file name for dotfiles or bare names.
  - Pattern ".env" matches basename ".env"
  - Pattern "env" matches basename "env"

# 1.0.0 (2025-10-03)
- Initial release.
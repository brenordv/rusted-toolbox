# Changelog

## 2.3.1
- Documentation: `get_app_folder` and `get_app_sub_folder` take a fixed name from the code (a package name, or a shared folder name such as the one `speak` keeps its settings under), never text from outside the program.

## 2.3.0
- Added `string_utils::display_line`: text on one line, with control characters, line and paragraph separators, and bidirectional controls each turned into a space. Moved from eh-gui.
- Added `string_utils::error_preview`, `ErrorPreview`, and `ERROR_PREVIEW_CHARS`: a failure text on one line, cut at 140 characters with "…". Moved from eh-gui.
- Added `string_utils::non_blank`: the text with the whitespace around it removed, or `None` when nothing is left.

## 2.2.0
- Added `string_utils::SecretString`: redacting Debug/Display, and allows access to the actual value only via `expose()`. Should be used when dealing with connection strings, tokens, keys, etc. to prevent accidental logging.
- Added `string_utils::format_thousands`: a count with comma thousands separators (`12,403`). `format_bytes_to_string` now groups its whole part with the same code.

## 2.1.0
- Added `string_utils::escape_for_terminal_display`: escapes control characters (C0, DEL, C1) and Unicode bidirectional-control characters for safe terminal output. Extracted from the identical hand-rolled escaping in tool-mqtt and tool-pingx, which now use it.

## 2.0.1
- Documented the remaining undocumented public items: `get_current_dir`, the `constants` values, `sanitize_string_for_filename` (stating the substitution-only guarantee and its limits), and the two datetime traits.
- Regrouped the string-formatting example tests with rstest and added cases pinning negative-duration magnitude and TB as the largest byte unit.

## 2.0.0
- Breaking: `get_app_sub_folder`, `get_filename_with_current_date`, and `get_full_filepath_from_string` now take `&str` instead of owned `String`; `format_bytes_to_string` now takes `u64` instead of `&u64`.
- Removed `sanitize_string_for_table_name` from `string_utils`.
- Added a crate-level doc comment and documentation on the changed functions, `get_app_folder`, and the `EnsureDirectoryExists` trait.
- Added unit tests across the string, filesystem, and datetime modules.
- Rewrote the readme, which previously described an unrelated crate.
- The changelog resumes tracking here after a version drift between the manifest (1.0.0) and this file (1.0.1); no history was rewritten.

## 1.0.1
- Updated dependencies.

## 1.0.0
- Initial release.
# Changelog

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
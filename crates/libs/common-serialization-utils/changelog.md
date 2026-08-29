# Changelog

## 1.1.0
- Removed the `cargo new` scaffolding (`add` and its test).
- Re-exported `load_json_file_to_object` at the crate root for a shorter path.
- Errors now use `.with_context` (the repo-standard mechanism) instead of
  `anyhow!`, naming the file and whether the read or the parse failed.
- Added a crate doc and the first real tests, including the BOM-strip case.
- Corrected the crate description: it loads JSON into typed objects; there is no
  serialize half yet.

## 1.0.0
- Initial release: async JSON file loader, split out of `shared`.

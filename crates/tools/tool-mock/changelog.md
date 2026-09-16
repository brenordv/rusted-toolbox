# Changelog

## 2.1.0
- `--locale`/`-l <CODE>` is back, now actually wired through to generation via
  mock-data-utils 3.0.0. Codes are case-insensitive with `-` and `_` interchangeable
  (`pt-br`, `PT_BR`); the default is `en` and the valid list is in `--help` under Locales.
- The locale is honored by the fake-backed types: `person.*` except birthday,
  `internet.username`, `commerce.company`, `commerce.job-title`, `commerce.industry`. Passing
  a non-default locale for any other type logs a warning naming both, instead of silently
  doing nothing (the defect that got the flag removed in 2.0.1).
- `MockConfig::new` (9 positional arguments) replaced with struct-literal construction.

## 2.0.1
- Fixed a duplicate clap short flag: `--age` and `--past` both claimed `-a`, which panicked clap's debug assertions on every debug-build invocation. `-a` stays on `--age`; `--past` is now long-only.
- BREAKING: removed the `--locale`/`-l` flag. It was parsed but never carried into the generation options, so it silently did nothing. It can return once `mock-data-utils` supports locale-aware generation (tracked in that crate's improvements list).
- Added the crate's first tests: clap definition self-check (`debug_assert`), the `MockConfig` to `MockOptions` field mapping, and the extracted `validate_range` min/max check.
- Extracted the min/max validation from `initialize()` into a pure `validate_range` function.
- Readme: documented that this tool deviates from the `<tool>_app.rs` convention on purpose, removed `--locale`, corrected the flag shorts, and narrowed the error-handling claims to what the tool actually checks.
- Version sync: the 2.0.0 entry below shipped without the matching `Cargo.toml` bump (the manifest stayed at 1.1.0); this release realigns the manifest with the changelog.
- Readme: listed the shared common-cli flags and corrected the unknown-data-type note (the error names the rejected value; the list of types lives in `--help`).

## 2.0.0
- Refactored to fit the new tooling.

## 1.1.0
- Added a random car brand data option.

## 1.0.1
- Updated dependencies.

## 1.0.0
Initial release
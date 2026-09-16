# common-utils

Small, dependency-light helpers shared across the toolbox. Kept free of heavy
dependencies by design; only `anyhow` and `chrono` are used.

Modules:

- `datetime_utc_utils`: filename-safe datetime formatting and elapsed-time helpers.
- `file_system`: home and application folder paths, dated filenames, directory creation.
- `string_utils`: filename sanitization, terminal-display escaping, duration formatting, and byte-size formatting.
- `constants`: shared size and formatting constants.

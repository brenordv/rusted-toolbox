# Changelog

## 2.1.0
- Without `-f`, the output format is now inferred from the `-o` extension: `-o x.svg` saves an
  SVG and `-o x.jpg` a JPEG instead of warning and saving `x.svg.png`. Inference covers `svg`,
  `png`, `jpg`/`jpeg`, `bmp`, `tif`/`tiff`, and `tga`, each pinned by a test that writes the
  file; extensions the grayscale QR buffer cannot encode (`ico`, `webp`, `gif`, and the like)
  keep the png fallback with the old warning-and-append behavior, as does anything
  unrecognized. An explicit `-f` wins over the extension, unchanged.

## 2.0.1
- Wifi payloads now backslash-escape the reserved characters `\`, `;`, `,`, `:`, and `"` in the
  SSID and password fields, so credentials containing them scan correctly. Previously the raw
  values produced a malformed payload.
- BREAKING: `--wifi-auth` only accepts `WPA`, `WEP`, or `nopass` (case insensitive, normalized to
  the canonical spelling). Other values are rejected at validation time with a clear message; they
  used to be embedded as-is and produced malformed payloads.
- The wifi password is shown masked as `(set)` in the `--app-header` output and redacted from the
  debug-level payload log, so `--log-level debug` and `--log-to-file` never persist it.
- A warning is now emitted when the `-o` filename extension contradicts the effective output
  format (e.g. `-o x.svg` with the default png format saves `x.svg.png`).
- Argument-validation errors are printed to stderr instead of being sent to the logger, which is
  not installed yet at that point.
- Runtime-config lines are rendered through `common-cli`'s `header_format` helpers; output bytes
  are unchanged apart from the password masking.
- Added tests for wifi payload escaping, auth normalization, extension-mismatch detection, and
  oversized-input errors.

## 2.0.0
- Refactored to fit the new tooling.
- Rewrote CLI parsing with the `clap` derive API and the shared `CommonToolArgs`, replacing the
  hand-built `Command` from the retired `shared` crate.
- Dropped the `-n/--no-header` flag. The runtime configuration is now shown on demand with the
  shared `--app-header` flag.
- Gained the shared runtime flags: `--log-level` (long-only, case insensitive), `--app-header`,
  `--log-to-console`, `--log-to-file`, and `--rotate-log-file-by-day`.
- Logging is now installed through the shared `AppLogger` during boot-up instead of a standalone
  initializer.
- Exit codes are explicit: `0` on success and `1` on failure.
- An incomplete wifi payload now reports the specific missing field (SSID or password) instead of a
  generic message.

## 1.0.0
- Initial release
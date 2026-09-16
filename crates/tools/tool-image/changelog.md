# Changelog

## 2.1.4
- The test suite uses the shared `common_cli::test_writers::FailAfter` double instead of a
  private byte-budget failing-writer copy; the behavior pinned by the tests is unchanged.

## 2.1.3
- Every encoder now writes to a temporary file in the destination directory and renames it
  over the target once the encode finishes. Before, `File::create` truncated the destination
  up front, and when no operation changes the file name (a quality-only re-encode, for
  example) the destination is the input itself, so a failed encode destroyed the only copy.
  Now a failure leaves the destination exactly as it was. On Unix the temporary file honors
  the umask instead of tempfile's 0600 default, and a replaced destination keeps its own
  permissions.
- The GIF encoder's delete-partial-output-on-failure step is gone; with the rename scheme a
  partial file can no longer land on the output path in the first place.

## 2.1.2
- main now follows the fleet's entrypoint pattern: a failed run logs the full error chain
  through `error!` and exits 1 via the shared exit helpers, instead of returning
  `anyhow::Result` and getting anyhow's `Error: ...` debug print.
- Every encoder's output-file creation now names the output path in its error. Before, only
  the GIF encoder did; a permission failure in the PNG/JPEG/WebP/AVIF/BMP paths read as a
  bare OS error under a warn line that only named the input file.
- The non-fatal orientation-read failure during decode now logs at `warn!` instead of
  `error!`; processing continues without the transform either way.

## 2.1.1
- Fixed GIF encoding, which was unusable end to end: the quantized palette was never handed to
  the encoder (empty global palette plus a frame without a local one), so gif 0.14 rejected
  every frame with `MissingColorPalette` and a headers-only `.gif` stub was left on disk looking
  like output. The quantized palette now becomes the global color table, and a failed encode
  deletes the partial output file (a failed deletion logs a warning naming the path).
- The GIF transparent index now picks the first fully transparent palette entry; the old check
  was inverted (it looked for the first opaque entry) and could never report an entry at
  position 0.
- Images wider or taller than 65535 pixels are rejected with a clear error before any file is
  created; the old `as u16` cast wrapped silently and then panicked inside the frame builder.
- GIF trailer-write failures (for example a disk filling up on the final bytes) now surface as
  errors instead of being swallowed, so a truncated file can no longer pass as a successful
  encode.
- A run with any failed job now exits 1 with "N of M jobs failed"; each failure logs a warning
  naming its input file. Previously all failures were counted but the tool still exited 0, and
  the underlying error was only visible at debug level.

## 2.1.0
- Added `-q/--quality <1-100>`: sets the encoding quality for JPEG (default 100) and AVIF
  (default 95) output. Values outside 1-100 are rejected at parse time; when the output format is
  neither JPEG nor AVIF the flag is ignored and a warning names the format.
- Added `-f/--filter <nearest|triangle|catmullrom|gaussian|lanczos3>`: chooses the resize filter
  (default `lanczos3`, the previous hard-wired behavior). Only valid together with `--resize`;
  using it without `--resize` is a parse error.
- Both flags appear in the `--app-header` runtime-config block when set.
- Readme: listed the shared common-cli flags and corrected the format-support line (inputs are
  jpg/jpeg/png/gif/webp/avif/tiff/tif/bmp; the wider format list applies to `--convert` output
  only).

## 2.0.1
- Fixed the quality loss on AVIF conversion: the encoder ran on the library's undocumented lossy defaults and now encodes with explicit near-lossless settings (quality 95, speed 4). A `--quality` flag to override them is a recorded future item.
- `--convert` validates its value at parse time; an unknown format is now a clap error instead of a panic.
- The fallback-encoder message is format-accurate: it warns about quality only for lossy targets and logs a plain info line for lossless ones such as TIFF.
- `determine_output_plan` returns an error instead of panicking when the input path has no parent directory (for example a root path); bare filenames still resolve to a relative output path.
- Replaced the check/cross marks in the per-file finish messages with plain text.
- Removed the dead `FilterType` enum from `models.rs`; a `--filter` flag is a recorded future item.
- The runtime-config header renders its `label: value` lines through `common-cli`'s `header_format::format_config_item`; output bytes unchanged.
- Added tests: PNG and WebP round-trips are pixel-identical, RGBA-to-JPEG produces an opaque RGB image, unknown `--convert` values are rejected at parse, and output-plan handling of parentless paths is pinned.
- Readme corrections: the `--resize 50` suffix is `resized50pct`, EXIF is not preserved (orientation is baked into the pixels and the data dropped), ICC profiles re-embed for JPEG/PNG/AVIF only, and the per-format quality behavior is spelled out.

## 2.0.0
- Refactored to fit the new tooling.

## 1.1.0
- Updated dependencies.
- Improved resizing functionality to also accept percentages with decimal values, and explicit width + height values.

## 1.0.2
- Updated dependencies.
- Removed support for PCX files.

## 1.0.1
- Removed emojis. They don't render properly on every terminal.

## 1.0.0
Initial release
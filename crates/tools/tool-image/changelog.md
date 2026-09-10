# Changelog

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
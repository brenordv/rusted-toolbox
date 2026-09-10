# Basic
[X] Migrate cli to the new pattern.
[X] Add more test coverage to the tool.
    Outcome (2.0.1): added round-trip encoder tests (PNG and WebP pixel-identical, RGBA-to-JPEG opaque RGB), parse-time rejection tests for --convert, and output-plan tests pinning bare-filename handling and erroring on parentless root paths.
[X] Research improvements to the tool.
    Outcome (2.0.1): reviewed the encode pipeline end to end; found the AVIF encoder running on undocumented lossy defaults (the quality-loss issue below), a panic on unknown --convert values, and a fallback-encoder warning that claimed quality loss even for lossless targets; all three fixed. Follow-ups recorded below as the --quality and --filter flags.
[X] Add a --quality flag (AVIF/JPEG) instead of fixed encoder defaults. Outcome (2.1.0): -q/--quality 1-100 overrides JPEG (100) and AVIF (95) defaults; other formats warn that the flag is ignored.
[X] Add a --filter flag choosing the resize filter (the removed FilterType enum was an unwired sketch of this). Outcome (2.1.0): -f/--filter over the five image::imageops::FilterType variants, default lanczos3, requires --resize.

# Known issues
1. Fix loss of quality: When Converting the images back and forth, I noticed a loss of quality. Don't know why, but we must fix this or the tool loses its purpose.
   Outcome (2.0.1): cause identified as the AVIF encoder's undocumented lossy defaults (plus the inherently lossy JPEG and GIF paths, now documented in the readme); fixed with explicit high-quality AVIF settings (quality 95, speed 4). PNG and WebP verified lossless by round-trip tests.

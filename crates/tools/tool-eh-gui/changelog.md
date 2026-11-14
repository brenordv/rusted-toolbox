# Changelog

## 1.0.0
- Initial release (in progress).
- Built on Rune, pinned at rev `c4eda05889c984fff91ccd906e3c00cdfc4cae24` (pre-release, not on
  crates.io). Rune's `[patch.crates-io]` forks apply to the whole workspace:
  - winit 0.30.13: Windows window creation without a taskbar COM call and at the requested
    position; a macOS IME reconversion API; Wayland touchpad scroll deltas scaled by 12.
  - wgpu-hal 30.0.1: DX12 adapter exposure, the shader compiler (FXC) loaded on first compile,
    partial presents on a `FLIP_SEQUENTIAL` swap chain.
  - parley_engine 0.12.0: bidi rule L1, shaping caches, segmenter hooks.
  - accesskit_windows 0.35.2: heading and landmark properties for UI Automation.
  - tiny-skia-path 0.11.4 (Rune's own path dependency, not a workspace patch): Skia's conic
    stroking.

  Each fork was checked to be byte-identical to its crates.io release plus Rune's committed
  `vendor/<name>.patch`, and each patch and Rune build script was read by hand: no network,
  file, process, or telemetry code. The `.wasm` that `rune-highlights` translates at build time
  matches the npm `@pierre/highlights` 0.2.0 release. Rune's `tooling` and `keys-overlay`
  features stay off.

# Changelog

## 1.0.0
- Initial release (in progress): the window shell on Rune. A custom titlebar with the Read and
  Export switch, the connection pill (data folder, log file, version, and the last error in its
  popover), and the connect sheet with the three sign-in methods (connection string, Azure
  credential, Key Vault), config.json recall, Save config, and the Storage settings (data folder,
  message files). Connect opens the data folder's archive, resolves the target as `eh read`
  does, and runs a capture session whose messages go to the archive; the message list, capture
  rules, and archive search come in later versions.
- Closing the window during a session stops it and lets the at-end pass finish in the
  background; a failure reopens the window with the error, and a pass still running after 30 s
  reopens it with a Force quit. The archive is closed at quit.
- A data folder change is accepted only while no session runs; the old folder's archive is
  closed right away.
- Logs to a file by default, at info. A window that cannot be created (no usable GPU adapter) is
  logged and exits with a failure instead of leaving a windowless process.
- Typed secrets stay in hidden fields that are wiped after Connect and Save, never appear in the
  element tree or to assistive technology, and a stored secret's bytes never enter the form.
- Design system in `src/design/`: tokens, kit overrides, motion roles, and a contrast test over
  every listed pair in both appearances. The test moved the `Submit` fill to `#0050B8` with a
  darkening hover and press, raised the dark floating tint to 0.88, and draws secondary text on
  selected rows in the primary text color.
- Icons from Lucide 1.73.0 (ISC; license in `assets/icons/LICENSE`).
- Dev-only demo mode (`--features demo`, binary `eh-gui-demo`): a generated event hub and a
  temporary data folder, scenarios chosen with `EH_GUI_DEMO`, and Rune's agent flags in debug
  builds. Release builds leave it out.
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

# Changelog

## 1.0.0
- Initial release (in progress): the window shell on Rune. A toolbar with the Read and Export
  switch, the connection pill (data folder, log file, version, and the last error in its
  popover), and the connect sheet with the three sign-in methods (connection string, Azure
  credential, Key Vault), config.json recall, Save config, and the Storage settings (data folder,
  message files). Connect opens the data folder's archive, resolves the target as `eh read`
  does, and runs a capture session whose messages go to the archive. Archive search comes in a
  later version.
- The Read section: a stats strip refreshed once a second (rolling digits; per-partition
  counts behind Received; a UI-dropped tile; quiet-hub and "Not receiving updates" states), a
  virtualized list of the newest 5,000 messages (and at most 256 MB of bodies) that follows the
  tail, lands new rows up to four times a second, and offers a "↓ N new" pill once scrolled up;
  keyboard selection and copy; a quick filter by text or regex over bodies and ids, rebuilt off
  the UI thread; an inspector with metadata, a capped selectable body, Copy body, Copy as JSON,
  export of one message to a chosen file, and a close button; and a session log in the connection pill.
- Capture rules in the sidebar (time window and keywords or regex) apply to the running session
  and its message files, or to the next Connect, and are saved with Save config and recalled
  from `config.json`.
- macOS draws the toolbar as the titlebar (traffic lights over it); Windows and Linux keep the
  system title bar.
- The connect sheet sits under the toolbar and grows with the window; its fields scroll between
  a fixed title and a fixed button row, so Connect is always on screen.
- On Windows the colors follow the system's light or dark setting while the app runs, read once
  a second through `UISettings`.
- The window can't be made smaller than 960x600, so the toolbar never overlaps itself.
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
  temporary data folder, scenarios chosen with `EH_GUI_DEMO` (including `seeded-read` and
  `seeded-search`), and Rune's agent flags in debug builds. Release builds leave it out.
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

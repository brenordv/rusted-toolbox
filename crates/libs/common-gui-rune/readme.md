# common-gui-rune

The tool-neutral part of the workspace's Rune design system. A Rune tool in this repo builds
its windows from this crate and rune-kit, and keeps only its own sizes and parts in its own
design module. (`common-gui` is the egui counterpart; the two share nothing.)

Modules:

- the crate root: `Ds` (the dark and light kits with the shared overrides: the `Submit` blue,
  light muted text, the accent focus ring), `Surface` and `surface` (the window, content, and
  raised levels in either appearance).
- `tokens`: `rgb`, the palette seeds, the spacing scale (`SPACE_XS` 4 through `SPACE_XXL` 32),
  the radii, the nine type roles, `CONTROL_HEIGHT`, `HAIRLINE`, `FOCUS_RING_WIDTH`.
- `motion`: the roles `FEEDBACK`, `CONTROL`, and `CONTROL_EXIT`, all critically damped.
- `parts`: `field_label`, `caption`, `issue`, `warning`, `hairline`, `detail_row`, the
  multi-line editors (`text_editor` in any type role, `lines_editor` for short lines), and
  `status_line`.
- `appearance`: `SystemAppearance` reads the system's dark or light setting on Windows, where
  Rune's own theme event does not arrive on Windows 10.
- `window`: `spawn_window_watch` for an app with one window, and the pure `window_failure`
  decision behind it.

## Rules for consumers

1. A color, size, radius, or spring literal lives in this crate or in the tool's own design
   module, never in a view.
2. Text takes one of the type roles. Prose the user writes or reads is `TYPE_BODY`; a value
   the user copies (an id, a path, a URL, a timestamp, a payload) is `TYPE_MONO`.
3. An animation names a motion role, never a spring of its own.
4. One `Submit` button per surface.

## Status lines

`status_line(k, key, tone, lines, text)` is a caption that reports a status: muted, danger,
or warning; wrapping, or one line cut at its width. It shows the text on one line with control
and bidirectional characters as spaces, cut at 140 characters with "…", so text from outside
the program (a server's error, a file error) is safe to pass in. It is a `status` in a polite
live region named by what shows, and a text that was cut is carried whole as its description.

## Editors

`text_editor` takes an `EditorSpec` whose `font` is the type role; a line of that role is the
unit of `EditorHeight`. `Fill { min_lines }` grows to the space it is given; `Fixed { lines }`
never grows, breaks long lines anywhere, and scrolls only once it holds text. The editor has no
submit: `on_key` sees every key first, so the caller inserts the newline on Return and takes
its own shortcuts.

## The window watch

Rune keeps running with no window when it cannot create one (no GPU adapter or device).
`spawn_window_watch(handle, hooks)` starts one thread that looks at the window four times a
second. It applies `hooks.shown` once when the window is first on screen and
`hooks.appearance` when the system appearance is first read or changes. When two seconds pass
with an error from Rune and no window, it quits the app and keeps the reason, which
`WindowWatch::failure()` hands to `main` after `App::try_run` returns. The watch logs nothing:
the caller reports the failure, with `GPU_BACKENDS` for the hint.

## Test support

The `test-harness` feature adds two modules for tests:

- `test_harness`: headless helpers over Rune's `Runtime`, generic over the state type
  (`runtime_at(state, view, size)`, `settle`, `press`, `press_key`, `command`, `element`,
  `rect`, `has_key`, `has_text`, `text_visible`, `focused_key`, `label_of`, `a11y_nodes`,
  `a11y_node`).
- `contrast`: WCAG ratio helpers and the `Contract` collector for a contrast-contract test.

A consumer lists the crate again under `[dev-dependencies]` with the feature on, so a plain
`cargo test` compiles the helpers and a release build leaves them out:

```toml
[dependencies]
common-gui-rune = { workspace = true }

[dev-dependencies]
common-gui-rune = { workspace = true, features = ["test-harness"] }
```

The pairs this crate's own tokens produce (text, muted, and accent text, danger and warning
captions, on the three surfaces; the `Submit` label in its four states) are tested here in
both appearances. A tool tests only the pairs its own parts add.

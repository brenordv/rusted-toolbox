# Changelog

## 1.1.0
- New part `parts::line_field` with `LineFieldSpec`: a label over a single-line field whose
  `on_key` runs before the field's own keys (the kit's `TextField` has no key hook, so a
  focused one swallows a window's shortcuts), with an `on_change` hook and the field's issue
  under it as a danger status keyed `{key}-issue`.
- `appearance::set_appearance(current, dark)`: stores the appearance a window draws in and
  says whether it changed, for a window watch's `appearance` hook.
- `window::gpu_hint(tool)`: the hint for the log line that reports a window that could not be
  created.
- New module `keys`: `is_chord(text, key, tx)` and `is_newline(key)` (Return, with or without
  Shift), the two checks every tool's key handlers repeated.

## 1.0.0
- Initial release, moved out of `eh-gui` so every Rune tool shares one copy.
- The crate root: `Ds`, `Surface`, `surface`.
- `tokens`: `rgb`, the palette seeds and the `Submit` and light-muted overrides, the spacing
  scale, the radii, the type roles, `CONTROL_HEIGHT`, `HAIRLINE`, `FOCUS_RING_WIDTH`.
- `motion`: `FEEDBACK`, `CONTROL`, `CONTROL_EXIT`.
- `parts`: `field_label`, `caption`, `issue`, `warning`, `hairline`, `detail_row`,
  `EditorHeight`, `EditorSpec`, `lines_editor`, and `text_editor` (eh-gui's private
  `mono_editor`, now taking its type role from `EditorSpec::font`).
- New part `status_line`: a status caption in a muted, danger, or warning tone, wrapping or on
  one line, cut at 140 characters with the whole text as its accessible description.
- `appearance`: `SystemAppearance`, `AppearanceWatch`, `is_light`.
- `window`: `spawn_window_watch`, `WindowWatch`, `WatchHooks`, the pure `window_failure`, and
  `GPU_BACKENDS`.
- Feature `test-harness`: the headless `test_harness` helpers, generic over the state type,
  and the `contrast` helpers.

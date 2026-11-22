# Changelog

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

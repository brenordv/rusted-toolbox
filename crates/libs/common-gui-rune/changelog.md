# Changelog

## 1.3.0
- `drop_fill(k)`: a drop target's fill while a file drag hovers it, the accent at
  `tokens::DROP_FILL_ALPHA` (0.12). The kit's selection fill (alpha 0.3) drops muted, danger,
  and warning captions to about 3.2:1; on this fill the worst is 4.7:1 over Content, and the
  contrast test covers those captions and the accent edge around the box.
- `test_harness::tooltip_of(rt, key)`: rests the pointer on an element past the hover delay and
  returns the tooltip it shows, text and box.

## 1.2.1
- `parts::action_row` holds the status in a row, so a status element that grows (eh-gui's Send
  status) keeps its height and sits centred on the buttons; in a column it collapsed to no
  height and hung below the row's centre.

## 1.2.0
- `Theme`: the two kits and the appearance a window watch read, with `kit(system_dark)`,
  `dark(system_dark)` (the read value wins once there is one), and `set(dark)`, which says
  whether it changed. It replaces the `Ds` plus `system_dark` pair each window held.
- New parts: `action_row` (a hairline over a fixed row with the status leading and the buttons
  trailing), `choice_switch` (a segmented choice with its accessible name, picking by index,
  every segment disabled when it is), and, moved from eh-gui, `reveal` and `section_header`.
- `keys::area_chord(count, key, tx)`: the area `mod+1` to `mod+<count>` picks, on a fresh press
  only. `keys::type_newline(edit, key)`: Return types a line break at the caret.
- `motion::CONTROL_PAIR` and, moved from eh-gui, `tokens::REVEAL_OFFSET`.
- `window::run_watched(app, hooks, context)` runs an app under the window watch with no Rune
  flags read from the process; `window::report_failure(tool, reason)` writes the one log line
  for a window that could not be created.
- `test_harness` gains `runtime_with_keys`, `deliver`, `type_into` (press, check focus, select
  all, paste), `focus_is_inside`, `bottom`, `is_disabled`, `use_memory_clipboard`,
  `clipboard_text`, and `answer_dialog`.
- `Ds` draws the kit's `border_active` (the checkbox focus ring, a progress fill) in the accent,
  which clears 3:1 on every surface where the kit's lifted neutral did not, and light muted text
  is `#6B6B70` (was `#6E6E73`) so a neutral badge's label clears 4.5:1 on its fill. The contrast
  test covers the focus rings and the neutral and accent badge labels.

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

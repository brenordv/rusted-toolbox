# GUI design system (Rune)

The rules every windowed tool in this repo follows. A new GUI starts here: pick a window shape
(section 2), build it from rune-kit and this crate, and keep only the tool's own sizes and parts
in the tool's `design.rs`. On a shared rule this document wins; section 13 lists where eh-gui
differs today and why.

Shared code lives in this crate (`common-gui-rune`); names in `code` are its items unless a
section says otherwise. Section 7 lists the parts that still live inside one app and what moving
one involves. Rune APIs named here were read at the workspace's pinned rev,
`6cf05a0ee0f445c25a33ceeede066b78bd73ef6a` of `brenordv/rune` (links in section 15); a Rune bump
re-checks the facts in section 7. Pointer and keyboard only; nothing here is sized for touch.

## 1. Direction

The target is "what if Apple had shipped this tool": nothing branded, no SF fonts, no Apple
marks. What we borrow is the discipline, and each principle below has a checkable form in a
later section.

1. The content is the hero. Whatever the user came for (a message stream, a text to speak, a
   list of files) gets the space and the contrast. Chrome recedes: neutral surfaces,
   hairlines, no boxes around boxes.
2. Tint means "you can press this" or "this is live". One accent. Nothing decorative is
   tinted.
3. Motion explains. An animation answers "where did this come from" or "what just changed".
   Anything that happens constantly does not move.
4. Depth comes from material. Surface levels, then frosted glass on floating chrome only.
5. Every outcome has one place. A result shows where the user is looking. A toast is for an
   outcome nothing else can show.
6. The model refuses, the view explains. The transition itself rejects what is not allowed,
   and the reason is on screen as text.

Appearance follows the system. Dark mode is not an inversion: surfaces step lighter as they
rise and text never hits pure white. Every owner check and every posted snapshot is done in
both appearances.

## 2. Window shapes

Pick the smallest shape that fits.

| Shape | For | Anatomy | Example |
|---|---|---|---|
| Notice window | one message, one button | fixed size, not resizable, Window surface only, title, a scroll viewport for the body, a footer with the one button | `alert` (440 x 220) |
| Form window | one task, up to about eight fields | resizable with a minimum, a form column, a fixed action row at the bottom | `alert --gui`, `speak --gui` Settings |
| Tabbed tool window | two to four related tasks | an area switch in a fixed header row, one form, list, or editor area per task, `mod+1..n` | `speak --gui`, seal-gui |
| Workspace window | a stream or a large data set | toolbar, sidebar, content, inspector, floating overlays, toasts | eh-gui |

The first three shapes are covered here in full. The workspace shape's own rules (eh-gui's
three panes and their widths, its cadence, toasts, sheets, and quit rule) live with eh-gui until
a second workspace tool exists; section 13 names them.

Rules for every shape:

- `AppSpec::min_size` is set (Rune's default is 420 x 400) to the smallest size at which every
  area fits with no outer scroll in its rest state: no standing caption, no confirm, one-line
  values. The arithmetic is a comment beside the token in the tool's `design.rs`, and the
  layout sweep asserts it (section 11). The minimum height has a ceiling of 680, so the
  window fits a 768 pt screen; an area that needs more is made shorter (two short fields in
  one row, a smaller fixed box), not given a taller window.
- An area fits by letting one or two boxes take the leftover height: they are `Fill` with a
  minimum line count and share the leftover equally; every other box has a fixed line count
  and scrolls inside. A box never grows with its content. A box that only scrolls once it
  holds text does not take the wheel while empty. A standing caption, a confirm, and a
  wrapped value take their height from the outer scroll, which exists for them and for large
  text settings, not for the rest layout: the wheel goes to the innermost scroll viewport and
  is never handed up.
- The primary action is always on screen, in a fixed action row that never scrolls away. An
  area with no single primary action has no `Submit`: its action row carries the status line
  only, and each task's button sits at the trailing end of its own row or form.
- A row of numbers that change in place has a fixed width.
- A fixed-size notice window gives a late caption its height from the body's scroll viewport,
  never from the window.
- Text from disk or the network can be any length: failure text is cut and names are cleaned
  (section 8). A path that stands alone (a detail row, the output folder) wraps anywhere. A
  path in a list row is one line cut in the middle (`truncate_middle`), so its end and the
  file name's extension stay in view, with the whole path as a tooltip shown only when it was
  cut (`help_when_truncated`). Rune's tooltip breaks lines only between words, so the tooltip
  text carries a zero-width space between graphemes.
- The area switch is a `SegmentedControl`. With a toolbar it sits at the leading edge, after
  the app name. Without one it has its own fixed header row with `SPACE_LG` insets, aligned to
  the form column's leading edge. The kit's `TabStrip` is browser-style document tabs and is
  the wrong idiom. A choice inside an area (a mode, a direction) is a labelled field in the
  form, never on the header row.
- Each area keeps its own scroll position, field contents, focus, and status across switches.
- On macOS a toolbar may be the titlebar (`App::titlebar`). Windows and Linux keep the system
  title bar: on Windows 10, Rune's custom frame left a native caption whose close button did
  nothing. A window with no toolbar never sets a titlebar.

Empty states. A workspace content pane with nothing to show: a 48 pt glyph, one `display`
line, one `body` line in muted text, at most one action button, centred. Inside a list box or
a form area: one `TYPE_BODY` line in muted text, centred in the box, which keeps its height.
No glyph, no `display` line.

## 3. Color

### Seeds and overrides

rune-kit derives a palette from three seeds with `Kit::from_palette(PaletteSeed { bg, fg,
accent })`. `Ds` builds the two kits once and holds the overrides, each with its reason.

| Appearance | `bg` | `fg` | `accent` |
|---|---|---|---|
| Dark | `#161618` | `#F2F2F4` | `#3D8BFF` |
| Light | `#FFFFFF` | `#1A1A1C` | `#0A66E0` |

- `Submit` fill `#0050B8` in both appearances, with a `#FFFFFF` label: 7.4:1 at rest and 4.9:1
  under the sheen. It is darker than either accent because white on the dark accent is 3.3:1,
  and on `#0A66E0` it drops under 4.5:1 once the sheen composites. `#3D8BFF` stays the dark
  accent for text, tint, and focus.
- `Submit` sheen capped at alpha 0.08: the kit's sheen under the label drops it below 4.5:1.
  Hover and press darken in both appearances (black at 0.12 and 0.20); the light kit's white
  hover overlay would brighten the fill under the white label.
- Light muted text `#6B6B70`: the kit floors muted text against white only, and a neutral
  badge's label needs 4.5:1 on the badge's fill.
- A status color used as text gets a darker light-mode variant when the contract test finds
  it under 4.5:1.

Neutrals carry a slight cool bias. No app defines a color. A color a tool needs and the shared
set lacks is added to the library with its contrast pairs.

### Surface ladder

| Level | Use | Treatment |
|---|---|---|
| Window | toolbar band, sidebar, the whole of a notice or form window | dark: darkest layer; light: one step darker than white |
| Content | the main pane of a workspace; a list box, an editor, or a result box inside a form | dark: one step up; light: white |
| Raised | cards, popovers, a sheet body | dark: one more step; light: white with the hairline |
| Floating | a pill or bar hovering over content | blur 12, content tint at alpha 0.78, rim, shadow |

`surface(k, level)` maps a level to a kit role. In light mode the kit's layers step darker
toward `fg`, so the mapping is by result and the contract test pins the order. Content inside
a form marks where the user's material lives; it is never decoration. Glass appears only on
floating overlays.

### Semantic color

- Status colors are the kit's state colors: success, warning, danger, info. They mark status
  only, and never carry it alone: a status always has words.
- Selection is the kit's accent-alpha fill.
- Hairlines are the kit's weak border role at 1 pt (`hairline`). One between a fixed band and
  a scroll area runs the full width; one between groups in a column is as wide as the column.
- The focus ring is the accent at full alpha, `FOCUS_RING_WIDTH` 2, keyboard focus only. The
  contract test holds it to 3:1 on the three surfaces.
- No list row has a hover state. The buttons in a row do.

### Contrast contract

Unit-tested in both appearances. `contrast::Contract` (feature `test-harness`) holds the
helpers. The library tests every pair built from shared tokens:

- Text, 4.5:1: `fg`, muted, and accent text on the three surfaces; danger and warning
  captions on the three surfaces; the `Submit` label at rest, under the sheen, on hover, and
  pressed; each badge label on its fill; text on the drop fill.
- Non-text, 3:1 on the three surfaces: the focus rings (`border_focus`, and the kit
  checkbox's `border_active`, which `Ds` sets to the accent); the accent edge of a drop target.
  A tool tests the non-text pairs its own parts add: a border that is a control's only visual
  mark (an empty checkbox, an empty radio), a status dot, a muted icon.
- Not in the contract, per WCAG 2.2's Understanding note for 1.4.11 ("If a control has visible
  content (such as text or a sufficiently contrasting icon) then a border or other indication
  of the overall boundary of the hit area is not required"): the edge of a button with a text
  label, and a badge's fill, whose text carries the status. Measured at rev 6cf05a0e, the kit's
  `Outline` ring (`border_muted`) is 1.1 to 2.2:1, and the checkbox's empty edge
  (`border_weak`) 1.2 to 1.4:1. seal-gui's `ChoiceKits` draw the empty checkbox edge and the
  empty radio ring (`border_strong`, 1.23:1 in light) in `icon_muted`, and the selected radio's
  dot (`knob_on`, 2.96:1 on the dark accent) in white, tested in that crate. A second tool
  with a checkbox or radio moves `ChoiceKits` here (section 7).

A tool that adds a pair tests it in its own crate with the same helpers: a status color as
text on a new surface, text on a selection or drop-hover fill, text on a floating tint.

A failing pair is fixed by an override in the library, never by leaving it out of the test.
The test models the role the pixel really uses: a contract once passed while a button was
white on white because it measured the wrong kit role, and a snapshot caught it.

## 4. Typography

Inter (embedded by Rune) for UI text; the platform monospace (`Family::SystemMono`) for
anything copyable. Numbers use tabular figures.

| Role | Size / weight | Use |
|---|---|---|
| `TYPE_DISPLAY` | 20 / 600 | a workspace empty state's title only |
| `TYPE_METRIC` | 22 / 600, tnum | stat values (workspace windows) |
| `TYPE_TITLE` | 15 / 600 | window and sheet titles, a notice's title |
| `TYPE_HEADLINE` | 13 / 600 | app name, section headers |
| `TYPE_BODY` | 13 / 400 | form text, descriptions, prose the user writes or reads |
| `TYPE_BODY_NUM` | 13 / 400, tnum | a body-sized line of numbers that change in place; muted |
| `TYPE_LABEL` | 12 / 500 | field labels, buttons, segmented control, checkbox and radio labels |
| `TYPE_CAPTION` | 11 / 500 | column headers, secondary metadata, status lines, badges; muted unless it carries a tone |
| `TYPE_MONO` | 12 / 400, tnum, SystemMono | ids, paths, keys, URLs, timestamps, sizes in a column, payloads, armored text |

Weight carries emphasis before size. A header is `TYPE_HEADLINE` over `TYPE_BODY` or
`TYPE_LABEL` content. No text role outside this table. A copyable value is `mono`; prose stays
`body` even when it is selectable. The whole row of a dense list is `mono`, so it never mixes
two baselines. A line that fits in one platform's monospace can break in another's: leave room
or let it wrap.

## 5. Space, size, radius, elevation

Spacing, 4 pt base: `SPACE_XS` 4, `SPACE_SM` 8, `SPACE_MD` 12, `SPACE_LG` 16, `SPACE_XL` 24,
`SPACE_XXL` 32.

Sizes: `CONTROL_HEIGHT` 28, the minimum pointer target for a control (24 for a button inside a
list row); `HAIRLINE` 1; `FOCUS_RING_WIDTH` 2; icons 16 pt with a 1.5 pt stroke.

### The form recipe

- The form column is centred with `SPACE_LG` insets and a maximum width token in the tool's
  `design.rs` (472 in speak). An area whose hero is a list or an editor sets a wider maximum
  (640 fits 64-column armored text in `mono` with its padding).
- A `field_label` sits above its field with `SPACE_XS` between; an `issue` or hint caption
  sits under the field with `SPACE_XS`. Rows are `SPACE_MD` apart. Groups are `SPACE_XL`
  apart with no hairline between them.
- A group opens with a `section_header` (`TYPE_HEADLINE`, sentence case, `SPACE_SM` below).
  The header row may carry one `Ghost` action at its trailing end.
- Fields fill the column. A number or short code field has a width token.
- The action row is `CONTROL_HEIGHT` plus `SPACE_MD` above and below, at least. Its hairline
  spans the window; its status and buttons align to the column's edges.
- A checkbox or radio row (the control, `SPACE_SM`, the label) is at least `CONTROL_HEIGHT`
  tall and the whole row toggles. The control alone takes focus and is the one accessibility
  node. Radio options stack `SPACE_XS` apart, and the group is named by its field label.

### Lists in a form

A list is a box on the Content surface with a hairline border, `RADIUS_CONTROL`, and a
line-count token. A row is at least `CONTROL_HEIGHT` tall with `SPACE_SM` inside: a leading
badge or checkbox in a fixed-width column, the main text, then trailing `Ghost` buttons,
top-aligned when the message under the main text wraps. A row's message is a caption under the
main text, aligned to it. Rows are `SPACE_XS` apart with no hairlines, no hover, and no
selection unless the list selects. A cut never hides what the row is acted on by: the row's
accessible name and its buttons' names carry the file name or label whole.

A list that can hold hundreds of rows builds only the rows in view (`El::virtual_rows`). The
height it is given is an estimate; Rune measures each row once laid out, so rows may differ in
height. Rows are keyed so a handler finds its item by key or id, never by index, and a row out
of view has no accessibility node and no place in the Tab order until it is built.

A drop target is the list box. At rest it shows nothing extra. While a file drag hovers, its
border becomes the accent at `FOCUS_RING_WIDTH`, drawn inside the box so nothing moves, and
the box takes `drop_fill` (the accent at `DROP_FILL_ALPHA` 0.12; the kit's selection fill at
0.3 drops muted, danger, and warning captions near 3.2:1). No dashed border.

### Radius

| Role | Token | Radius |
|---|---|---|
| Badges | `RADIUS_BADGE` | 4 |
| Row selection highlight | | 6 |
| Buttons, fields, segmented control, list boxes | `RADIUS_CONTROL` | 7 |
| Cards, tiles, popovers, toast cards | `RADIUS_TILE` | 10 |
| Sheets and dialogs | `RADIUS_SHEET` | 14 |
| Floating pill | | capsule |

Content has no shadow. Floating things get two shadow layers, a 1 pt rim, and a top highlight;
the values sit with eh-gui's floating parts until a second tool needs them. Shadows never
animate.

A literal for a color, size, radius, shadow, or spring lives in `common-gui-rune` or in the
tool's own `design.rs` (window sizes, line counts, widths only that tool has). A value two
tools use moves to the library.

## 6. Motion

### Roles

Assigned by role, never ad hoc. A view never calls `Spring::new`.

| Role | Rune preset | Period / bounce | For |
|---|---|---|---|
| `FEEDBACK` | `INSTANT` | 0.10 / 0 | press feedback, toggles |
| `CONTROL` | `SNAP` | 0.25 / 0 | segmented indicator, area swap, field reveal, status swap |
| `LAYOUT` | `SETTLE` | 0.30 / 0 | a surface's height reflow |
| `SURFACE` | `REFLOW` | 0.40 / 0 | sheets, a workspace empty state's entrance |
| exits | the preset's `_EXIT` variant | | always quicker than the entrance |

`FEEDBACK`, `CONTROL`, and `CONTROL_EXIT` are in `motion`. `LAYOUT` and `SURFACE` are in
eh-gui's `design/motion.rs` and move here with the first shared part that needs them. No
bounce anywhere; `MORPH` and `POP` are off-limits. Always pass a role to a kit component that
animates: the kit's defaults bounce (`RollingText`) or spring where the spec says still (the
kit `TextArea`'s height).

### Interaction table

Every animation in an app is a row here, or in the table its tool keeps for what only it has.

| Interaction | Frequency | Treatment |
|---|---|---|
| Press the one `Submit` | frequent | `.press_scale(0.97)` on `FEEDBACK`, on pointer down |
| Press any other button | constant | the kit's pressed overlay, no scale |
| Typing, keyboard selection, rows arriving | constant | none |
| Area switch | frequent | the segmented indicator slides (kit); content swaps with `Transition::opacity` on `CONTROL` |
| A field or group appears because a choice changed | occasional | `reveal`: opacity plus a 6 pt rise on `CONTROL`, quicker exit |
| Status line changes | frequent | `Transition::opacity` on `CONTROL`, keyed on the state, never on the text; a count inside it updates in place |
| Field validation message | frequent | the caption and the field's invalid mark appear with the state change; no motion |
| Copy done | frequent | none; the status line says what was copied |
| A button goes busy | occasional | the kit's `Loading` variant |
| Sheet or modal open and close | occasional | opacity plus an 8 pt offset and scrim fade on `SURFACE` |
| Drop target while a file drag hovers | occasional | border and fill change at once |
| Theme switch | rare | Rune's built-in color ease |
| Window resize | constant | layout follows at once |

Rules that came from defects:

- A commit no input caused lands instantly in Rune. A background answer that should not
  animate (rows, a restored value) uses `tx.instant()`; the rare one that should (a toast in a
  workspace window) asks with `tx.animated()`.
- Motion and live regions key on the state, not on text that changes every tick.
- Nothing dims for a load that takes milliseconds. A load the user can perceive shows
  `Loading` on its button and disables what cannot be used meanwhile.
- Hover and press never re-run the view. Never mirror hover in state.
- Reduced motion: Rune snaps travel and keeps a 0.15 s fade. An app adds a rule only for a
  loop it starts itself.

Performance: the commit budget is 8 ms (`RUNE_COMMIT_BUDGET_MS=8`), measured in a release
build, first commit excluded. A view clones no large value and logs nothing per frame. A
background stream that can post hundreds of events (per-file progress), and a list that can
grow to hundreds of rows, are measured at their worst case before they ship: the longest text,
the smallest window, the list scrolled end to end.

## 7. Components

rune-kit first; a shared part where the kit has a gap; an app part only for something one
tool has.

### Kit components to use as they are

`Button` (`Submit`, `Neutral`, `Outline`, `Ghost`, `Loading`), `IconButton` (always with
`.label`), `SegmentedControl`, `Select`, `Checkbox`, `RadioGroup`, `Badge`, `Progress`,
`Spinner`, `Popover`, `SplitView`, `Toolbar`, `TextInput`.

Buttons. One `Submit` per surface, at most. Secondary actions are `Neutral` or `Outline`;
inline, row, and header actions are `Ghost`. A secondary action sits at the trailing end of
the row it acts on. A button's label never changes to report a result.

Badges. The label is `TYPE_CAPTION` in sentence case, in a fixed-width column so the text
beside it aligns. Neutral: waiting. Accent: running. Success, warning, and danger fills from
the state colors: finished; skipped or invalid; failed. A badge never repeats a value shown
elsewhere in the row.

Confirms. An action that cannot be undone asks once, in place.

- Something the user can get back (a list entry, a public key someone can send again): the
  row's button changes to the confirm ("Delete" to "Confirm delete"), in danger text with a
  danger border, both pairs in the contract.
- Something the user cannot recreate (a private key file, the only copy of anything): a
  separate confirm, a `warning` caption that names what is lost with an `Outline` confirm and
  a `Ghost` Cancel as their own controls, so a double-click cannot pass it.
- A named set of the user's inputs disarms a confirm; background answers never do. Return
  never passes a confirm it raised. Focus does not move when a confirm appears.
- The guard is also where the damage would happen: the code that writes refuses to replace
  unless the request says the user confirmed.

### Shared parts (`common-gui-rune::parts`)

| Part | What it is | Why not the kit |
|---|---|---|
| `field_label`, `caption`, `issue`, `warning` | the text roles of a form | |
| `hairline` | the 1 pt separator | |
| `detail_row` | caption over a selectable `mono` value that wraps anywhere | |
| `status_line(k, key, tone, lines, text)` | one status caption (muted, danger, warning), cleaned, cut at 140 characters, a polite live region whose description is the whole text | |
| `line_field` + `LineFieldSpec` | a labelled single-line field with key and change hooks | the kit `TextField` has no key hook and swallows `mod+enter` |
| `text_editor` + `EditorSpec` | a multi-line editor that takes its type role and a `Fill` or `Fixed { lines }` height | the kit `TextArea` fixes its font and springs its height per line |
| `lines_editor` | a fixed-height `key=value` box that scrolls only once it holds text | |
| `section_header` | a group's `TYPE_HEADLINE` title | |
| `action_row(k, key, status, buttons)` | a hairline over the fixed row under an area: the status leading at the width the buttons leave, buttons trailing | |
| `choice_switch` | a `SegmentedControl` with its accessible name, picking by index; every segment disabled when the choice is | the kit has no control-wide disabled state |
| `reveal` | the field-reveal transition (`motion::CONTROL_PAIR`, `REVEAL_OFFSET`) | |

Beside them, at the crate root and in other modules: `Ds`, `Theme`, `Surface`, `surface`,
`drop_fill`; `keys::is_chord`, `keys::is_newline`, `keys::area_chord`, `keys::type_newline`;
`appearance::SystemAppearance`; `window::run_watched`, `spawn_window_watch`, `window_failure`,
`report_failure`, `gpu_hint`. Behind the `test-harness` feature: `test_harness` and `contrast`.

### Parts that still live in one app

A tool that needs one of these moves it, with its tokens and its tests, into this crate in the
same change; it never copies it. Each one was kept in its app because one app used it.

| Part | Lives in | What it is |
|---|---|---|
| `ChoiceKits` | seal-gui `src/design.rs` | the kits that draw an empty checkbox edge, an empty radio ring, and a selected radio's dot at 3:1 |
| `ConfirmKits` | seal-gui `src/design.rs` | the kits that draw an in-place "Confirm delete" in danger text at 4.5:1 in every state |
| `list_box`, `ListRows`, `ListHeight`, `list_height` | seal-gui `src/views/parts.rs` | a list in a form: the box, its empty text, built or virtual rows, a row count or `Fill` height |
| `badge`, `BadgeTone` | seal-gui `src/views/parts.rs` | a status badge in a fixed-width column, with its five tones |
| `drop_hovered` | seal-gui `src/views/parts.rs` | a list box's look while a file drag hovers it |
| `output_box` | seal-gui `src/views/parts.rs` | a read-only, selectable `mono` result box |
| `breakable` | seal-gui `src/views/files_tab.rs` | text with a zero-width space between graphemes, for a tooltip that must wrap a path |
| `LAYOUT`, `SURFACE`, `SURFACE_PAIR` | eh-gui `src/design/motion.rs` | the reflow and sheet motion roles |
| `sheet`, `SheetFrame` | eh-gui `src/design/parts.rs` | a modal sheet that scrolls (the kit `Dialog` is a fixed 368 pt panel that cannot) |
| `labeled_switch` | eh-gui `src/design/parts.rs` | a switch with its label at 4.5:1 (the kit's is under) |
| `popover_body`, `form_popover` | eh-gui `src/design/parts.rs` | popover bodies wider than the kit's 320 pt cap, shrinking to the window |
| `empty_state`, `empty_state_with_note` | eh-gui `src/design/parts.rs` | the workspace empty state |
| `floating_pill`, `floating_bar`, `floating_tint` | eh-gui `src/design/parts.rs`, `src/design/mod.rs` | floating overlays on frosted material |
| `toast` and the stack | eh-gui `src/design/toast.rs` | toast cards with their hold and focus rules |
| the error details sheet | eh-gui `src/views/details.rs` | the whole failure text behind "View details" |
| `lucide` and the icon set | eh-gui `src/design/icons.rs` | Lucide icons (ISC), 16 pt, 1.5 pt stroke |

### Kit and Rune facts every app meets

- Plain Return types nothing in a multi-line field with no submit hook and presses the
  window's default button instead; the field's key handler inserts the newline
  (`keys::is_newline`). The change hook does not run for that insert.
- A focused field consumes keys before a window-level handler sees them. A shortcut that must
  work while typing is handled in the field's own key hook (`keys::is_chord`).
- Return presses the default button, and Space a focused button, before the window key
  handler; `Tx::by_keys()` tells those from a pointer press.
- Assigning a secure field's text does not wipe its buffer. Call `TextEdit::clear()`.
- A scroll viewport only shrinks when every box between it and the bounded ancestor has
  `min_h(0)`.
- Rune runs a press only on the topmost element with a handler; nothing bubbles.
- A pressable element is not focusable unless made so. A disabled element takes no focus but
  keeps its press handler unless the view leaves it off.
- There is no initial-focus API. Focus the first field with `Tx::focus` from the watch's
  `shown` hook, and return true from the hook so the commit happens.
- A view cannot measure text. State can (`rune::text::measure_with`); otherwise cut by
  characters under a width cap, or let Rune cut at the width (`truncate`, `truncate_middle`).
- A tooltip (`El::help`, or `help_when_truncated` for one shown only while its text is cut) is
  AppKit's on macOS. Elsewhere Rune draws it, at most 300 pt wide, breaking lines only between
  words.
- `AppSpec::title` is static; a run-time title is set with `Tx::set_title` from `shown`.
- Rune reads its own flags from the process arguments. Every tool passes `App::args` an empty
  list so clap owns the command line, and tests that a Rune flag is a usage error.
- `Tx::open_file` returns one path. There is no multi-file dialog at the pinned rev. A dialog
  answers through a callback, so the app tracks that one is open and refuses a second.
- OS file drop (`El::on_drop_files`, `El::on_drop_hover`) works on macOS, Windows, and X11.
  Wayland reports no file drags, so a drop target always has a button that does the same.
- Workspace windows also meet: the kit `Select` keys its field `<key>-trigger`, has no
  disabled option, and needs an app-owned open flag; `popover::fit_scroll` must be the popover
  body's direct child; the kit paints overlays in push order, so a picker inside a modal
  mounts its overlays after the modal and the top modal is pushed last.

### Icons

16 pt, 1.5 pt stroke, monochrome in the muted text color, tinted only on the active control.
Icons come from Lucide (ISC, credited in the crate that draws them). A form or tabbed tool is
better with a word on the button.

## 8. Words

- Buttons are verbs that name their object when two could be confused ("Choose folder…",
  "Add recipient"). Sentence case everywhere, badges included. No exclamation marks.
- An error says what failed and what to do next, in that order, and names the stage.
- A refusal names only what the user can see.
- A message claims only what the code knows, and stays true for as long as it shows.
- The same failure has the same words wherever it shows. Singular and plural are written once.
- Numbers: thousands separators; sizes `1.2 KB`; durations `2m 14s`; recency `2s ago`.
- Text from outside the program (a server's error, a name or path from disk, a value from a
  hand-edited config) goes through `common_utils::string_utils::display_line` wherever it is
  shown: on screen, in a tooltip, in an accessible name or description, in a window title. In
  a log field it is recorded escaped (`?value`), never raw.
- Failure text shows at most 140 characters in place, then "…" (`error_preview`). In an action
  row it may take two caption lines, which fit `CONTROL_HEIGHT`, wrapping anywhere. A workspace
  window adds a Ghost "View details" that opens the whole text in a sheet. Other shapes have
  no details sheet: the whole text is the status line's accessible description and is in the
  log.
- No error text repeats what was typed when that could be a secret. Use fixed wording.
- An issue line is drawn from state and clears when its field is edited. When two fields feed
  one check, editing either clears the shared issue.
- "Details are in the log file" appears only when file logging is on.

### Outcomes and where they show

| Outcome | Where |
|---|---|
| A field is wrong | an `issue` caption under the field, and the field is marked invalid, so color is not the only carrier |
| The action is blocked by something the user fixes in this area's fields | the `Submit` stays enabled; the press marks the fields, moves focus to the first, and the status line says what is missing |
| The action is blocked by something a press cannot fix (a job running, a stopped worker) | the `Submit` is disabled and the reason shows |
| An action is running | the button's `Loading` state; the status line says what is running, with a count that updates in place |
| An action finished or failed | the status line of the area it belongs to |
| Something was copied | the status line names what was copied |
| Another area's job is running or failed (tabbed tool) | this area's status line says so by area name ("Files: sealing 3 of 12", "The Files run failed. See Files."); another area's success is not reported |
| A condition that stays (settings cannot be saved, a worker stopped) | a standing caption, in the warning or danger tone |
| An outcome whose surface is not on screen (workspace window) | a toast |

One status line per area, one thing at a time, highest first: what is running here; what is
running in another area, by area name; the last outcome shown here (a result, a failure, a
copy, a dialog error, what a refused press found missing, another area's failure), kept
until the user's next input in this area; nothing. A switch between areas is not an input in
either, so an outcome that landed while its area was hidden is there on arrival. A disabled
`Submit` thus always has its reason in view: the running line, or a standing caption. The
status line explains the `Submit` only: any other disabled control has its cause visible
beside it (an empty box, a job named in the status line) or a caption under its row.

A standing caption for a window-wide condition is a full-width row between the scroll area
and the action row's hairline, aligned to the column, at most two lines, danger above warning,
announced once when it appears. One tied to a field is a caption under that field.

A control that changed state is its own confirmation. Nothing is said twice.

### Secrets and the clipboard

- A stored secret never enters a text field. The form shows that one is stored and offers to
  replace it.
- A typed secret is cleared from its field once it has been used.
- No UI action copies a credential to the clipboard.
- A tool that shows the user's own sensitive output decides, as a recorded decision, between
  `Tx::copy` and `Tx::copy_secret` (kept out of clipboard histories, cleared after a time
  held in a token). With `copy_secret` the status line says when the copy leaves, and the
  tool checks what a selection copy of the same text does, since selectable text is a second
  way to the clipboard.
- Failure text is credential-free where it is made.
- Anything that holds user text has a hand-written `Debug` that prints lengths, not content.
  That includes inputs, effects, and background messages that carry the text.
- A screenshot or snapshot shows temp paths with the OS user name. Crop before posting.

## 9. Accessibility and keyboard

- Rune exposes elements through AccessKit. Secure text is never exposed. A virtual list
  exposes mounted rows only.
- Every control has a name. An `IconButton` carries `.label`. A group of fields under one
  heading is a `group` named by the heading, with the visible label hidden from assistive
  technology so the name is read once.
- A list row is one named node: its status, then its main text. A button in a row carries the
  row's main text in its name ("Remove report.pdf"). A value shown cut carries its whole text
  as its description.
- A status is a `status` with a polite live region, named by the text it shows. An empty
  status is not mounted, and its row keeps its height. One live region changes per
  transition; when two would, the more specific one (a confirm line) wins.
- A line that changes while the user types, or every second, is not a live region. A counting
  line gets a separate phrase per state, with no digits in it.
- A caption that reports an outcome is the live region for it.
- A notice window's root is a `dialog` named by its title and described by its body.
- A disabled control's reason is visible text (section 8), not only a tooltip. A busy button
  reads as disabled; inert elements leave the accessibility tree.
- Overlays mount nothing while closed.

Keyboard:

- `mod+1..n` switches areas, also from a focused field. `mod+enter` runs the area's primary
  action from any of its fields; in an area with no `Submit` it runs the form that has focus.
  Chords come from `Chord::parse` (Ctrl on Windows and Linux, Cmd on macOS).
- Return in a single-line field of a form presses that form's button. Return in a multi-line
  field types a newline and never submits.
- Escape closes the topmost layer, one layer per press. With no layer open it does nothing:
  it never closes the window and never clears a form.
- No shortcut acts behind a modal.
- Tab order follows reading order. A press that finds issues moves focus to the first field
  with one and scrolls it into view (`Tx::focus` does not scroll; `Tx::scroll_into_view`
  does).
- Focus is requested only for a control that the same transition leaves mounted and enabled.
- An area switch puts focus on the control that area last had focused, else its first field.
- When the user's own action unmounts the focused control (a removed row, a confirm that
  goes away), the same transition names the landing: the next sibling of the same kind, else
  the previous, else the control that adds one.
- A modal returns focus to its opener. When the opener can be gone or inert by then, the
  close path names a surviving control instead, on every way out.
- A window that can take focus while the user types in another app (a notice) guards its
  keys: Return, Space on the default button, and Escape close it only as a fresh press at
  least one second after it was first on screen. A pointer press always works.
- Route keys by focus, never by hover.

## 10. How an app is built

One architecture, sized to the window.

- State is one struct the app owns, with `detached()` for `AppSpec` and `new(..)` for the real
  start. Anything valid in only one state lives in that state's enum variant. A status is an
  enum matched in full, with no catch-all arm.
- `Input` is everything that can happen: a user action, a background answer, the window
  closing. `apply(&mut self, Input) -> Outcome` is pure: no `Tx`, no file, network, or clock
  read. Time arrives as a value in the input. `Outcome` carries whether the view must re-run,
  the effects to start, and a focus request if the transition names one.
- `Effect` is work for the services. `trait Services` has one method per kind of work.
  Background work returns at once and reports back as an `Input`; a service that cannot start
  its work returns the failure, and the effect runner applies it in the same transition.
  Production services post through `AppHandle::update_if`; tests use a recording
  implementation that keeps whole requests.
- A view is a pure function of the state. Handlers call `dispatch(state, tx, input)`. A
  handler that needs `Tx` for something only it can do (open a dialog, copy) first asks the
  same guard `apply` uses. A handler may set a flag directly only when the flag disables no
  control and gates no request.
- No network, database, or long file work in a view, a handler, `apply`, or `on_close`. Long
  means anything that reads or writes the user's files, or whose time grows with their size.
  A short, bounded file-system call (a stat, an existence check, writing a small settings
  file) may run in the effect runner on the UI thread; it never runs in a view or in `apply`.
  eh-gui keeps the strict form (section 13).
- A notice window with no background work may skip the `Input` layer and keep plain methods
  on its state, with side effects passed in as closures (alert does).

Rules for background work:

- Every request gets exactly one final answer, on every background thread, including when
  the task or thread dies. One function turns a dead task into the failure answer.
- The transition is the guard. A second request while one is running is refused in `apply`,
  because a shortcut reaches `apply` without the disabled button. The view reads the same
  function the guard reads to show the reason.
- One action at a time needs no request ids when three things hold: the pending mark is
  cleared only by that request's own final answer; the answer's type names its request kind,
  so no answer type serves two requests; one thread serializes the work. Then refuse the
  second request and ignore an answer that arrives with nothing pending. If the pending mark
  can be cleared another way (a cancel, a timeout, a failure made up locally while the real
  answer can still arrive), or several kinds of work are in flight, answers carry an id or
  generation checked against state.
- An answer that changes what a list or caption shows refreshes it in the same transition.
- A save that derives values writes them back into the form.
- One clock. Tests pass time as a parameter and never sleep.
- An app with no tick has no timed UI. A tick (1 Hz in eh-gui) exists only where something on
  screen counts or expires.
- A background failure is logged by the thread that hit it, before it posts; the window may
  be gone when the answer arrives. `apply` and `dispatch` log nothing for an answer.

Window lifecycle:

- `App::on_close` cannot stop a close. Work that must finish either runs to completion after
  the window is gone, with the entry point waiting on it and late posts dropped, or the app
  keeps running and reopens (`keep_running`, eh-gui's quit rule). Pick the first unless losing
  the work loses user data.
- A thread the entry point waits on is stopped by a message it reads in order, not by hoping
  its channel closes: the state that holds the sender may outlive the event loop.
- A tokio runtime that owns blocking work is ended with `shutdown_background` after the event
  loop returns; dropping it waits without limit.
- Rune keeps running without a window when it cannot create one. `spawn_window_watch`
  detects that (`window_failure`, 2 s grace), quits, and hands the entry point the reason,
  which is logged once with `gpu_hint` before exit 1. The `WindowWatch` must stay alive for
  the run.
- Windows does not deliver Rune's theme change on Windows 10; the watch polls `UISettings`
  once a second and the view reads `system_dark.unwrap_or(cx.system_dark())`.
- A GUI logs to a file at `info` by default, because nobody watches a window's stderr. It
  writes a start line, so an empty log means logging is broken, and a line when a job starts
  and when it ends, so a hung job shows as a start with no end. `common-cli`'s
  `gui_boot::boot_gui` turns the file on and installs a panic hook that logs the message,
  location, and thread before the default hook runs. The log never holds the user's text, a
  URL with credentials, or a secret; it holds counts, hosts, paths the user chose, and failure
  chains.

## 11. Testing

What a headless test proves: the joins between view, state, and services; the accessibility
tree (roles, names, live regions, descriptions); geometry at chosen window sizes; where focus
lands; clipboard content through `rune::clipboard::memory()`; file dialogs
(`Runtime::file_dialog`, `resolve_file_dialog`) and file drops (`Runtime::drag_files`,
`drop_files`); tooltips (`test_harness::tooltip_of`).

What it cannot prove, and what the owner checks in a window instead: pixels and colors, light
mode, per-OS font widths, real motion timing, Reduce Motion, screen reader speech, dead keys
and IME, a real UI Automation client, and the event-loop entry code.

Techniques that caught real bugs:

- Type through the event loop (press the field, assert focus, paste), never through the edit
  API.
- Test every shortcut with a field focused.
- Use a fixed settle of 30 frames; a spinner never settles. Set the runtime's `on_key` or
  window keys do nothing.
- One directed `apply` test per state-table row, rejected transitions included. A journey per
  user task through the view.
- A layout sweep over the app's states at the minimum and the default size, with worst-case
  text (a 300-character path, a 2,000-character failure). It asserts that in the rest state
  the outer viewport does not scroll at the minimum size and the box a result lands in is
  inside the window, and that with captions, confirms, and worst-case text every button can
  still be hit and the outer scroll reaches every box. A live finding adds its row.
- An accessibility sweep asserting that every focusable node has a name.
- Positive controls before a negative assertion. Measure the viewport, not the surface around
  it. Prove a new test by putting the bug back by hand once.
- Pure decision functions take their boundary as a parameter (`window_failure` at 1,999 ms and
  2 s).
- A hygiene test for any tool that handles a secret or the user's private text: capture at
  the most verbose level, cover a success as well as a failure, and look in the log, the
  status text, the accessible descriptions, and the `Debug` of every recorded input.
- Log sites sit in functions a test can call on its own thread, so a scoped capture sees
  them.
- Seeded random walks over the rendered UI were fragile. Prefer directed tests.
- A frame-time measurement is an ignored test run by hand in a release build
  (`cargo test -p <tool> --release --lib -- --ignored --nocapture <name>`), reporting median,
  p95, and max per pass.

Feature-gated test and demo code has its own clippy and test lines in CI. A demo binary is
worth its cost for a workspace window whose look is decided from snapshots; small windows
have not needed one.

## 12. Review rules

Reviews treat each as a defect. The check that backs a rule a diff cannot show is named.

1. A color, size, radius, shadow, or spring literal outside `common-gui-rune` and the tool's
   `design.rs`.
2. An animation with no row in section 6 or the tool's own table; a spring instead of a role;
   any bounce.
3. More than one `Submit` on a surface (checked in the owner's window pass; no headless check
   exists). Tint on a decoration.
4. Frosted material on anything but a floating overlay.
5. A text role not in section 4; a copyable value not in `mono`.
6. Network, database, or long file work in a view, a handler, `apply`, or `on_close`.
7. A toast for something the UI already shows; a credential reaching the clipboard.
8. A guard that exists only as a disabled control, or only as a check made before the write
   on another thread (backed by a rejected-transition test per guard).
9. A catch-all arm on an app-owned enum; data valid in one state stored beside the status; a
   flag that mirrors a status.
10. A request with a path that never answers (backed by the dead-thread test).
11. Outside text shown without `display_line`, or failure text shown uncut.
12. A live region on text that changes while typing or every second; a focusable node with no
    name (backed by the accessibility sweep).
13. A second copy of something `common-gui-rune` has, or of a part section 7 lists in another
    app; a third copy of anything.
14. A shortcut tested only with nothing focused; a test that calls the edit API instead of
    typing.
15. An area whose rest state scrolls at the minimum size; a disabled control with no visible
    reason; a focused control unmounted with no landing; a list, badge, or hover-fill pair
    missing from the contrast test (backed by the layout sweep and the contract).

## 13. Where eh-gui differs

eh-gui is a stream viewer and keeps rules no small tool inherits: the three-pane anatomy and
its widths, `CONTENT_MIN` and the pane-yield order, the 1 Hz display cadence and rolling
numbers, the list caps and flush, the live dot, the floating pill and bar, signature moments,
the toast stack with its hold and focus rules, the error details sheet, the quit rule with
Force quit, request ids on every answer, the connect sheet and profiles, and the seeded
sequence suite.

Where it differs from this document on a shared rule. Each difference stands until eh-gui is
next changed there; none is copied by a new tool.

| Topic | This document | eh-gui | Why it stands |
|---|---|---|---|
| Copy done | the status line says so | the button's label crossfades to "Copied" with a drawn check for 1.5 s | eh-gui has a tick to time it |
| Field validation | appears with the state change | the caption fades in | shipped and reviewed; cosmetic |
| Status swap | `Transition::opacity` | `blur_replace` on its small action-row block | shipped; a small element |
| File work | short bounded calls may run in the effect runner | none in a view, a handler, or `on_close` at all | eh-gui's file work is a database and a network |

## 14. Open points

1. Light mode has no automated check beyond contrast pairs. It is a standing owner check.
2. "One `Submit` per surface" cannot be asserted headlessly.
3. The shared logger has no file-only mode, so a GUI's `info` lines also reach stderr.

## 15. References

Rune at `6cf05a0ee0f445c25a33ceeede066b78bd73ef6a` (base
`https://raw.githubusercontent.com/brenordv/rune/6cf05a0ee0f445c25a33ceeede066b78bd73ef6a`):

- Model, motion, reduced motion, fonts, accessibility, secure text: `/crates/rune/README.md`
- Components and contrast floors: `/crates/rune-kit/README.md`
- `App::on_close` ("Runs whenever the window closes ... before it goes"), `App::keep_running`,
  `AppHandle::update`, `update_if`, `quit`, `AppSpec::poll`: `/crates/rune/src/app.rs`
- File drop (`on_drop_files`, `on_drop_hover`, `accepts_files`; "macOS, Windows and X11.
  Wayland reports no file drags in winit 0.30"): `/crates/rune/src/el/drop.rs`
- Headless drops (`drag_files`, `drop_files`): `/crates/rune/src/runtime/input/drop.rs`
- File dialogs (`Tx::open_file`, `save_file`, `open_folder`; `FileDialogOptions { title,
  directory, file_name, filters }`; `FileDialogResult = Result<Option<PathBuf>,
  FileDialogError>`): `/crates/rune/src/file_dialog.rs`
- Headless dialogs (`file_dialog`, `resolve_file_dialog`):
  `/crates/rune/src/runtime/input/file_dialog.rs`
- Clipboard (`copy`, `copy_secret`, `read_clipboard`): `/crates/rune/src/clipboard/tx.rs`
- Selectable text, truncation (`El::selectable`, `truncate`, `truncate_middle`, `max_lines`):
  `/crates/rune/src/el/text.rs`
- Tooltips (`El::help`, `help_when_truncated`), `El::disabled`: `/crates/rune/src/el/help.rs`,
  `/crates/rune/src/el/a11y.rs`; tooltip layout and width: `/crates/rune/src/runtime/input/help.rs`,
  `/crates/rune/src/runtime/input/help_look.rs`
- Virtual lists (`El::virtual_rows`, `virtual_rows_version`, `on_visible_rows`):
  `/crates/rune/src/el/virtual_rows.rs`
- Secure input (`TextInput::secure`, `revealed`): `/crates/rune-kit/src/text_input.rs`
- `SegmentedControl`: `/crates/rune-kit/src/segmented.rs`; `TabStrip`:
  `/crates/rune-kit/src/shell/tab_strip.rs`
- Window options (`always_on_top`, `resizable`, `min_size`, `max_size`):
  `/crates/rune/src/window/spec.rs`
- License MIT: `/Cargo.toml`

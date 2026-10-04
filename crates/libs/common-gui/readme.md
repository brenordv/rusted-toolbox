# common-gui

The workspace's egui design system. Every egui tool in this repo builds its
views from this crate instead of styling anything by hand. It has three
modules and no dependency beyond `egui` itself (notably: no `eframe`, so any
egui host can use it):

- `tokens`: the single source of truth for spacing, typography, and color.
  A 4px spacing scale (`SPACE_XS` 4 through `SPACE_XL` 24), four type roles
  (body 13, small 11, heading 15, mono 12.5, all egui bundled fonts), and the
  fixed dark `Palette` (three background steps, a border grey, two text
  greys, one accent, three status colors).
- `theme`: `build_style(&Palette)` produces the whole `egui::Style` (pure,
  unit-tested); `apply_theme(ctx, &palette)` installs it and pins the dark
  theme. Call it once, in the app constructor.
- `widgets`: the component recipes views compose: `section`, `form_row`,
  `primary_button`, `ghost_button`, `status_badge`, `toolbar`, and the toast
  model (`Toast`, `ToastKind`, `show_toasts`).

## The six styling rules

These bind every consumer; reviews treat a violation as a defect.

1. Spacing comes only from the token scale. A layout that wants 10px actually
   wants `SPACE_SM` or `SPACE_MD`.
2. One accent color. Status colors mark status, never decoration.
3. Text gets one of the four type tokens; no ad-hoc `RichText::size`.
4. Every container is `bg_base`, `bg_panel`, or `bg_raised`. Depth comes from
   those three steps plus `border`, never from shadows.
5. Monospace for anything a user might copy: keys, armored text, paths.
6. Interactive affordance comes from the five globally themed widget states;
   recipes never hand-roll hover colors.

A `Color32::from_rgb` or a literal pixel value outside this crate is a review
defect.

## Toasts

The app owns a `Vec<Toast>`; call `show_toasts(ctx, &mut toasts)` as the last
thing in the frame. It drops expired entries, renders the rest bottom-right
(newest first), and schedules exactly one repaint for the earliest remaining
expiry, floored at 50 ms. The floor matters: asking an on-demand renderer to
wake sooner than a frame budget turns the timer into a continuous repaint
loop.

All coordinates and token values are logical points. egui applies the
display's pixels-per-point itself; never multiply by a scale factor.

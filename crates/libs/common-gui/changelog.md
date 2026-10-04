# Changelog

## 1.0.0
- Initial release.
- `tokens`: 4px spacing scale, four type roles over egui's bundled fonts, and
  the fixed dark palette.
- `theme`: pure `build_style` plus the one-call `apply_theme` installer; all
  five widget states themed explicitly.
- `widgets`: `section`, `form_row`, `primary_button`, `ghost_button`,
  `danger_button` (the destructive-confirm shape), `status_badge`, `toolbar`,
  `drop_target` (accent-stroked while a drag hovers), and the toast model
  with a single scheduled repaint wake (floored at 50 ms).

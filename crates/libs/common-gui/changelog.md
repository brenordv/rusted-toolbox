# Changelog

## 1.0.0
- Initial release.
- `tokens`: 4px spacing scale, four type roles over egui's bundled fonts, and
  the fixed dark palette.
- `theme`: pure `build_style` plus the one-call `apply_theme` installer; all
  five widget states themed explicitly.
- `widgets`: `section`, `form_row`, `primary_button`, `ghost_button`,
  `status_badge`, `toolbar`, and the toast model with a single scheduled
  repaint wake (floored at 50 ms).

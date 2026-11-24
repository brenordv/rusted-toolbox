//! The seal window's own sizes. Colors, spacing, type, and motion come from
//! `common-gui-rune`.

/// The window's size when it opens.
pub const WINDOW_SIZE: (f32, f32) = (980.0, 720.0);
/// The smallest window. The width keeps the form column's controls side by side. The height
/// is the Text tab at rest with nothing to scroll: the header row (60 = 2 x 16 inset + 28),
/// its hairline (1), the form's insets (32), Direction (16 label + 4 + 28 switch), Input
/// (12 + 16 + 4 + 4 lines of mono at 16 + 16 padding), the Recipients group (24 + 16 + 18
/// header + 8 + 3 rows of 32 + 8, then 12 + 16 + 4 + 28 for Other key), Output (24 + 28 + 4
/// + 80), and the action row (1 + 52): 676, under the design system's 680 ceiling.
pub const WINDOW_MIN: (f32, f32) = (760.0, 680.0);
/// The form column's widest width: 64 columns of armored text in `mono` with the box's
/// padding.
pub const FORM_MAX_WIDTH: f32 = 640.0;
/// The fewest lines the input and output boxes show.
pub const TEXT_MIN_LINES: f32 = 4.0;
/// The recipient rows the Text tab's list shows before it scrolls.
pub const TEXT_RECIPIENT_ROWS: f32 = 3.0;

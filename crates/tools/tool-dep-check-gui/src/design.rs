//! dep-check-gui's own sizes: the window, the form column, the Packages box, and the badge
//! column. Colors, type roles, and spacing come from `common-gui-rune`.

/// The window's size at start.
pub const WINDOW_SIZE: (f32, f32) = (760.0, 720.0);

/// The smallest window. Height, top to bottom: the insets 2 x 16; Ecosystem 16 + 4 + 28; a gap
/// of 24; Packages 16 + 4 + (6 mono lines + 2 x 8) + 4 + the hint 14; a gap of 24; the Results
/// label 16 + 4, the sources caption (two caption lines, about 30, plus 4: a report on screen is
/// the window's usual state, and a GitHub block often makes the caption two lines), and
/// `list_height(3)` 108; the action row 1 + 12 + 28 + 12. That is about 495, rounded up to 520.
/// Width 560: the status column then holds about 72 caption characters a line, so a
/// 140-character status fits the two lines a status may take.
pub const WINDOW_MIN: (f32, f32) = (560.0, 520.0);

/// The form column's widest: a result row with its badge column and mono lines.
pub const FORM_MAX_WIDTH: f32 = 640.0;

/// The Packages box's height in lines of `TYPE_MONO`.
pub const PACKAGES_LINES: f32 = 6.0;

/// The fewest rows the Results box shows, however small the window.
pub const RESULTS_MIN_ROWS: f32 = 3.0;

/// A result row's height before Rune measures it: the name and a supply-chain line, the usual
/// clean row. Rows with findings are taller and are measured once laid out.
pub const RESULT_ROW_ESTIMATE: f32 = 52.0;

/// The badge column's width: "Not checked" in `TYPE_CAPTION` with `SPACE_XS` on each side, and
/// room to spare.
pub const BADGE_WIDTH: f32 = 84.0;

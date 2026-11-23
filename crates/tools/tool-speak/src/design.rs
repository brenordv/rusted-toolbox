//! The speak window's own sizes. Colors, spacing, type, and motion come from
//! `common-gui-rune`.

/// The window's size when it opens.
pub const WINDOW_SIZE: (f32, f32) = (720.0, 560.0);
/// The smallest window: both areas keep every control reachable at this size.
pub const WINDOW_MIN: (f32, f32) = (560.0, 480.0);
/// The Settings form's widest width.
pub const FORM_MAX_WIDTH: f32 = 472.0;
/// The lines the Settings reactions box shows before it scrolls.
pub const REACTION_LINES: f32 = 6.0;
/// The rows of reaction buttons Compose shows before the block scrolls.
pub const REACTION_ROWS_MAX: f32 = 3.0;
/// A reaction button's widest width.
pub const REACTION_LABEL_MAX: f32 = 160.0;
/// The most characters of a tag a reaction button shows; a longer tag is cut with "…" and
/// carried whole as the button's help text. About what fits [`REACTION_LABEL_MAX`].
pub const REACTION_LABEL_MAX_CHARS: usize = 22;
/// The fewest lines the Compose text editor shows.
pub const TEXT_MIN_LINES: f32 = 4.0;

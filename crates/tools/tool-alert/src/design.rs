//! alert's own sizes. Colors, spacing, type, and motion come from `common-gui-rune`.

/// The alert window, in logical points. It never resizes.
pub const ALERT_SIZE: (f32, f32) = (440.0, 220.0);
/// The tester window as it opens.
pub const TESTER_SIZE: (f32, f32) = (480.0, 420.0);
/// The tester window's smallest size: every field and the Test button stay on screen.
pub const TESTER_MIN: (f32, f32) = (420.0, 380.0);
/// The lines the tester's Body editor shows before it scrolls.
pub const BODY_LINES: f32 = 4.0;

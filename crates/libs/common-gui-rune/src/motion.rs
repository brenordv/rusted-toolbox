//! Motion roles every Rune window shares. An animation names one of these and never a spring
//! of its own. The values are rune-motion's presets (`rune::Spring` re-exports the type but
//! not the preset constants), all critically damped: nothing bounces.

use rune::{Spring, SpringPair};

/// Press feedback, toggles (rune-motion `INSTANT`).
pub const FEEDBACK: Spring = Spring::new(0.10, 0.0);
/// Segmented indicator, field reveal, area and status swaps (rune-motion `SNAP`).
pub const CONTROL: Spring = Spring::new(0.25, 0.0);
/// Exit of a [`CONTROL`] motion (rune-motion `SNAP_EXIT`).
pub const CONTROL_EXIT: Spring = Spring::new(0.20, 0.0);
/// [`CONTROL`] with its exit, for presence transitions.
pub const CONTROL_PAIR: SpringPair = SpringPair::new(CONTROL, CONTROL_EXIT);

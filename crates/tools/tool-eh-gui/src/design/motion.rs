//! Motion roles. Every animation names one of these; none uses a spring of its own. The values
//! are rune-motion's presets (`rune::Spring` re-exports the type but not the preset constants),
//! all critically damped: nothing in this app bounces.

use rune::{Spring, SpringPair};

/// Press feedback, toggles (rune-motion `INSTANT`).
pub const FEEDBACK: Spring = Spring::new(0.10, 0.0);
/// Segmented indicator, field reveal, pill, panes, section swap (rune-motion `SNAP`).
pub const CONTROL: Spring = Spring::new(0.25, 0.0);
/// Exit of a [`CONTROL`] motion (rune-motion `SNAP_EXIT`).
pub const CONTROL_EXIT: Spring = Spring::new(0.20, 0.0);
/// Sheet height reflow (rune-motion `SETTLE`).
pub const LAYOUT: Spring = Spring::new(0.30, 0.0);
/// Sheets, empty-state entrance (rune-motion `REFLOW`).
pub const SURFACE: Spring = Spring::new(0.40, 0.0);
/// Exit of a [`SURFACE`] motion (rune-motion `REFLOW_EXIT`).
pub const SURFACE_EXIT: Spring = Spring::new(0.32, 0.0);
/// [`SURFACE`] with its exit, for presence transitions.
pub const SURFACE_PAIR: SpringPair = SpringPair::new(SURFACE, SURFACE_EXIT);

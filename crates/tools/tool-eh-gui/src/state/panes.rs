//! The content minimum: how wide the side panes show beside a content area that keeps
//! [`CONTENT_MIN`], and the stored pane a drag on its edge starts from.

use super::{AppState, ListSection};
use crate::design::tokens::{CONTENT_MIN, FRAME_INSET};
use rune_kit::PaneState;

/// Pane widths as shown.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaneFit {
    pub sidebar: f32,
    /// `None` for a section without an inspector.
    pub inspector: Option<f32>,
}

/// A side pane: the sidebar, or one list section's inspector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pane {
    Sidebar,
    Inspector(ListSection),
}

/// Shown widths for `sidebar` and `inspector` in a window `window` points wide, so the content
/// keeps [`CONTENT_MIN`]: the inspector gives way first, down to its minimum, then the sidebar,
/// down to its own. Whole points, never above a stored width or below a pane's minimum. A
/// closed pane's width is its stored one.
pub fn fit(window: f32, sidebar: &PaneState, inspector: Option<&PaneState>) -> PaneFit {
    let open_inspector = inspector.filter(|pane| pane.is_open());
    let sides_without_pane =
        f32::from(u8::from(!sidebar.is_open()) + u8::from(open_inspector.is_none()));
    let need = sidebar.shown_width()
        + inspector.map_or(0.0, PaneState::shown_width)
        + FRAME_INSET * sides_without_pane
        + CONTENT_MIN;
    let mut excess = (need - window).max(0.0);
    let inspector_width = inspector.map(|pane| match open_inspector {
        Some(open) => give_way(open, &mut excess),
        None => pane.width(),
    });
    let sidebar_width = if sidebar.is_open() {
        give_way(sidebar, &mut excess)
    } else {
        sidebar.width()
    };
    PaneFit {
        sidebar: sidebar_width,
        inspector: inspector_width,
    }
}

/// `pane`'s width after giving up as much of `excess` as its minimum allows, floored to a whole
/// point; what it gave comes off `excess`.
fn give_way(pane: &PaneState, excess: &mut f32) -> f32 {
    let given = excess.min(pane.width() - pane.min_width());
    *excess -= given;
    (pane.width() - given).floor().max(pane.min_width())
}

impl AppState {
    /// The shown widths of the sidebar and `list`'s inspector (none for a section without a
    /// list) in a window `window` points wide.
    pub fn pane_fit(&self, window: f32, list: Option<ListSection>) -> PaneFit {
        let inspector = list.map(|list| match list {
            ListSection::Read => &self.read.inspector.pane,
            ListSection::Export => &self.archive.inspector.pane,
        });
        fit(window, &self.sidebar, inspector)
    }

    /// `pane`, its stored width first lowered to the width it shows at `window_width`;
    /// unchanged before the first layout or while it is closed.
    pub fn held_pane(&mut self, pane: Pane) -> &mut PaneState {
        let list = match pane {
            Pane::Sidebar => self.section.list(),
            Pane::Inspector(list) => Some(list),
        };
        let fitted = self.window_width.map(|window| self.pane_fit(window, list));
        let (held, shown) = match pane {
            Pane::Sidebar => (&mut self.sidebar, fitted.map(|fit| fit.sidebar)),
            Pane::Inspector(ListSection::Read) => (
                &mut self.read.inspector.pane,
                fitted.and_then(|fit| fit.inspector),
            ),
            Pane::Inspector(ListSection::Export) => (
                &mut self.archive.inspector.pane,
                fitted.and_then(|fit| fit.inspector),
            ),
        };
        if let Some(shown) = shown {
            held.set_width(shown);
        }
        held
    }
}

#[cfg(test)]
mod tests;

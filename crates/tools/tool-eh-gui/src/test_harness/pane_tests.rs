//! The content minimum in the real view: pane widths at the smallest window, back when it
//! grows, and drags that start from the width a pane shows.

use super::sweep_tests::{export_selected, read_selected};
use super::{Harness, rect};
use crate::design::tokens::{
    CONTENT_MIN, INSPECTOR_MAX, INSPECTOR_MIN, SIDEBAR_MAX, SIDEBAR_MIN, SIDEBAR_WIDTH, WINDOW_MIN,
    WINDOW_SIZE,
};
use crate::state::ListSection;
use rune::accesskit::Action;
use rune_kit::PaneState;

/// `list`'s section with a row selected, so its inspector is open.
fn selected(list: ListSection, size: (f32, f32)) -> Harness {
    match list {
        ListSection::Read => read_selected(size),
        ListSection::Export => export_selected(size),
    }
}

fn inspector_pane(h: &mut Harness, list: ListSection) -> &mut PaneState {
    match list {
        ListSection::Read => &mut h.rt.state.read.inspector.pane,
        ListSection::Export => &mut h.rt.state.archive.inspector.pane,
    }
}

/// Stores `sidebar` and `inspector` as the widths the user set, then lets the view show them.
fn store(h: &mut Harness, list: ListSection, sidebar: f32, inspector: f32) {
    h.rt.state.sidebar.set_width(sidebar);
    inspector_pane(h, list).set_width(inspector);
    h.rt.invalidate();
    h.settle();
}

fn stored(h: &mut Harness, list: ListSection) -> (f32, f32) {
    (h.rt.state.sidebar.width(), inspector_pane(h, list).width())
}

/// The sidebar, content, and inspector widths as shown in a window `window` points wide, to the
/// whole point the fit gives (a pane spring's last hundredths of a point read as at rest).
fn shown(h: &Harness, window: f32) -> [f32; 3] {
    let [x, _, w, _] = rect(&h.rt, "content");
    [x.round(), w.round(), (window - (x + w)).round()]
}

#[rstest::rstest]
#[case::read(ListSection::Read)]
#[case::export(ListSection::Export)]
fn panes_narrow_at_the_smallest_window_and_come_back_when_it_grows(#[case] list: ListSection) {
    let mut h = selected(list, WINDOW_MIN);
    store(&mut h, list, SIDEBAR_MAX, INSPECTOR_MAX);

    let mut windows = vec![h.state().window_width];
    let smallest = shown(&h, WINDOW_MIN.0);
    h.resize(WINDOW_SIZE);
    windows.push(h.state().window_width);
    let grown = shown(&h, WINDOW_SIZE.0);
    h.resize(WINDOW_MIN);
    windows.push(h.state().window_width);
    let again = shown(&h, WINDOW_MIN.0);

    assert_eq!(
        windows,
        [Some(WINDOW_MIN.0), Some(WINDOW_SIZE.0), Some(WINDOW_MIN.0)]
    );
    assert_eq!(smallest, [SIDEBAR_WIDTH, CONTENT_MIN, INSPECTOR_MIN]);
    assert_eq!(
        grown,
        [
            SIDEBAR_MAX,
            WINDOW_SIZE.0 - SIDEBAR_MAX - INSPECTOR_MAX,
            INSPECTOR_MAX
        ]
    );
    assert_eq!(again, smallest);
    assert_eq!(stored(&mut h, list), (SIDEBAR_MAX, INSPECTOR_MAX));
}

#[test]
fn a_drag_on_the_inspector_starts_from_the_width_it_shows() {
    let mut h = selected(ListSection::Read, WINDOW_MIN);
    store(&mut h, ListSection::Read, SIDEBAR_MIN, INSPECTOR_MAX);
    let before = shown(&h, WINDOW_MIN.0);

    h.drag("inspector-grip", 20.0);

    assert_eq!(before[2], 364.0);
    assert_eq!(shown(&h, WINDOW_MIN.0)[2], 344.0);
    assert_eq!(stored(&mut h, ListSection::Read), (SIDEBAR_MIN, 344.0));
}

#[test]
fn a_drag_after_the_window_grew_starts_from_the_grown_width() {
    let mut h = selected(ListSection::Read, WINDOW_MIN);
    store(&mut h, ListSection::Read, SIDEBAR_MAX, INSPECTOR_MAX);
    h.resize(WINDOW_SIZE);

    h.drag("inspector-grip", 20.0);

    assert_eq!(shown(&h, WINDOW_SIZE.0)[2], INSPECTOR_MAX - 20.0);
    assert_eq!(
        stored(&mut h, ListSection::Read),
        (SIDEBAR_MAX, INSPECTOR_MAX - 20.0)
    );
}

#[rstest::rstest]
#[case::inspector_wider("inspector-grip", -60.0, (SIDEBAR_MAX, INSPECTOR_MIN))]
#[case::sidebar_wider("sidebar-grip", 60.0, (SIDEBAR_WIDTH, INSPECTOR_MAX))]
fn a_pane_edge_dragged_wider_stops_at_the_content_minimum(
    #[case] grip: &str,
    #[case] dx: f32,
    #[case] stored_after: (f32, f32),
) {
    let mut h = selected(ListSection::Read, WINDOW_MIN);
    store(&mut h, ListSection::Read, SIDEBAR_MAX, INSPECTOR_MAX);

    h.drag(grip, dx);

    assert_eq!(
        shown(&h, WINDOW_MIN.0),
        [SIDEBAR_WIDTH, CONTENT_MIN, INSPECTOR_MIN]
    );
    assert_eq!(stored(&mut h, ListSection::Read), stored_after);
}

#[test]
fn an_accessibility_step_starts_from_the_width_the_sidebar_shows() {
    let mut h = selected(ListSection::Read, WINDOW_MIN);
    store(&mut h, ListSection::Read, SIDEBAR_MAX, INSPECTOR_MAX);

    h.rt.accessibility_action("sidebar-grip", Action::Decrement, None);
    h.settle();

    assert_eq!(
        shown(&h, WINDOW_MIN.0),
        [SIDEBAR_WIDTH - 8.0, CONTENT_MIN, INSPECTOR_MIN + 8.0]
    );
    assert_eq!(
        stored(&mut h, ListSection::Read),
        (SIDEBAR_WIDTH - 8.0, INSPECTOR_MAX)
    );
}

#[test]
fn closing_the_sidebar_at_the_smallest_window_stores_the_width_it_showed() {
    let mut h = selected(ListSection::Read, WINDOW_MIN);
    store(&mut h, ListSection::Read, SIDEBAR_MAX, INSPECTOR_MAX);

    h.press("sidebar-toggle");
    let closed = (h.state().sidebar.is_open(), h.state().sidebar.width());
    h.press("sidebar-toggle");

    assert_eq!(closed, (false, SIDEBAR_WIDTH));
    assert!(h.state().sidebar.is_open());
    assert_eq!(
        shown(&h, WINDOW_MIN.0),
        [SIDEBAR_WIDTH, CONTENT_MIN, INSPECTOR_MIN]
    );
}

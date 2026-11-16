use super::*;
use crate::design::tokens::{
    INSPECTOR_MAX, INSPECTOR_MIN, INSPECTOR_WIDTH, SIDEBAR_MAX, SIDEBAR_MIN, SIDEBAR_WIDTH,
};
use crate::form::ConnectForm;
use crate::state::Section;
use common_cli::app_logger::FileLogging;

fn sidebar(width: f32) -> PaneState {
    PaneState::new(SIDEBAR_WIDTH)
        .range(SIDEBAR_MIN, SIDEBAR_MAX)
        .with_width(width)
}

fn inspector(width: f32) -> PaneState {
    PaneState::new(INSPECTOR_WIDTH)
        .range(INSPECTOR_MIN, INSPECTOR_MAX)
        .with_width(width)
}

fn closed(mut pane: PaneState) -> PaneState {
    pane.set_open(false);
    pane
}

fn shown(sidebar: f32, inspector: f32) -> PaneFit {
    PaneFit {
        sidebar,
        inspector: Some(inspector),
    }
}

#[rstest::rstest]
#[case::room_for_both(1280.0, 264.0, 380.0, shown(264.0, 380.0))]
#[case::both_widest_at_the_smallest_window(960.0, 340.0, 560.0, shown(264.0, 320.0))]
#[case::the_inspector_gives_way_first(1100.0, 340.0, 560.0, shown(340.0, 384.0))]
#[case::under_the_minimums(800.0, 340.0, 560.0, shown(220.0, 320.0))]
fn both_panes_open_keep_the_content_minimum_where_the_window_allows(
    #[case] window: f32,
    #[case] sidebar_width: f32,
    #[case] inspector_width: f32,
    #[case] expected: PaneFit,
) {
    let fitted = fit(
        window,
        &sidebar(sidebar_width),
        Some(&inspector(inspector_width)),
    );

    assert_eq!(fitted, expected);
}

#[test]
fn a_fractional_window_gives_whole_points_and_keeps_the_content_minimum() {
    let window = 960.6;

    let fitted = fit(window, &sidebar(340.0), Some(&inspector(560.0)));

    let inspector_width = fitted.inspector.unwrap();
    assert_eq!(fitted.sidebar.fract(), 0.0);
    assert_eq!(inspector_width.fract(), 0.0);
    assert!(window - fitted.sidebar - inspector_width >= CONTENT_MIN);
}

#[test]
fn a_closed_sidebar_leaves_the_frame_inset_and_the_inspector_gives_the_rest() {
    let fitted = fit(940.0, &closed(sidebar(340.0)), Some(&inspector(560.0)));

    assert_eq!(fitted, shown(340.0, 556.0));
}

#[test]
fn a_closed_inspector_is_returned_as_stored_and_takes_nothing() {
    let fitted = fit(960.0, &sidebar(340.0), Some(&closed(inspector(560.0))));

    assert_eq!(fitted, shown(340.0, 560.0));
}

#[test]
fn a_section_without_an_inspector_keeps_the_sidebar() {
    let fitted = fit(960.0, &sidebar(340.0), None);

    assert_eq!(
        fitted,
        PaneFit {
            sidebar: 340.0,
            inspector: None,
        }
    );
}

/// The Read section at a 960 pt window with the sidebar and both inspectors open at their
/// widest.
fn widest_at_the_smallest_window() -> AppState {
    let mut s = AppState::new(None, ConnectForm::default(), None, FileLogging::Off);
    s.window_width = Some(960.0);
    s.sidebar.set_width(SIDEBAR_MAX);
    for pane in [&mut s.read.inspector.pane, &mut s.archive.inspector.pane] {
        pane.set_width(INSPECTOR_MAX);
        pane.set_open(true);
    }
    s
}

#[test]
fn holding_an_inspector_lowers_it_to_its_shown_width_and_leaves_the_sidebar() {
    let mut s = widest_at_the_smallest_window();

    let held = s.held_pane(Pane::Inspector(ListSection::Read)).width();

    assert_eq!(held, INSPECTOR_MIN);
    assert_eq!(s.sidebar.width(), SIDEBAR_MAX);
}

#[test]
fn holding_the_sidebar_lowers_it_to_its_shown_width_and_leaves_the_inspector() {
    let mut s = widest_at_the_smallest_window();

    let held = s.held_pane(Pane::Sidebar).width();

    assert_eq!(held, SIDEBAR_WIDTH);
    assert_eq!(s.read.inspector.pane.width(), INSPECTOR_MAX);
}

#[test]
fn holding_the_export_inspector_fits_it_against_its_own_section() {
    let mut s = widest_at_the_smallest_window();
    s.archive.inspector.pane.set_width(400.0);
    s.read.inspector.pane.set_open(false);

    let held = s.held_pane(Pane::Inspector(ListSection::Export)).width();

    assert_eq!(held, INSPECTOR_MIN);
}

#[test]
fn holding_a_pane_before_the_first_layout_leaves_it_as_stored() {
    let mut s = widest_at_the_smallest_window();
    s.window_width = None;

    let held = s.held_pane(Pane::Sidebar).width();

    assert_eq!(held, SIDEBAR_MAX);
}

#[test]
fn holding_a_closed_inspector_leaves_it_as_stored() {
    let mut s = widest_at_the_smallest_window();
    s.read.inspector.pane.set_open(false);
    s.archive.inspector.pane.set_open(false);
    s.window_width = Some(700.0);

    let held = s.held_pane(Pane::Inspector(ListSection::Read)).width();

    assert_eq!(held, INSPECTOR_MAX);
}

#[test]
fn holding_the_sidebar_in_the_send_section_keeps_it_as_stored() {
    let mut s = widest_at_the_smallest_window();
    s.section = Section::Send;

    let held = s.held_pane(Pane::Sidebar).width();

    assert_eq!(held, SIDEBAR_MAX);
}

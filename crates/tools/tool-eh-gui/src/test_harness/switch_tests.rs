use super::sweep_tests::{export_selected, orders_form, read_selected, send_idle};
use super::worst_cases::long_text;
use super::{FRAME, Harness, a11y_node, element, focused_key, lists, rect, text_visible};
use crate::bridge::UiDelivery;
use crate::design::tokens::{WINDOW_MIN, WINDOW_SIZE};
use crate::state::{AppState, Input};
use crate::views::{
    BULK_METADATA_KEY, CONDENSE_KEY, EXPORT_METADATA_KEY, LOCAL_TIME_KEY, PARTITIONS_SCROLL_KEY,
    PILL_SCROLL_KEY, READ_METADATA_KEY, SKIP_EXPORTED_KEY, SWITCH_KEY,
};
use eh_core::engine::EngineEvent;
use eh_core::stats::{PartitionStats, StatsSnapshot};
use rstest::rstest;
use rune::Key;
use rune::accesskit::Action;

fn export_options(size: (f32, f32)) -> Harness {
    let mut h = export_selected(size);
    h.press("bulk-options");
    h
}

/// The centre of the gap between a switch and its label, whichever side the label is on.
fn gap_centre(h: &Harness, key: &str) -> (f32, f32) {
    let [sx, sy, sw, sh] = rect(&h.rt, key);
    let [lx, _, lw, _] = rect(&h.rt, &format!("{key}-label"));
    let x = if lx < sx {
        (lx + lw + sx) / 2.0
    } else {
        (sx + sw + lx) / 2.0
    };
    (x, sy + sh / 2.0)
}

/// The focused switch keyed `key`.
///
/// # Panics
/// When the switch takes no focus.
fn focus(h: &mut Harness, key: &str) {
    assert!(
        h.rt.accessibility_action(key, Action::Focus, None),
        "{key} takes no focus"
    );
    h.settle();
}

#[rstest]
#[case::read_metadata(read_selected, READ_METADATA_KEY, |s: &AppState| s.read.inspector.metadata)]
#[case::export_metadata(export_selected, EXPORT_METADATA_KEY, |s: &AppState| s.archive.inspector.metadata)]
#[case::include_metadata(export_options, BULK_METADATA_KEY, |s: &AppState| s.archive.options.metadata)]
#[case::condense(export_options, CONDENSE_KEY, |s: &AppState| s.archive.options.condense)]
#[case::local_time(export_options, LOCAL_TIME_KEY, |s: &AppState| s.archive.options.local_time)]
#[case::skip_exported(export_options, SKIP_EXPORTED_KEY, |s: &AppState| s.archive.options.skip_exported)]
#[case::template(send_idle, SWITCH_KEY, |s: &AppState| s.send.composer.template().is_on())]
fn the_label_the_gap_the_switch_and_space_each_toggle_a_labeled_switch_once(
    #[case] build: fn((f32, f32)) -> Harness,
    #[case] key: &str,
    #[case] on: fn(&AppState) -> bool,
) {
    let mut h = build(WINDOW_SIZE);
    let before = on(h.state());
    let (gap_x, gap_y) = gap_centre(&h, key);

    h.press(&format!("{key}-label"));
    let after_label = on(h.state());
    h.press_at(gap_x, gap_y);
    let after_gap = on(h.state());
    h.press(key);
    let after_switch = on(h.state());
    focus(&mut h, key);
    h.key(Key::Text(" ".into()));
    let after_space = on(h.state());

    assert_eq!(
        [after_label, after_gap, after_switch, after_space],
        [!before, before, !before, before]
    );
}

#[rstest]
#[case::read_metadata(read_selected, READ_METADATA_KEY, "Metadata")]
#[case::export_metadata(export_selected, EXPORT_METADATA_KEY, "Metadata")]
#[case::include_metadata(export_options, BULK_METADATA_KEY, "Include metadata")]
#[case::condense(export_options, CONDENSE_KEY, "One file per month")]
#[case::local_time(export_options, LOCAL_TIME_KEY, "Local time")]
#[case::skip_exported(export_options, SKIP_EXPORTED_KEY, "Skip already exported")]
#[case::template(send_idle, SWITCH_KEY, "Template")]
fn a_labeled_switch_is_one_switch_node_named_by_its_label(
    #[case] build: fn((f32, f32)) -> Harness,
    #[case] key: &str,
    #[case] label: &str,
) {
    let h = build(WINDOW_SIZE);

    let tree = h.a11y();

    let Some(node) = a11y_node(&tree, key) else {
        panic!("{key} is not in the tree");
    };
    assert_eq!(node["role"], "switch", "{node}");
    assert_eq!(node["name"], label, "{node}");
    assert!(a11y_node(&tree, &format!("{key}-row")).is_none());
    assert!(a11y_node(&tree, &format!("{key}-label")).is_none());
}

#[test]
fn a_labeled_switch_reads_checked_after_a_toggle() {
    let mut h = send_idle(WINDOW_SIZE);
    let before = lists(
        a11y_node(&h.a11y(), SWITCH_KEY).unwrap(),
        "states",
        "checked",
    );

    h.press(&format!("{SWITCH_KEY}-label"));

    let tree = h.a11y();
    assert!(!before);
    assert!(lists(
        a11y_node(&tree, SWITCH_KEY).unwrap(),
        "states",
        "checked"
    ));
}

#[rstest]
#[case::read(read_selected, READ_METADATA_KEY)]
#[case::export(export_selected, EXPORT_METADATA_KEY)]
fn the_metadata_switch_and_its_label_fit_the_narrowest_inspector(
    #[case] build: fn((f32, f32)) -> Harness,
    #[case] key: &str,
) {
    let h = build(WINDOW_MIN);
    let [ix, iy, iw, ih] = rect(&h.rt, "inspector");

    let parts = [key.to_owned(), format!("{key}-label")].map(|part| rect(&h.rt, &part));

    for [x, y, w, height] in parts {
        assert!(
            x >= ix - 0.5 && x + w <= ix + iw + 0.5 && y >= iy - 0.5 && y + height <= iy + ih + 0.5,
            "{key} at {:?} outside the inspector at {:?}",
            [x, y, w, height],
            [ix, iy, iw, ih]
        );
    }
}

/// Elements inside the element keyed `within` that lie outside it sideways.
fn sideways_outside(h: &Harness, within: &str) -> Vec<String> {
    let outer = element(&h.rt, within);
    let [x, _, w, _] = outer.rect;
    h.rt.elements()
        .into_iter()
        .filter(|inner| !inner.exiting && inner.path.starts_with(&format!("{}/", outer.path)))
        .filter(|inner| inner.rect[0] < x - 0.5 || inner.rect[0] + inner.rect[2] > x + w + 0.5)
        .map(|inner| format!("{} at {:?}", inner.path, inner.rect))
        .collect()
}

fn inside_window(rect: [f32; 4], size: (f32, f32)) -> bool {
    let [x, y, w, h] = rect;
    x >= -0.5 && y >= -0.5 && x + w <= size.0 + 0.5 && y + h <= size.1 + 0.5
}

/// Whether the centre of the element keyed `key` lies inside the element keyed `within`.
fn centred_inside(h: &Harness, key: &str, within: &str) -> bool {
    let [x, y, w, height] = rect(&h.rt, key);
    let [ox, oy, ow, oh] = rect(&h.rt, within);
    let (cx, cy) = (x + w / 2.0, y + height / 2.0);
    cx >= ox && cx <= ox + ow && cy >= oy && cy <= oy + oh
}

/// A Read session with a 200-character data folder and eight long partition errors, the pill
/// popover open; returns the View details key of the oldest error, the popover's last line.
fn long_pill(size: (f32, f32)) -> (Harness, String) {
    let mut s = orders_form();
    s.form.data_folder = format!("{}data", "nested-folder/".repeat(14));
    assert!(s.form.data_folder.chars().count() >= 200);
    let mut h = Harness::new(s, size);
    let (session, _) = h.start_session(8);
    let errors = (0..8)
        .map(|partition| EngineEvent::Error {
            partition_id: Some(partition.to_string()),
            error: format!("{partition}: {}", long_text()),
            fatal: false,
        })
        .collect();
    let at = h.now();
    h.deliver(Input::Delivery(UiDelivery {
        at,
        session,
        events: errors,
        stats: None,
    }));
    let oldest = h.state().read.session_log[0].id;
    h.press("pill");
    (h, format!("log-details-{oldest}"))
}

#[test]
fn the_pill_popover_keeps_long_rows_inside_and_scrolls_to_its_last_line() {
    let (mut h, last) = long_pill(WINDOW_MIN);
    let content = element(&h.rt, "pill-popover-content");
    let outside = sideways_outside(&h, "pill-popover-content");
    let below_fold = !centred_inside(&h, &last, PILL_SCROLL_KEY);

    assert!(h.rt.accessibility_action(&last, Action::ScrollIntoView, None));
    h.settle();
    let scrolled_in = centred_inside(&h, &last, PILL_SCROLL_KEY);
    h.press(&last);

    assert!(outside.is_empty(), "{outside:#?}");
    assert!(
        inside_window(content.rect, WINDOW_MIN),
        "{:?}",
        content.rect
    );
    assert!(below_fold, "the last line already showed");
    assert!(scrolled_in);
    assert!(h.state().details().is_some());
}

#[test]
fn tab_brings_the_pill_popovers_last_details_button_into_view() {
    const MOST_TABS: usize = 80;
    let (mut h, last) = long_pill(WINDOW_MIN);
    let below_fold = !centred_inside(&h, &last, PILL_SCROLL_KEY);

    let mut tabs = 0;
    while focused_key(&h.rt).as_deref() != Some(last.as_str()) && tabs < MOST_TABS {
        h.rt.key(rune::KeyEvent {
            key: Key::Tab,
            mods: rune::Mods::default(),
        });
        h.rt.frame(FRAME);
        tabs += 1;
    }
    h.settle();

    assert!(below_fold, "the last line already showed");
    assert_eq!(focused_key(&h.rt).as_deref(), Some(last.as_str()));
    assert!(centred_inside(&h, &last, PILL_SCROLL_KEY));
    assert!(centred_inside(&h, &last, "pill-popover-content"));
    assert!(inside_window(rect(&h.rt, &last), WINDOW_MIN));
}

/// A session over 32 partitions, each past ten million messages, with the partitions popover
/// open.
fn many_partitions(size: (f32, f32)) -> Harness {
    let mut h = Harness::new(orders_form(), size);
    let (session, _) = h.start_session(32);
    let at = h.now();
    let stats = StatsSnapshot {
        received: 32 * 12_345_678,
        per_partition: (0..32)
            .map(|id| {
                (
                    id.to_string(),
                    PartitionStats {
                        received: 12_345_678,
                        last_received_at: Some(at),
                        last_enqueued_at: Some(at),
                    },
                )
            })
            .collect(),
        ..StatsSnapshot::default()
    };
    h.capture(session, Vec::new(), Some(stats));
    h.tick();
    h.press("stat-received");
    h
}

#[test]
fn the_partitions_popover_shows_short_lines_and_scrolls_to_its_last_partition() {
    let mut h = many_partitions(WINDOW_MIN);
    let content = element(&h.rt, "partitions-popover-content");
    let outside = sideways_outside(&h, "partitions-popover-content");
    let last_hidden = !text_visible(&h.rt, "Partition 31");

    let [x, y, w, height] = rect(&h.rt, PARTITIONS_SCROLL_KEY);
    for _ in 0..10 {
        h.rt.wheel(x + w / 2.0, y + height / 2.0, -2_000.0);
        h.rt.frame(FRAME);
    }
    h.settle();

    assert!(outside.is_empty(), "{outside:#?}");
    assert!(
        inside_window(content.rect, WINDOW_MIN),
        "{:?}",
        content.rect
    );
    assert!(h.has_text("12,345,678 · last "));
    assert!(last_hidden, "partition 31 already showed");
    assert!(text_visible(&h.rt, "Partition 31"));
}

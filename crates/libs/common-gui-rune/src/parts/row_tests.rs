use super::*;
use crate::Ds;
use crate::test_harness::{a11y_nodes, has_key, rect, runtime_at, settle};
use rune::{Cx, Runtime};
use rune_kit::{Button, ButtonVariant};

const ROW_KEY: &str = "actions";
const STATUS_KEY: &str = "actions-status";
const SWITCH_KEY: &str = "choice";
const LABELS: [&str; 3] = ["One", "Two", "Three"];
const NARROW: (f32, f32) = (420.0, 200.0);

struct Form {
    ds: Ds,
    status: Option<String>,
    growing_status: bool,
    enabled: bool,
    selected: usize,
    picks: Vec<usize>,
}

fn form(status: Option<&str>) -> Form {
    Form {
        ds: Ds::new(),
        status: status.map(str::to_owned),
        growing_status: false,
        enabled: true,
        selected: 0,
        picks: Vec::new(),
    }
}

fn view(form: &Form, _: &mut Cx<Form>) -> El {
    let k = form.ds.kit(true);
    let status = form.status.as_deref().map(|words| {
        if form.growing_status {
            row()
                .key(STATUS_KEY)
                .grow(1.0)
                .min_w(0.0)
                .center_y()
                .child(text(words).font(TYPE_CAPTION).color(k.c.text_muted))
        } else {
            status_line(k, STATUS_KEY, Tone::Muted, StatusLines::Wrap, words)
        }
    });
    let buttons = ["Play", "Generate"]
        .into_iter()
        .map(|label| {
            Button::new(label)
                .variant(ButtonVariant::Neutral)
                .build(k)
                .key(label)
        })
        .collect();
    col()
        .w_full()
        .h_full()
        .child(choice_switch(
            k,
            SWITCH_KEY,
            "Direction",
            &LABELS,
            form.selected,
            form.enabled,
            |form: &mut Form, index: usize, _: &mut Tx| {
                form.picks.push(index);
                form.selected = index;
            },
        ))
        .child(spacer())
        .child(action_row(k, ROW_KEY, status, buttons))
}

fn runtime(form: Form) -> Runtime<Form> {
    runtime_at(form, view, NARROW)
}

fn segment_centre(rt: &Runtime<Form>, label: &str) -> (f32, f32) {
    let segment = rt
        .elements()
        .into_iter()
        .find(|element| element.label.as_deref() == Some(label) && !element.exiting)
        .unwrap_or_else(|| panic!("no segment labeled {label}"));
    let [x, y, w, h] = segment.rect;
    (x + w / 2.0, y + h / 2.0)
}

fn press_segment(rt: &mut Runtime<Form>, label: &str) {
    let (x, y) = segment_centre(rt, label);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

#[test]
fn a_long_status_takes_the_width_the_buttons_leave_and_both_buttons_stay_in_the_row() {
    let rt = runtime(form(Some(
        &"The server took too long to answer. ".repeat(6),
    )));

    let [row_x, _, row_w, _] = rect(&rt, ROW_KEY);
    let [play_x, _, _, _] = rect(&rt, "Play");
    let [generate_x, _, generate_w, _] = rect(&rt, "Generate");
    let [status_x, _, status_w, _] = rect(&rt, STATUS_KEY);

    assert!(play_x >= row_x, "Play starts at {play_x}");
    assert!(
        generate_x + generate_w <= row_x + row_w - SPACE_LG + 0.5,
        "Generate ends at {}",
        generate_x + generate_w
    );
    assert!(
        status_x + status_w <= play_x - SPACE_MD + 0.5,
        "the status ends at {}, Play starts at {play_x}",
        status_x + status_w
    );
    assert!(status_w > 0.0);
}

#[rstest::rstest]
#[case::a_status_line(false)]
#[case::a_status_that_grows(true)]
fn the_status_is_centred_on_the_buttons(#[case] growing_status: bool) {
    let rt = runtime(Form {
        growing_status,
        ..form(Some("Ready to send"))
    });

    let [_, status_y, _, status_h] = rect(&rt, STATUS_KEY);
    let [_, button_y, _, button_h] = rect(&rt, "Play");

    assert!(status_h > 0.0);
    assert!(
        ((status_y + status_h / 2.0) - (button_y + button_h / 2.0)).abs() < 1.0,
        "status at {status_y}+{status_h}, Play at {button_y}+{button_h}"
    );
}

#[test]
fn with_no_status_nothing_is_mounted_and_the_row_keeps_its_height() {
    let without = runtime(form(None));
    let with = runtime(form(Some("Ready")));

    assert!(!has_key(&without, STATUS_KEY));
    assert!(has_key(&with, STATUS_KEY));
    assert_eq!(rect(&without, ROW_KEY)[3], rect(&with, ROW_KEY)[3]);
    assert!(rect(&without, ROW_KEY)[3] >= CONTROL_HEIGHT + 2.0 * SPACE_MD);
}

#[test]
fn an_enabled_switch_picks_the_pressed_segment() {
    let mut rt = runtime(form(None));

    press_segment(&mut rt, "Two");

    assert_eq!(rt.state.picks, vec![1]);
}

#[test]
fn a_disabled_switch_calls_nothing_and_reads_as_disabled() {
    let mut disabled = form(None);
    disabled.enabled = false;
    let mut rt = runtime(disabled);

    press_segment(&mut rt, "Two");

    assert!(rt.state.picks.is_empty());
    let tree = rt.accessibility_json();
    let segments: Vec<_> = a11y_nodes(&tree)
        .into_iter()
        .filter(|node| LABELS.iter().any(|label| node["name"] == *label))
        .collect();
    assert_eq!(segments.len(), LABELS.len(), "{segments:?}");
    assert!(
        segments.iter().all(|node| node["states"]
            .as_array()
            .is_some_and(|states| states.iter().any(|state| state == "disabled"))),
        "{segments:?}"
    );
}

#[test]
fn the_switch_is_named_by_its_label() {
    let rt = runtime(form(None));

    let tree = rt.accessibility_json();
    let named = a11y_nodes(&tree)
        .into_iter()
        .any(|node| node["name"] == "Direction");

    assert!(named, "{tree}");
}

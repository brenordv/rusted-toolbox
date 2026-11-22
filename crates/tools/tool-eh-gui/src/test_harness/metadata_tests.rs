use super::builders::send_state;
use super::services::ServiceCall;
use super::{FRAME, Harness, a11y_nodes, rect, row_visible, text_visible};
use crate::design::tokens::{WINDOW_MIN, WINDOW_SIZE};
use crate::send::compose::RoutingChoice;
use crate::views::{
    BODY_KEY, CORRELATION_ID_KEY, COUNT_KEY, PROPERTIES_KEY, SEND_BUTTON_KEY, SWITCH_KEY,
};
use rune::Key;
use serde_json::Value;

const PROPERTIES_HINT: &str = "One key=value per line";

fn send_harness(size: (f32, f32)) -> Harness {
    let (s, _) = send_state();
    Harness::new(s, size)
}

/// The Send section with `properties` already in the Properties field.
fn with_properties(properties: &str, size: (f32, f32)) -> Harness {
    let (mut s, _) = send_state();
    s.send.composer.properties.replace_text(properties);
    Harness::new(s, size)
}

fn start_sends(h: &mut Harness) -> Vec<crate::send::SendRequest> {
    h.take_calls()
        .into_iter()
        .filter_map(|call| match call {
            ServiceCall::StartSend(request) => Some(request),
            _ => None,
        })
        .collect()
}

fn composer(h: &Harness) -> &crate::send::compose::Composer {
    &h.state().send.composer
}

/// The top of the Count field, which moves when the sidebar scrolls.
fn count_top(h: &Harness) -> f32 {
    rect(&h.rt, COUNT_KEY)[1]
}

/// Scrolls the sidebar to its end with the pointer over the Destination block.
fn scroll_sidebar_down(h: &mut Harness) {
    let [x, y, w, _] = rect(&h.rt, "send-destination");
    for _ in 0..10 {
        h.rt.wheel(x + w / 2.0, y + 4.0, -400.0);
        h.rt.frame(FRAME);
    }
    h.settle();
}

/// A wheel of `dy` with the pointer at the centre of the element keyed `key`.
fn wheel_over(h: &mut Harness, key: &str, dy: f32) {
    let [x, y, w, height] = rect(&h.rt, key);
    h.rt.wheel(x + w / 2.0, y + height / 2.0, dy);
    h.settle();
}

#[test]
fn return_in_the_body_types_a_newline_and_the_template_follows_the_new_text() {
    let mut h = send_harness(WINDOW_SIZE);
    h.press(SWITCH_KEY);
    h.type_into(BODY_KEY, "{{random.integer}}");
    let compiled = composer(&h).template().issue().is_none();
    h.key(Key::End);
    for _ in 0.."eger}}".len() {
        h.key(Key::Left);
    }

    h.key(Key::Enter);

    assert!(compiled);
    assert_eq!(composer(&h).body.text, "{{random.int\neger}}");
    assert!(composer(&h).template().issue().is_some());
    assert!(start_sends(&mut h).is_empty());
}

#[test]
fn return_in_properties_types_a_newline_and_sends_nothing() {
    let mut h = send_harness(WINDOW_SIZE);
    h.type_into(PROPERTIES_KEY, "source=load-test");

    h.key(Key::Enter);
    h.rt.paste("run=7");
    h.settle();

    assert_eq!(composer(&h).properties.text, "source=load-test\nrun=7");
    assert!(start_sends(&mut h).is_empty());
}

#[test]
fn typing_ten_lines_leaves_the_properties_box_its_height() {
    let mut h = send_harness(WINDOW_SIZE);
    let before = rect(&h.rt, PROPERTIES_KEY);

    let lines: Vec<String> = (0..10).map(|n| format!("key{n}=value{n}")).collect();
    h.type_into(PROPERTIES_KEY, &lines.join("\n"));

    assert_eq!(rect(&h.rt, PROPERTIES_KEY), before);
}

#[test]
fn send_shortcut_from_properties_sends_the_properties_and_correlation_id() {
    let mut h = send_harness(WINDOW_SIZE);
    h.type_into(CORRELATION_ID_KEY, " trace-1 ");
    h.type_into(PROPERTIES_KEY, "source=load-test\n\nrun = 7");

    h.command(Key::Enter);

    let requests = start_sends(&mut h);
    let [request] = requests.as_slice() else {
        panic!("expected one StartSend, got {requests:?}");
    };
    assert_eq!(request.event.correlation_id.as_deref(), Some("trace-1"));
    assert_eq!(
        request.event.properties.iter().collect::<Vec<_>>(),
        [
            (&"run".to_owned(), &"7".to_owned()),
            (&"source".to_owned(), &"load-test".to_owned())
        ]
    );
}

#[test]
fn a_properties_issue_blocks_the_send_and_lands_focus_on_screen_at_the_smallest_size() {
    let mut h = with_properties("source=load-test\nno equals", WINDOW_MIN);

    h.press(SEND_BUTTON_KEY);

    assert!(start_sends(&mut h).is_empty());
    assert!(composer(&h).properties.focused);
    assert!(row_visible(&h.rt, PROPERTIES_KEY));
    assert!(text_visible(
        &h.rt,
        "Line 2: no '=' (write it as key=value)"
    ));
    assert!(super::has_text(&h.rt, "Fix 1 field to send"));
}

#[test]
fn count_and_pacing_show_without_scrolling_at_the_smallest_size() {
    let (mut s, _) = send_state();
    s.send.composer.routing = RoutingChoice::PartitionKey;
    s.send.show_issues = true;
    s.send.composer.count.replace_text("0");
    let h = Harness::new(s, WINDOW_MIN);

    let pacing =
        h.rt.elements()
            .into_iter()
            .find(|element| element.text.as_deref() == Some("Batched"))
            .unwrap_or_else(|| panic!("no pacing picker"));

    assert!(row_visible(&h.rt, COUNT_KEY));
    assert!(text_visible(&h.rt, "Enter a number from 1 to 1,000,000"));
    assert!(
        pacing.rect[1] + pacing.rect[3] <= WINDOW_MIN.1,
        "pacing ends at {:?}",
        pacing.rect
    );
}

#[test]
fn a_wheel_over_the_empty_properties_box_scrolls_the_sidebar() {
    let mut h = send_harness(WINDOW_MIN);
    scroll_sidebar_down(&mut h);
    let before = count_top(&h);

    wheel_over(&mut h, PROPERTIES_KEY, 200.0);

    assert!(count_top(&h) > before, "{} vs {before}", count_top(&h));
}

#[test]
fn a_filled_properties_box_takes_the_wheel_over_it_but_not_beside_it() {
    let mut h = with_properties("source=load-test\nrun=7", WINDOW_MIN);
    scroll_sidebar_down(&mut h);
    let before = count_top(&h);

    wheel_over(&mut h, PROPERTIES_KEY, 200.0);
    let over_box = count_top(&h);
    wheel_over(&mut h, "send-correlation-id-group", 200.0);

    assert_eq!(over_box, before);
    assert!(count_top(&h) > before, "{} vs {before}", count_top(&h));
}

#[test]
fn the_first_keystroke_in_an_empty_properties_box_keeps_focus_and_caret() {
    let mut h = send_harness(WINDOW_SIZE);
    h.press(PROPERTIES_KEY);

    h.rt.paste("a");
    h.settle();
    h.rt.paste("=b");
    h.settle();

    assert_eq!(composer(&h).properties.text, "a=b");
    assert!(composer(&h).properties.focused);
}

#[test]
fn a_long_value_with_no_spaces_wraps_inside_the_box() {
    let value = "0123456789abcdef".repeat(5);
    let h = with_properties(&format!("trace={value}"), WINDOW_SIZE);
    let shell = rect(&h.rt, PROPERTIES_KEY);
    let input =
        h.rt.elements()
            .into_iter()
            .find(|element| {
                element.key.as_deref() == Some("input")
                    && element.path.contains(&format!("/{PROPERTIES_KEY}/"))
            })
            .unwrap_or_else(|| panic!("no Properties input"));

    assert!(input.rect[2] <= shell[2], "{:?} in {shell:?}", input.rect);
    assert!(
        input.rect[3] > 2.0 * crate::design::tokens::TYPE_MONO.line_px(),
        "{:?}",
        input.rect
    );
}

/// The input node named `name`: the one whose role ends in "text-input".
fn input_named<'a>(tree: &'a Value, name: &str) -> &'a Value {
    a11y_nodes(tree)
        .into_iter()
        .find(|node| {
            node["name"] == name
                && node["role"]
                    .as_str()
                    .is_some_and(|role| role.ends_with("text-input"))
        })
        .unwrap_or_else(|| panic!("no input named {name}"))
}

#[test]
fn the_metadata_fields_are_named_for_assistive_technology() {
    let h = send_harness(WINDOW_SIZE);
    let tree = h.a11y();

    let properties = input_named(&tree, "Properties");
    let body = input_named(&tree, "Event body");
    let correlation_id = input_named(&tree, "Correlation id");

    assert_eq!(properties["role"], body["role"]);
    assert_eq!(properties["description"], PROPERTIES_HINT);
    assert_eq!(correlation_id["role"], "text-input");
}

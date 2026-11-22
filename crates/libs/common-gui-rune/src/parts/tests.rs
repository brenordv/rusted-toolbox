use super::*;
use crate::Ds;
use crate::test_harness::{
    a11y_node, a11y_nodes, element, focused_key, has_text, label_of, press, press_key, rect,
    runtime_at,
};
use crate::tokens::TYPE_BODY;
use rstest::rstest;
use rune::{Cx, Key, Runtime};

const SIZE: (f32, f32) = (480.0, 360.0);
const EDITOR_KEY: &str = "notes";
const STATUS_KEY: &str = "status";
const LINES: f32 = 4.0;

struct Form {
    ds: Ds,
    edit: TextEdit,
    font: TextStyle,
    status: String,
    status_lines: StatusLines,
}

fn form(font: TextStyle, status: &str) -> Form {
    Form {
        ds: Ds::new(),
        edit: TextEdit::new(""),
        font,
        status: status.to_owned(),
        status_lines: StatusLines::Wrap,
    }
}

fn newline_on_return(form: &mut Form, event: &KeyEvent, _: &mut Tx) -> bool {
    if event.key != Key::Enter {
        return false;
    }
    form.edit.insert("\n");
    true
}

fn view(form: &Form, _: &mut Cx<Form>) -> El {
    let k = form.ds.kit(true);
    let spec = EditorSpec {
        key: EDITOR_KEY,
        label: "Notes",
        placeholder: "Type a note",
        description: None,
        invalid: false,
        height: EditorHeight::Fixed { lines: LINES },
        font: form.font,
    };
    col()
        .w_full()
        .h_full()
        .gap(SPACE_SM)
        .child(text_editor(
            k,
            spec,
            &form.edit,
            |form: &mut Form| &mut form.edit,
            newline_on_return,
            |_: &mut Form, _: &mut Tx| {},
        ))
        .child(status_line(
            k,
            STATUS_KEY,
            Tone::Danger,
            form.status_lines,
            &form.status,
        ))
}

fn runtime(form: Form) -> Runtime<Form> {
    runtime_at(form, view, SIZE)
}

fn focus_is_inside(rt: &Runtime<Form>, key: &str) -> bool {
    let shell = element(rt, key).path;
    rt.elements()
        .into_iter()
        .any(|element| element.focused && element.path.starts_with(&format!("{shell}/")))
}

#[rstest]
#[case::body(TYPE_BODY)]
#[case::mono(TYPE_MONO)]
fn a_text_editor_mounts_with_its_label_and_a_height_of_its_roles_lines(#[case] font: TextStyle) {
    let rt = runtime(form(font, "ok"));

    let viewport = rt
        .elements()
        .into_iter()
        .find(|element| element.key.as_deref() == Some("scroll"))
        .map(|element| element.rect[3]);

    assert_eq!(label_of(&rt, "input").as_deref(), Some("Notes"));
    let tree = rt.accessibility_json();
    let input = a11y_nodes(&tree)
        .into_iter()
        .find(|node| node["name"] == "Notes")
        .unwrap();
    assert_eq!(input["placeholder"], "Type a note", "{input}");
    assert_eq!(viewport, Some(LINES * font.line_px()));
    assert_eq!(
        rect(&rt, EDITOR_KEY)[3],
        LINES * font.line_px() + 2.0 * SPACE_SM
    );
}

#[test]
fn a_press_focuses_the_editor_and_return_reaches_its_key_handler() {
    let mut rt = runtime(form(TYPE_BODY, "ok"));

    press(&mut rt, EDITOR_KEY);
    let focused = focus_is_inside(&rt, EDITOR_KEY);
    rt.paste("one");
    press_key(&mut rt, Key::Enter);

    assert!(focused, "focus: {:?}", focused_key(&rt));
    assert_eq!(rt.state.edit.text, "one\n");
    assert!(has_text(&rt, "ok"));
}

#[test]
fn a_long_status_shows_cut_and_carries_the_whole_text_as_its_description() {
    let whole = format!("Couldn't reach the server: {}", "x".repeat(200));
    let rt = runtime(form(TYPE_BODY, &whole));

    let tree = rt.accessibility_json();
    let status = a11y_node(&tree, STATUS_KEY).unwrap();
    let name = status["name"].as_str().unwrap_or_default();

    assert_eq!(status["role"], "status");
    assert!(
        status["states"]
            .as_array()
            .is_some_and(|states| states.iter().any(|state| state == "live-polite")),
        "{status}"
    );
    assert_eq!(name.chars().count(), 141, "{name}");
    assert!(name.ends_with('…'), "{name}");
    assert_eq!(status["description"], whole.as_str());
    assert!(has_text(&rt, name));
}

#[test]
fn a_short_status_is_named_by_its_text_and_has_no_description() {
    let rt = runtime(form(TYPE_BODY, "Saved"));

    let tree = rt.accessibility_json();
    let status = a11y_node(&tree, STATUS_KEY).unwrap();

    assert_eq!(status["name"], "Saved");
    assert!(status["description"].is_null(), "{status}");
}

#[test]
fn a_one_line_status_stays_one_caption_line_tall() {
    let mut long = form(TYPE_BODY, &"word ".repeat(28));
    long.status_lines = StatusLines::One;
    let rt = runtime(long);

    assert_eq!(rect(&rt, STATUS_KEY)[3], TYPE_CAPTION.line_px());
}

#[test]
fn a_wrapping_status_takes_more_than_one_line_when_it_needs_them() {
    let rt = runtime(form(TYPE_BODY, &"word ".repeat(28)));

    assert!(rect(&rt, STATUS_KEY)[3] > TYPE_CAPTION.line_px());
}

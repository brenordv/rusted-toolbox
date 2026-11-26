use super::*;
use crate::Ds;
use crate::parts::{LineFieldSpec, line_field};
use rune::{FileDialogOptions, FileDialogResult, TextEdit, col};
use rune_kit::{Button, ButtonVariant};
use std::path::PathBuf;

const FIELD_KEY: &str = "name";
const OPEN_KEY: &str = "open";
const COPY_KEY: &str = "copy";
const NOTE_KEY: &str = "note";
const SIZE: (f32, f32) = (360.0, 240.0);

struct Form {
    ds: Ds,
    name: TextEdit,
    picked: Vec<FileDialogResult>,
    posted: usize,
}

fn view(form: &Form, _: &mut Cx<Form>) -> El {
    let k = form.ds.kit(true);
    let open = Button::new("Open…")
        .variant(ButtonVariant::Neutral)
        .on_press_tx(|_: &mut Form, tx: &mut Tx| {
            tx.open_file(
                FileDialogOptions {
                    title: "Open a file".to_owned(),
                    ..FileDialogOptions::default()
                },
                |form: &mut Form, answer: FileDialogResult, _: &mut Tx| form.picked.push(answer),
            );
        })
        .build(k)
        .key(OPEN_KEY);
    let copy = Button::new("Copy")
        .variant(ButtonVariant::Ghost)
        .on_press_tx(|form: &mut Form, tx: &mut Tx| tx.copy(form.name.text.to_string()))
        .build(k)
        .key(COPY_KEY);
    col()
        .w_full()
        .h_full()
        .child(line_field(
            k,
            LineFieldSpec {
                key: FIELD_KEY,
                label: "Name",
                placeholder: "",
                issue: None,
            },
            &form.name,
            |form: &mut Form| &mut form.name,
            |_: &mut Form, _: &KeyEvent, _: &mut Tx| false,
            |_: &mut Form, _: &mut Tx| {},
        ))
        .child(open)
        .child(copy)
        .child(rune::text("Not a field").key(NOTE_KEY))
}

fn runtime() -> Runtime<Form> {
    runtime_at(
        Form {
            ds: Ds::new(),
            name: TextEdit::new("old text"),
            picked: Vec::new(),
            posted: 0,
        },
        view,
        SIZE,
    )
}

#[test]
fn type_into_replaces_what_the_field_held() {
    let mut rt = runtime();

    type_into(&mut rt, FIELD_KEY, "new");

    assert_eq!(rt.state.name.text, "new");
    assert!(focus_is_inside(&rt, FIELD_KEY));
}

#[test]
#[should_panic(expected = "note did not take focus")]
fn type_into_fails_naming_the_key_when_focus_does_not_land() {
    let mut rt = runtime();

    type_into(&mut rt, NOTE_KEY, "text");
}

#[test]
fn answer_dialog_hands_the_path_to_the_dialogs_callback() {
    let mut rt = runtime();
    press(&mut rt, OPEN_KEY);
    let path = PathBuf::from("picked.txt");

    answer_dialog(&mut rt, FileDialogKind::Open, Some(&path));

    assert_eq!(rt.state.picked, vec![Ok(Some(path))]);
}

#[test]
#[should_panic(expected = "the dialog showing is not Open")]
fn answer_dialog_fails_when_no_dialog_shows() {
    let mut rt = runtime();

    answer_dialog(&mut rt, FileDialogKind::Open, None);
}

#[test]
#[should_panic(expected = "the dialog showing is not Save")]
fn answer_dialog_fails_when_another_kind_shows() {
    let mut rt = runtime();
    press(&mut rt, OPEN_KEY);

    answer_dialog(&mut rt, FileDialogKind::Save, None);
}

#[test]
fn a_memory_clipboard_holds_what_the_app_copied() {
    let mut rt = runtime();
    let clipboard = use_memory_clipboard(&mut rt);

    press(&mut rt, COPY_KEY);

    assert_eq!(clipboard_text(&clipboard).as_deref(), Some("old text"));
}

#[test]
fn deliver_applies_the_post_and_settles() {
    let mut rt = runtime();

    deliver(&mut rt, |form: &mut Form, _| {
        form.posted += 1;
        true
    });

    assert_eq!(rt.state.posted, 1);
}

#[test]
fn bottom_is_where_the_element_ends() {
    let rt = runtime();

    let [_, y, _, height] = rect(&rt, OPEN_KEY);

    assert_eq!(bottom(&rt, OPEN_KEY), y + height);
}

#[test]
fn an_enabled_button_does_not_read_as_disabled() {
    let rt = runtime();

    assert!(!is_disabled(&rt, OPEN_KEY));
}

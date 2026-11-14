use super::*;
use crate::form::ConnectForm;
use common_cli::app_logger::FileLogging;
use rune::{Env, Runtime};

const MARKER: &str = "SharedAccessKey=marker-55219";

fn runtime(state: AppState) -> Runtime<AppState> {
    let mut rt = Runtime::new(state, view, (1280.0, 800.0), Env::new(true, 1.0));
    rt.frame(0.0);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    rt
}

fn settle(rt: &mut Runtime<AppState>) {
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
}

fn press(rt: &mut Runtime<AppState>, key: &str) {
    let (x, y) = rt
        .find(key)
        .unwrap_or_else(|| panic!("no element keyed {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

fn state() -> AppState {
    AppState::new(None, ConnectForm::default(), None, FileLogging::Off)
}

fn has_key(rt: &Runtime<AppState>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|element| element.key.as_deref() == Some(key))
}

#[test]
fn pressing_export_switches_the_section() {
    let mut rt = runtime(state());

    press(&mut rt, "segment1");

    assert_eq!(rt.state.section, crate::state::Section::Export);
    assert!(has_key(&rt, "export-empty"));
}

#[test]
fn a_disconnected_window_offers_connect_and_opens_the_sheet() {
    let mut rt = runtime(state());

    press(&mut rt, "empty-connect");

    assert!(rt.state.sheet_open);
    assert!(has_key(&rt, "connect-form"));
}

#[test]
fn the_sheet_shows_only_the_chosen_methods_secret_field() {
    let mut s = state();
    s.sheet_open = true;
    let mut rt = runtime(s);
    let connection_string_first = has_key(&rt, "connection-string");

    rt.state.form.set_auth(crate::form::AuthChoice::Credential);
    rt.invalidate();
    settle(&mut rt);

    assert!(connection_string_first);
    assert!(!has_key(&rt, "connection-string"));
}

#[test]
fn a_typed_secret_reaches_neither_the_element_tree_nor_accessibility() {
    let mut s = state();
    s.sheet_open = true;
    s.form.connection_string.edit_mut().replace_text(MARKER);
    let mut rt = runtime(s);
    let hidden = (
        rt.accessibility_json().to_string(),
        format!("{:?}", rt.elements()),
    );

    rt.state
        .form
        .connection_string
        .edit_mut()
        .set_revealed(true);
    rt.invalidate();
    settle(&mut rt);
    let revealed = (
        rt.accessibility_json().to_string(),
        format!("{:?}", rt.elements()),
    );

    for (accessibility, tree) in [hidden, revealed] {
        assert!(!accessibility.contains("marker-55219"));
        assert!(!tree.contains("marker-55219"));
    }
}

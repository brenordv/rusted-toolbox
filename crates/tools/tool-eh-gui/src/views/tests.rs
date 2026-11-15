use super::*;
use crate::bridge::UiDelivery;
use crate::bridge::test_support::detached_session;
use crate::design::tokens::WINDOW_MIN;
use crate::form::{AuthChoice, ConnectForm, CredentialChoice};
use crate::read::ReadInput;
use crate::read::rows::ROW_CAP;
use crate::read::test_support::captured;
use crate::state::{Effect, Input};
use common_cli::app_logger::FileLogging;
use rune::{Env, Key, KeyEvent, Mods, Runtime};
use shared_eventhub_core::engine::EngineEvent;
use shared_eventhub_core::stats::StatsSnapshot;

const MARKER: &str = "SharedAccessKey=marker-55219";

fn runtime(state: AppState) -> Runtime<AppState> {
    runtime_at(state, (1280.0, 800.0))
}

fn runtime_at(state: AppState, size: (f32, f32)) -> Runtime<AppState> {
    let mut rt = Runtime::new(state, view, size, Env::new(true, 1.0));
    rt.frame(0.0);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    rt
}

/// `[x, y, width, height]` of the element keyed `key`.
fn rect(rt: &Runtime<AppState>, key: &str) -> [f32; 4] {
    rt.elements()
        .into_iter()
        .find(|element| element.key.as_deref() == Some(key) && !element.exiting)
        .unwrap_or_else(|| panic!("no element keyed {key}"))
        .rect
}

fn press_key(rt: &mut Runtime<AppState>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods::default(),
    });
    settle(rt);
}

fn open_sheet(auth: AuthChoice) -> AppState {
    let mut s = state();
    s.form.set_auth(auth);
    s.sheet_open = true;
    s
}

/// A state connected to a session over four partitions; the runtime must outlive it.
fn connected(rt: &tokio::runtime::Runtime) -> AppState {
    let mut s = state();
    s.form
        .connection_string
        .edit_mut()
        .replace_text("Endpoint=sb://orders.servicebus.windows.net/;SharedAccessKey=k");
    let out = s.apply(Input::ConnectPressed);
    let [Effect::PrepareSession { attempt, .. }] = out.effects.as_slice() else {
        panic!("expected PrepareSession, got {:?}", out.effects);
    };
    let attempt = *attempt;
    let (session, _) = detached_session(attempt, rt.handle());
    s.apply(Input::SessionReady {
        attempt,
        result: Ok(session),
    });
    s.apply(Input::Delivery(UiDelivery {
        at: chrono::Utc::now(),
        session: attempt,
        events: vec![EngineEvent::Started {
            partitions: ["0", "1", "2", "3"].map(str::to_owned).to_vec(),
        }],
        stats: None,
    }));
    s
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

#[rstest::rstest]
#[case::connection_string(AuthChoice::ConnectionString, "connection-string")]
#[case::client_secret(AuthChoice::Credential, "client-secret")]
fn a_closed_sheet_mounts_no_dialog_and_no_secret_field(
    #[case] auth: AuthChoice,
    #[case] secret_key: &str,
) {
    let mut s = state();
    s.form.set_auth(auth);
    s.form.credential = CredentialChoice::ClientSecret;
    let mut rt = runtime(s);
    let closed = (has_key(&rt, "connect-dialog"), has_key(&rt, secret_key));

    rt.state.sheet_open = true;
    rt.invalidate();
    settle(&mut rt);

    assert_eq!(closed, (false, false));
    assert!(has_key(&rt, "connect-dialog"));
    assert!(has_key(&rt, secret_key));
}

#[test]
fn the_sheets_buttons_stay_on_screen_in_the_smallest_window() {
    let rt = runtime_at(open_sheet(AuthChoice::KeyVault), WINDOW_MIN);

    let [_, y, _, height] = rect(&rt, "sheet-connect");

    assert!(
        y >= 0.0 && y + height <= WINDOW_MIN.1,
        "Connect at {y}+{height}"
    );
}

#[test]
fn a_form_taller_than_the_sheet_scrolls_inside_it() {
    let rt = runtime_at(open_sheet(AuthChoice::KeyVault), WINDOW_MIN);

    let viewport = rect(&rt, "connect-scroll")[3];
    let form = rect(&rt, "connect-form")[3];

    assert!(form > viewport, "form {form} in a {viewport} viewport");
}

#[test]
fn escape_closes_the_sheet() {
    let mut rt = runtime(open_sheet(AuthChoice::ConnectionString));

    press_key(&mut rt, Key::Escape);

    assert!(!rt.state.sheet_open);
    assert!(!has_key(&rt, "connect-dialog"));
}

#[test]
fn a_click_outside_the_sheet_closes_it() {
    let mut rt = runtime(open_sheet(AuthChoice::ConnectionString));

    rt.pointer_down(20.0, 400.0);
    rt.pointer_up(20.0, 400.0);
    settle(&mut rt);

    assert!(!rt.state.sheet_open);
}

#[test]
fn tab_keeps_focus_inside_the_sheet() {
    let mut rt = runtime(open_sheet(AuthChoice::KeyVault));
    let dialog = rt
        .elements()
        .into_iter()
        .find(|element| element.key.as_deref() == Some("connect-dialog"))
        .map(|element| element.path)
        .unwrap();

    let mut outside = Vec::new();
    for _ in 0..24 {
        press_key(&mut rt, Key::Tab);
        let focused = rt.elements().into_iter().find(|element| element.focused);
        if let Some(element) = focused.filter(|element| !element.path.starts_with(&dialog)) {
            outside.push(element.path);
        }
    }

    assert!(outside.is_empty(), "focus left the sheet: {outside:?}");
}

#[test]
fn the_toolbar_fits_the_smallest_window_while_connected() {
    let tokio = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let rt = runtime_at(connected(&tokio), WINDOW_MIN);

    let [export_x, _, export_width, _] = rect(&rt, "segment1");
    let [pill_x, ..] = rect(&rt, "pill");

    assert!(
        export_x + export_width <= pill_x,
        "Export ends at {} but the pill starts at {pill_x}",
        export_x + export_width
    );
}

#[test]
fn the_received_tile_shows_the_count_with_separators() {
    let tokio = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut s = connected(&tokio);
    let session = s.connection.session().unwrap().id();
    s.apply(Input::Delivery(UiDelivery {
        at: chrono::Utc::now(),
        session,
        events: Vec::new(),
        stats: Some(StatsSnapshot {
            received: 12_403,
            stored: 12_380,
            ..StatsSnapshot::default()
        }),
    }));
    s.apply(Input::Tick(chrono::Utc::now()));

    let rt = runtime(s);

    let label = rt
        .elements()
        .into_iter()
        .find(|element| element.key.as_deref() == Some("stat-received"))
        .and_then(|element| element.label);
    assert_eq!(label.as_deref(), Some("Received 12,403"));
}

fn with_rows(s: &mut AppState, first: usize, count: usize) {
    let session = s.connection.session().unwrap().id();
    let events = (first..first + count)
        .map(|sequence| EngineEvent::Message(Box::new(captured("0", sequence as i64, "body"))))
        .collect();
    s.apply(Input::Delivery(UiDelivery {
        at: chrono::Utc::now(),
        session,
        events,
        stats: None,
    }));
    s.apply(Input::Read(ReadInput::ListFlush));
}

fn row_visible(rt: &Runtime<AppState>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|element| element.key.as_deref() == Some(key) && element.visible[3] > 0.0)
}

fn built_rows(rt: &Runtime<AppState>) -> usize {
    rt.elements()
        .iter()
        .filter(|element| {
            element
                .key
                .as_deref()
                .is_some_and(|key| key.starts_with("p0-s"))
        })
        .count()
}

#[test]
fn a_full_list_builds_only_the_rows_near_the_viewport() {
    let tokio = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut s = connected(&tokio);
    with_rows(&mut s, 0, ROW_CAP);

    let rt = runtime(s);

    let built = built_rows(&rt);
    assert!(built > 0 && built < 200, "built {built} rows");
    assert!(row_visible(&rt, &format!("p0-s{}", ROW_CAP - 1)));
}

#[test]
fn the_list_follows_the_tail_until_scrolled_up_then_the_pill_brings_it_back() {
    let tokio = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut s = connected(&tokio);
    with_rows(&mut s, 0, 200);
    let mut rt = runtime(s);

    with_rows(&mut rt.state, 200, 5);
    rt.invalidate();
    settle(&mut rt);
    assert!(row_visible(&rt, "p0-s204"), "the tail is followed");

    for _ in 0..10 {
        rt.wheel(640.0, 500.0, 400.0);
        rt.frame(1.0 / 60.0);
    }
    with_rows(&mut rt.state, 205, 5);
    rt.state.apply(Input::Tick(chrono::Utc::now()));
    rt.invalidate();
    settle(&mut rt);
    assert!(!row_visible(&rt, "p0-s209"), "scrolled up, the view stays");
    assert!(has_key(&rt, "new-messages"), "the pill shows");

    press(&mut rt, "new-messages");
    settle(&mut rt);

    assert!(row_visible(&rt, "p0-s209"));
}

#[test]
fn the_pill_counts_session_log_lines_until_its_popover_opens() {
    let tokio = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut s = connected(&tokio);
    let session = s.connection.session().unwrap().id();
    s.apply(Input::Delivery(UiDelivery {
        at: chrono::Utc::now(),
        session,
        events: vec![EngineEvent::Error {
            partition_id: Some("2".to_owned()),
            error: "link lost".to_owned(),
            fatal: false,
        }],
        stats: None,
    }));
    let mut rt = runtime(s);
    assert_eq!(rt.state.read.session_log_unseen, 1);

    press(&mut rt, "pill");

    assert_eq!(rt.state.read.session_log_unseen, 0);
    assert!(rt.elements().iter().any(|element| {
        element
            .text
            .as_deref()
            .is_some_and(|text| text.contains("link lost"))
    }));
}

#[test]
fn a_stale_feed_shows_on_the_pill() {
    let tokio = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut s = connected(&tokio);
    s.read.feed_stale = true;

    let rt = runtime(s);

    let label = rt
        .elements()
        .into_iter()
        .find(|element| element.key.as_deref() == Some("pill"))
        .and_then(|element| element.label);
    assert_eq!(label.as_deref(), Some("Connection: Not receiving updates"));
}

#[test]
fn the_find_shortcut_focuses_the_quick_search() {
    let tokio = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut s = connected(&tokio);
    with_rows(&mut s, 0, 3);
    let mut rt = runtime(s);
    rt.on_key = Some(window_key);

    rt.key(KeyEvent {
        key: Key::Command('f'),
        mods: Mods {
            cmd: true,
            ctrl: !cfg!(target_os = "macos"),
            logo: cfg!(target_os = "macos"),
            ..Mods::default()
        },
    });
    settle(&mut rt);

    assert!(rt.state.read.search_edit.focused);
}

#[test]
fn the_inspector_close_button_clears_the_selection() {
    let tokio = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut s = connected(&tokio);
    with_rows(&mut s, 0, 3);
    let mut rt = runtime(s);
    press(&mut rt, "p0-s1");

    press(&mut rt, "inspector-close");

    assert!(rt.state.read.selected.is_none());
    assert!(!rt.state.read.inspector.is_open());
}

#[test]
fn clicking_a_row_opens_the_inspector_and_escape_closes_it() {
    let tokio = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut s = connected(&tokio);
    with_rows(&mut s, 0, 3);
    let mut rt = runtime(s);

    press(&mut rt, "p0-s1");
    assert!(has_key(&rt, "inspector"));

    press_key(&mut rt, Key::Escape);

    assert!(rt.state.read.selected.is_none());
    assert!(!has_key(&rt, "inspector"));
}

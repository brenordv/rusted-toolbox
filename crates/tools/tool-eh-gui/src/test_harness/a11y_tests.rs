use super::sweep_tests::{
    export_not_searched, orders_form, read_disconnected, read_selected, send_idle,
    send_sending_run, sheet, sheet_send_target,
};
use super::{Harness, a11y_node, has_key, lists};
use crate::design::tokens::WINDOW_SIZE;
use crate::form::AuthChoice;
use crate::send::SendInput;
use crate::state::{Input, SheetPurpose};
use crate::views::SEND_BUTTON_KEY;

const MARKER: &str = "marker-55219";

#[test]
fn a_message_row_is_named_by_its_partition_sequence_and_time() {
    let h = read_selected(WINDOW_SIZE);

    let tree = h.a11y();
    let name = a11y_node(&tree, "p0-s39").and_then(|row| row["name"].as_str());

    assert!(
        name.is_some_and(|name| name.starts_with("Partition 0, sequence 39, enqueued ")),
        "{name:?}"
    );
}

/// What a press of a primary action changed: calls recorded and whether a sheet is open.
fn effect(h: &Harness) -> (usize, bool) {
    (h.calls.count(|_| true), h.state().sheet.is_some())
}

fn connect_sheet(size: (f32, f32)) -> Harness {
    let mut s = orders_form();
    s.sheet = Some(SheetPurpose::Connect);
    Harness::new(s, size)
}

#[rstest::rstest]
#[case::connect(read_disconnected, "empty-connect", (0, true))]
#[case::search(export_not_searched, "archive-search-button", (1, false))]
#[case::send(send_idle, SEND_BUTTON_KEY, (1, false))]
#[case::sheet_connect(connect_sheet, "sheet-connect", (1, false))]
#[case::sheet_done(sheet_send_target, "sheet-done", (0, false))]
fn an_assistive_click_does_what_a_press_does(
    #[case] build: fn((f32, f32)) -> Harness,
    #[case] key: &str,
    #[case] expected: (usize, bool),
) {
    let mut pressed = build(WINDOW_SIZE);
    let mut activated = build(WINDOW_SIZE);

    pressed.press(key);
    activated.activate(key);

    assert_eq!(effect(&pressed), expected);
    assert_eq!(effect(&activated), expected);
}

#[test]
fn the_send_status_is_a_polite_live_region_that_announces_no_numbers() {
    let (mut h, run) = send_sending_run(WINDOW_SIZE);
    let before = h.a11y();

    h.deliver(Input::Send(SendInput::Progress { run, sent: 2 }));
    h.tick();
    let after = h.a11y();

    for tree in [&before, &after] {
        let Some(status) = a11y_node(tree, "send-status-sending") else {
            panic!("no sending status in the tree");
        };
        assert_eq!(status["role"], "status");
        assert!(lists(status, "states", "live-polite"), "{status}");
        assert!(status["children"].is_null(), "{status}");
        assert_eq!(status["name"], "Sending to orders");
    }
}

#[rstest::rstest]
#[case::connect(SheetPurpose::Connect, "sheet-connect")]
#[case::send_target(SheetPurpose::SendTarget, "sheet-done")]
fn the_sheet_submit_is_the_default_button(#[case] purpose: SheetPurpose, #[case] key: &str) {
    let mut h = sheet_send_target(WINDOW_SIZE);
    h.rt.state.sheet = Some(purpose);
    h.rt.invalidate();
    h.settle();

    let tree = h.a11y();

    assert_eq!(
        a11y_node(&tree, key).map(|node| node["role"].clone()),
        Some("default-button".into())
    );
}

#[rstest::rstest]
#[case::connection_string(AuthChoice::ConnectionString, "connection-string", false)]
#[case::connection_string_revealed(AuthChoice::ConnectionString, "connection-string", true)]
#[case::client_secret(AuthChoice::Credential, "client-secret", false)]
#[case::client_secret_revealed(AuthChoice::Credential, "client-secret", true)]
#[case::key_vault_client_secret(AuthChoice::KeyVault, "client-secret", false)]
#[case::key_vault_client_secret_revealed(AuthChoice::KeyVault, "client-secret", true)]
fn a_typed_secret_reaches_neither_the_tree_nor_the_elements(
    #[case] auth: AuthChoice,
    #[case] field: &str,
    #[case] revealed: bool,
) {
    let mut h = sheet(WINDOW_SIZE, auth);
    let secret = match auth {
        AuthChoice::ConnectionString => &mut h.rt.state.form.connection_string,
        AuthChoice::Credential | AuthChoice::KeyVault => &mut h.rt.state.form.client_secret,
    };
    let edit = secret.edit_mut();
    edit.replace_text(format!("SharedAccessKey={MARKER}"));
    edit.set_revealed(revealed);
    h.rt.invalidate();
    h.settle();
    let typed = h.rt.state.form.to_config().1;
    let live = [typed.connection_string, typed.client_secret]
        .iter()
        .flatten()
        .any(|value| value.expose().contains(MARKER));

    let tree = h.a11y().to_string();
    let elements = format!("{:?}", h.rt.elements());

    assert!(live && has_key(&h.rt, field), "the marker is not typed");
    assert!(!tree.contains(MARKER));
    assert!(!elements.contains(MARKER));
}

#[test]
fn a_sheet_hides_the_window_behind_it_from_assistive_technology() {
    let mut h = read_disconnected(WINDOW_SIZE);
    let open = h.a11y();
    h.press("empty-connect");

    let tree = h.a11y();

    assert!(a11y_node(&open, "pill").is_some());
    assert!(a11y_node(&tree, "pill").is_none());
    assert!(a11y_node(&tree, "empty-connect").is_none());
}

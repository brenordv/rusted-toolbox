use super::*;
use crate::archive::results::{More, Results};
use crate::archive::{ArchiveEffect, ArchiveInput};
use crate::bridge::UiDelivery;
use crate::design::tokens::WINDOW_MIN;
use crate::form::{AuthChoice, CredentialChoice};
use crate::read::rows::ROW_CAP;
use crate::state::{Effect, Input};
use crate::test_harness::builders::{
    connected, export_results, export_state, open_sheet, record, send_running, send_state, state,
    template_state, with_rows,
};
use crate::test_harness::{
    command, focused_key, has_key, has_text, press, press_key, rect, row_visible, runtime,
    runtime_at, settle, text_visible,
};
use common_cli::app_logger::FileLogging;
use rune::{Key, KeyEvent, Mods, Runtime};
use shared_eventhub_core::engine::EngineEvent;
use shared_eventhub_core::stats::StatsSnapshot;
use shared_eventhub_persistence::export::ExportOutcome;

const MARKER: &str = "SharedAccessKey=marker-55219";

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

    assert!(rt.state.sheet.is_some());
    assert!(has_key(&rt, "connect-form"));
}

#[test]
fn the_sheet_shows_only_the_chosen_methods_secret_field() {
    let mut s = state();
    s.sheet = Some(crate::state::SheetPurpose::Connect);
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
    s.sheet = Some(crate::state::SheetPurpose::Connect);
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

    rt.state.sheet = Some(crate::state::SheetPurpose::Connect);
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

    assert!(rt.state.sheet.is_none());
    assert!(!has_key(&rt, "connect-dialog"));
}

#[test]
fn a_click_outside_the_sheet_closes_it() {
    let mut rt = runtime(open_sheet(AuthChoice::ConnectionString));

    rt.pointer_down(20.0, 400.0);
    rt.pointer_up(20.0, 400.0);
    settle(&mut rt);

    assert!(rt.state.sheet.is_none());
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

    assert!(rt.state.read.inspector.selected.is_none());
    assert!(!rt.state.read.inspector.pane.is_open());
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

    assert!(rt.state.read.inspector.selected.is_none());
    assert!(!has_key(&rt, "inspector"));
}

#[rstest::rstest]
#[case::not_searched(Results::NotSearched, "export-empty")]
#[case::searching(Results::Loading { generation: 1 }, "export-searching")]
#[case::unavailable(
    Results::Unavailable { generation: 1, error: "in use by another eh".to_owned() },
    "export-unavailable"
)]
#[case::no_matches(
    Results::Shown {
        generation: 1,
        rows: crate::archive::results::ArchiveRows::default(),
        more: More::End,
    },
    "export-no-matches"
)]
fn each_export_state_shows_its_empty_state(#[case] results: Results, #[case] key: &str) {
    let rt = runtime(export_state(results));

    assert!(has_key(&rt, key));
    assert!(!has_key(&rt, "bulk-bar"));
}

#[test]
fn results_show_in_the_list_with_the_export_bar() {
    let (s, _) = export_results(3, false);

    let rt = runtime(s);

    assert!(row_visible(&rt, "p0-s0"));
    assert!(has_key(&rt, "bulk-bar") && has_key(&rt, "bulk-export"));
}

#[test]
fn scrolling_near_the_end_asks_for_one_page_and_shows_the_footer_spinner() {
    let (s, pages) = export_results(300, true);
    let mut rt = runtime(s);
    let before = pages.searches();

    for _ in 0..40 {
        rt.wheel(640.0, 500.0, -2_000.0);
        rt.frame(1.0 / 60.0);
    }
    settle(&mut rt);

    assert_eq!(before, 0);
    assert_eq!(pages.searches(), 1);
    assert!(matches!(
        &rt.state.archive.results,
        Results::Shown {
            more: More::Loading(_),
            ..
        }
    ));
    assert!(has_key(&rt, "results-footer"));
}

#[test]
fn a_running_export_shows_its_counts_and_cancel() {
    let (mut s, _) = export_results(3, false);
    s.apply(Input::Archive(ArchiveInput::BulkExportPressed));

    let mut rt = runtime(s);
    press(&mut rt, "bulk-cancel");

    assert!(matches!(
        rt.state.archive.bulk,
        crate::archive::bulk::BulkExport::Running {
            cancel_requested: true,
            ..
        }
    ));
}

#[test]
fn a_finished_export_shows_done_which_returns_to_idle() {
    let (mut s, _) = export_results(3, false);
    let out = s.apply(Input::Archive(ArchiveInput::BulkExportPressed));
    let [Effect::Archive(ArchiveEffect::BulkExport(request))] = out.effects.as_slice() else {
        panic!("expected a bulk export, got {:?}", out.effects);
    };
    let export = request.export;
    s.apply(Input::Archive(ArchiveInput::BulkDone {
        export,
        elapsed_ms: 1,
        result: Ok(ExportOutcome::default()),
    }));
    let mut rt = runtime(s);

    press(&mut rt, "bulk-done");

    assert!(matches!(
        rt.state.archive.bulk,
        crate::archive::bulk::BulkExport::Idle
    ));
    assert!(has_key(&rt, "bulk-export"));
}

#[rstest::rstest]
#[case::logging_off(FileLogging::Off, false)]
#[case::writing(FileLogging::Writing("eh-gui.log".into()), true)]
fn a_failed_export_points_at_the_log_only_when_one_is_written(
    #[case] logging: FileLogging,
    #[case] points: bool,
) {
    let (mut s, _) = export_results(3, false);
    s.file_logging = logging;
    let out = s.apply(Input::Archive(ArchiveInput::BulkExportPressed));
    let [Effect::Archive(ArchiveEffect::BulkExport(request))] = out.effects.as_slice() else {
        panic!("expected a bulk export, got {:?}", out.effects);
    };
    let export = request.export;
    s.apply(Input::Archive(ArchiveInput::BulkDone {
        export,
        elapsed_ms: 1,
        result: Ok(ExportOutcome {
            failed: 1,
            first_error: Some("x.txt: disk full".to_owned()),
            ..ExportOutcome::default()
        }),
    }));

    let rt = runtime(s);

    assert!(has_text(&rt, "x.txt: disk full"));
    assert_eq!(has_text(&rt, "Details are in the log file."), points);
}

#[test]
fn the_sheet_says_why_the_data_folder_cannot_change_during_a_bulk_export() {
    let (mut s, _) = export_results(3, false);
    s.apply(Input::Archive(ArchiveInput::BulkExportPressed));
    s.sheet = Some(crate::state::SheetPurpose::Connect);

    let rt = runtime(s);

    assert!(has_key(&rt, "change-folder"));
    assert!(has_text(&rt, "once the bulk export ends"));
}

#[test]
fn the_export_options_fit_inside_their_popover() {
    let (s, _) = export_results(3, false);
    let mut rt = runtime(s);

    press(&mut rt, "bulk-options");

    let [x, y, w, h] = rect(&rt, "bulk-options-trigger-content");
    let inside = |key: &str| {
        let [cx, cy, cw, ch] = rect(&rt, key);
        cx >= x && cy >= y && cx + cw <= x + w + 0.5 && cy + ch <= y + h + 0.5
    };
    assert!(rt.state.archive.options_open);
    assert!(w >= crate::design::tokens::FORM_POPOVER_WIDTH);
    assert!(inside("bulk-options-panel"));
    assert!(inside("bulk-destination"));
}

#[test]
fn pressing_outside_the_export_options_closes_them() {
    let (mut s, _) = export_results(3, false);
    s.archive.options_open = true;
    let mut rt = runtime(s);

    press(&mut rt, "bulk-options-scrim");

    assert!(!rt.state.archive.options_open);
}

#[test]
fn the_search_button_searches_the_archive() {
    let mut s = state();
    let pages = record(&mut s);
    s.section = crate::state::Section::Export;
    let mut rt = runtime(s);

    press(&mut rt, "archive-search-button");

    assert_eq!(pages.searches(), 1);
    assert!(matches!(rt.state.archive.results, Results::Loading { .. }));
}

fn send_runtime(s: AppState, size: (f32, f32)) -> Runtime<AppState> {
    let mut rt = runtime_at(s, size);
    rt.on_key = Some(window_key);
    rt
}

#[test]
fn the_third_segment_shows_the_send_section_with_send_on_screen_at_the_smallest_size() {
    let mut rt = runtime_at(state(), WINDOW_MIN);

    press(&mut rt, "segment2");

    let [_, y, _, height] = rect(&rt, crate::views::send::SEND_BUTTON_KEY);
    assert_eq!(rt.state.section, crate::state::Section::Send);
    assert!(
        y >= 0.0 && y + height <= WINDOW_MIN.1,
        "Send at {y}+{height}"
    );
}

#[test]
fn mod_3_shows_the_send_section_with_nothing_focused() {
    let mut rt = send_runtime(state(), (1280.0, 800.0));

    command(&mut rt, Key::Command('3'));

    assert_eq!(rt.state.section, crate::state::Section::Send);
}

#[test]
fn return_in_the_body_types_a_newline_and_sends_nothing() {
    let (s, sends) = send_state();
    let mut rt = send_runtime(s, (1280.0, 800.0));
    press(&mut rt, crate::views::send::BODY_KEY);

    press_key(&mut rt, Key::Enter);

    assert_eq!(rt.state.send.composer.body.text, "\n");
    assert_eq!(sends.sends(), 0);
}

#[test]
fn return_in_the_count_sends_nothing() {
    let (s, sends) = send_state();
    let mut rt = send_runtime(s, (1280.0, 800.0));
    press(&mut rt, crate::views::send::sidebar::COUNT_KEY);

    press_key(&mut rt, Key::Enter);

    assert_eq!(sends.sends(), 0);
}

#[rstest::rstest]
#[case::body(Some(crate::views::send::BODY_KEY))]
#[case::count(Some(crate::views::send::sidebar::COUNT_KEY))]
#[case::nothing_focused(None)]
fn mod_enter_sends_from_a_focused_field_and_with_nothing_focused(#[case] field: Option<&str>) {
    let (s, sends) = send_state();
    let mut rt = send_runtime(s, (1280.0, 800.0));
    if let Some(field) = field {
        press(&mut rt, field);
        assert!(focused_key(&rt).is_some(), "{field} did not take focus");
    }

    command(&mut rt, Key::Enter);

    assert_eq!(sends.sends(), 1);
}

#[test]
fn mod_enter_with_the_sheet_open_sends_nothing() {
    let (mut s, sends) = send_state();
    s.sheet = Some(crate::state::SheetPurpose::SendTarget);
    let mut rt = send_runtime(s, (1280.0, 800.0));

    command(&mut rt, Key::Enter);

    assert_eq!(sends.sends(), 0);
}

#[rstest::rstest]
#[case::body(crate::views::send::BODY_KEY)]
#[case::count(crate::views::send::sidebar::COUNT_KEY)]
fn mod_1_switches_sections_while_a_send_field_has_focus(#[case] field: &str) {
    let (s, _) = send_state();
    let mut rt = send_runtime(s, (1280.0, 800.0));
    press(&mut rt, field);

    command(&mut rt, Key::Command('1'));

    assert_eq!(rt.state.section, crate::state::Section::Read);
}

/// The live region's phrase: the label of the status block.
fn status_label(rt: &Runtime<AppState>) -> String {
    rt.elements()
        .into_iter()
        .find(|element| {
            !element.exiting
                && element
                    .key
                    .as_deref()
                    .is_some_and(|key| key.starts_with(crate::views::send::STATUS_KEY))
        })
        .and_then(|element| element.label)
        .unwrap_or_default()
}

#[test]
fn a_running_send_shows_its_count_and_cancel_keeps_the_line() {
    let (mut s, _) = send_state();
    send_running(&mut s, false);
    let mut rt = runtime(s);
    let counted = has_text(&rt, "of 1,000 to orders");

    press(&mut rt, "send-cancel");

    assert!(counted);
    assert!(rt.state.send.run.cancel_requested());
    assert!(has_text(&rt, "Canceling"));
    assert!(has_text(&rt, "of 1,000 to orders"));
}

#[test]
fn a_paced_send_names_its_pacing_and_its_elapsed_time() {
    let (mut s, _) = send_state();
    send_running(&mut s, true);

    let rt = runtime(s);

    assert!(has_text(&rt, "one every 1,000 ms"));
    assert!(has_text(&rt, "· 4s"));
}

#[test]
fn the_status_announces_words_never_digits_while_sending() {
    let (mut s, _) = send_state();
    send_running(&mut s, true);

    let rt = runtime(s);

    let label = status_label(&rt);
    assert_eq!(label, "Sending to orders");
    assert!(!label.chars().any(|c| c.is_ascii_digit()));
}

#[test]
fn picking_paced_reveals_the_interval_and_batched_removes_it() {
    let (s, _) = send_state();
    let mut rt = runtime(s);
    let hidden = has_key(&rt, crate::views::send::sidebar::INTERVAL_KEY);

    rt.state.send.composer.pacing = crate::send::compose::PacingChoice::Paced;
    rt.invalidate();
    settle(&mut rt);
    let shown = has_key(&rt, crate::views::send::sidebar::INTERVAL_KEY);
    rt.state.send.composer.pacing = crate::send::compose::PacingChoice::Batched;
    rt.invalidate();
    settle(&mut rt);

    assert!(!hidden && shown);
    assert!(!row_visible(&rt, crate::views::send::sidebar::INTERVAL_KEY));
}

#[test]
fn an_invalid_interval_shows_its_issue_reachable_in_the_smallest_window() {
    let (mut s, _) = send_state();
    s.send.composer.pacing = crate::send::compose::PacingChoice::Paced;
    s.send.composer.routing = crate::send::compose::RoutingChoice::PartitionKey;
    s.send.composer.interval.replace_text("0");
    s.apply(Input::Send(crate::send::SendInput::SendPressed));

    let mut rt = runtime_at(s, WINDOW_MIN);
    let issue = "Enter an interval from 1 to 3,600,000 ms (one hour)";
    let shown_at_first = text_visible(&rt, issue);

    for _ in 0..20 {
        rt.wheel(120.0, 400.0, -2_000.0);
        rt.frame(1.0 / 60.0);
    }
    settle(&mut rt);

    assert!(has_text(&rt, "Fix 2 fields to send"));
    assert!(!shown_at_first, "the issue starts below the fold");
    assert!(
        text_visible(&rt, issue),
        "scrolling the sidebar reaches the issue"
    );
}

#[test]
fn a_send_with_issues_moves_focus_into_the_first_invalid_field() {
    let (mut s, sends) = send_state();
    s.send.composer.count.replace_text("0");
    let mut rt = send_runtime(s, (1280.0, 800.0));

    press(&mut rt, crate::views::send::SEND_BUTTON_KEY);

    assert_eq!(sends.sends(), 0);
    assert!(rt.state.send.composer.count.focused);
}

#[test]
fn pasting_into_the_body_with_template_on_recompiles_it() {
    let (s, _) = template_state("");
    let mut rt = send_runtime(s, (1280.0, 800.0));
    press(&mut rt, crate::views::send::BODY_KEY);

    rt.paste("{{random.integer min=4 max=4}}");
    settle(&mut rt);

    let template = rt.state.send.composer.template().template();
    assert_eq!(template.map(|t| t.placeholder_count()), Some(1));
    assert!(has_text(&rt, "4"));
}

#[test]
fn return_in_the_body_with_template_on_recompiles_it() {
    let (s, _) = template_state("{{nope}}");
    let mut rt = send_runtime(s, (1280.0, 800.0));
    rt.state.send.composer.body = rune::TextEdit::new("{{random.integer}}");
    let stale = rt.state.send.composer.template().issue().is_some();
    press(&mut rt, crate::views::send::BODY_KEY);
    press_key(&mut rt, Key::End);

    press_key(&mut rt, Key::Enter);

    assert!(stale);
    assert_eq!(rt.state.send.composer.body.text, "{{random.integer}}\n");
    assert!(rt.state.send.composer.template().template().is_some());
}

#[test]
fn the_switch_shows_and_removes_the_sample_block() {
    let (s, _) = send_state();
    let mut rt = runtime(s);
    let hidden = has_key(&rt, crate::views::send::template::SAMPLE_KEY);

    press(&mut rt, crate::views::send::template::SWITCH_KEY);
    let shown = has_key(&rt, crate::views::send::template::SAMPLE_KEY);
    press(&mut rt, crate::views::send::template::SWITCH_KEY);

    assert!(!hidden && shown);
    assert!(!row_visible(&rt, crate::views::send::template::SAMPLE_KEY));
}

#[test]
fn the_caption_row_keeps_its_height_when_the_switch_turns_on() {
    let (s, _) = send_state();
    let mut rt = runtime(s);
    let off = rect(&rt, "send-body-caption")[3];

    press(&mut rt, crate::views::send::template::SWITCH_KEY);
    let on = rect(&rt, "send-body-caption")[3];

    assert_eq!(off, on);
}

#[test]
fn the_placeholder_list_opens_and_mod_enter_closes_it() {
    let (s, _) = template_state("{{person.email}}");
    let mut rt = send_runtime(s, (1280.0, 800.0));

    press(&mut rt, crate::views::send::template::PLACEHOLDERS_KEY);
    let listed = has_text(&rt, "person.first-name");
    command(&mut rt, Key::Enter);

    assert!(listed);
    assert!(!rt.state.send.composer.placeholders_open);
}

#[test]
fn the_placeholder_list_stays_inside_the_smallest_window() {
    let (mut s, _) = template_state("{{person.email}}");
    s.send.composer.placeholders_open = true;

    let rt = runtime_at(s, WINDOW_MIN);

    let [_, y, _, height] = rect(&rt, crate::views::send::template::PLACEHOLDER_LIST_KEY);
    assert!(
        y >= 0.0 && y + height <= WINDOW_MIN.1,
        "list at {y}+{height}"
    );
}

#[test]
fn an_invalid_template_shows_its_reason_muted_before_any_press() {
    let (s, _) = template_state("{{person.fullname}}");

    let rt = runtime(s);

    assert!(has_text(
        &rt,
        "Line 1, column 1: unknown type 'person.fullname'"
    ));
    assert!(!has_text(&rt, "Fix 1 field to send"));
}

#[test]
fn a_template_issue_at_the_smallest_size_keeps_eight_editor_lines_and_send_on_screen() {
    let (mut s, _) = template_state("{{person.fullname}}");
    s.apply(Input::Send(crate::send::SendInput::SendPressed));

    let rt = runtime_at(s, WINDOW_MIN);

    let editor = rect(&rt, crate::views::send::BODY_KEY)[3];
    let [_, y, _, height] = rect(&rt, crate::views::send::SEND_BUTTON_KEY);
    assert!(has_text(&rt, "Fix 1 field to send"));
    assert!(
        editor >= 8.0 * crate::design::tokens::TYPE_MONO.line_px(),
        "editor {editor}"
    );
    assert!(
        y >= 0.0 && y + height <= WINDOW_MIN.1,
        "Send at {y}+{height}"
    );
}

#[rstest::rstest]
#[case::render_fails(
    "{{internet.password length=1048576}}x",
    "This sample failed: the rendered body passes the 1048576-byte limit"
)]
#[case::cut("{{internet.password length=20000}}", "Sample cut at 16 KB")]
#[case::over_cap(
    &"x".repeat(crate::send::compose::BODY_CAP + 1),
    "No sample for bodies over 1 MB"
)]
#[case::no_placeholders("plain", "The template has no placeholders; every event is the same")]
#[case::inert_locale(
    "{{random.integer locale=pt-br}}",
    "Line 1, column 1: locale=pt-br has no effect on random.integer"
)]
fn the_sample_block_says_what_it_shows(#[case] body: &str, #[case] line: &str) {
    let (s, _) = template_state(body);

    let rt = runtime(s);

    assert!(has_text(&rt, line), "missing {line:?}");
}

#[test]
fn regenerate_draws_a_new_sample() {
    let (s, _) = template_state("{{internet.password}}");
    let mut rt = runtime(s);

    press(&mut rt, crate::views::send::template::REGENERATE_KEY);

    assert!(matches!(
        rt.state.send.composer.template(),
        crate::send::template::TemplateState::Ready { generation: 1, .. }
    ));
}

#[test]
fn the_edit_button_opens_the_target_only_sheet() {
    let (s, _) = send_state();
    let mut rt = runtime(s);

    press(&mut rt, crate::views::send::sidebar::EDIT_TARGET_KEY);

    assert_eq!(rt.state.sheet, Some(crate::state::SheetPurpose::SendTarget));
    assert!(has_key(&rt, "sheet-done") && !has_key(&rt, "sheet-connect"));
}

#[test]
fn selecting_a_result_opens_the_export_inspector() {
    let (s, _) = export_results(3, false);
    let mut rt = runtime(s);

    press(&mut rt, "p0-s1");

    assert!(has_key(&rt, "inspector"));
    assert_eq!(
        rt.state
            .archive
            .inspector
            .selected
            .as_ref()
            .unwrap()
            .key_str,
        "p0-s1"
    );
}

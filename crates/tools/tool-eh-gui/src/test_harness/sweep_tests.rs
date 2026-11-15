use super::builders::{ORDERS, export_state, send_state, state};
use super::journeys::orders;
use super::services::ServiceCall;
use super::worst_cases::{WORST, long_connect_failure};
use super::{Harness, a11y_node, a11y_nodes, lists};
use crate::archive::ArchiveInput;
use crate::archive::results::Results;
use crate::archive::test_support::{messages, page};
use crate::design::tokens::{INSPECTOR_MIN, SIDEBAR_MIN, WINDOW_MIN, WINDOW_SIZE};
use crate::form::{AuthChoice, CredentialChoice};
use crate::read::test_support::captured;
use crate::send::SendInput;
use crate::state::{AppState, Input, SheetPurpose};
use crate::views::SEND_BUTTON_KEY;
use crate::views::details::{COPY_KEY, DONE_KEY, FAILURE_KEY};
use rune::Runtime;
use rune::accesskit::Action;
use serde_json::Value;
use shared_eventhub_core::producer::{SendEnd, SendSummary};
use shared_eventhub_persistence::export::ExportOutcome;
use std::time::Duration;

const INTERACTIVE: [&str; 12] = [
    "button",
    "default-button",
    "switch",
    "radio-button",
    "check-box",
    "splitter",
    "list-item",
    "text-input",
    "multiline-text-input",
    "password-input",
    "search-input",
    "combo-box",
];
pub(super) const TOOLBAR: [&str; 3] = ["toolbar/segment0", "toolbar/segment1", "toolbar/segment2"];

/// One state of the window: what a pointer can reach on its top layer, what shows disabled,
/// and what an overlay covers.
pub(super) struct Row {
    pub(super) name: &'static str,
    pub(super) build: fn((f32, f32)) -> Harness,
    pub(super) reachable: &'static [&'static str],
    pub(super) disabled: &'static [&'static str],
    pub(super) covered: &'static [&'static str],
}

pub(super) fn read_disconnected(size: (f32, f32)) -> Harness {
    Harness::new(state(), size)
}

pub(super) fn orders_form() -> AppState {
    let mut s = state();
    s.form.connection_string.edit_mut().replace_text(ORDERS);
    s
}

fn read_preparing(size: (f32, f32)) -> Harness {
    let mut h = Harness::new(orders_form(), size);
    h.deliver(Input::ConnectPressed);
    h
}

pub(super) fn read_selected(size: (f32, f32)) -> Harness {
    let mut h = Harness::new(orders_form(), size);
    let (session, _) = h.start_session(4);
    let rows = (0..40)
        .map(|sequence| captured("0", sequence, &format!("order {sequence}")))
        .collect();
    h.capture(session, rows, None);
    h.deliver(Input::Read(crate::read::ReadInput::Select(
        "p0-s39".to_owned(),
    )));
    h
}

fn read_failed(size: (f32, f32)) -> Harness {
    failed_with(size, "the archive is in use by another eh")
}

pub(super) fn failed_with(size: (f32, f32), text: &str) -> Harness {
    let mut h = read_preparing(size);
    let attempt = match &h.state().connection {
        crate::state::ConnectionState::Connecting { attempt, .. } => *attempt,
        other => panic!("not preparing: {other:?}"),
    };
    h.deliver(Input::SessionReady {
        attempt,
        result: Err(text.to_owned()),
    });
    h
}

pub(super) fn export_not_searched(size: (f32, f32)) -> Harness {
    Harness::new(export_state(Results::NotSearched), size)
}

fn export_searching(size: (f32, f32)) -> Harness {
    let mut h = export_not_searched(size);
    h.press("archive-search-button");
    h
}

/// The newest recorded call `pick` accepts, taken out of the log.
pub(super) fn taken<T>(h: &mut Harness, pick: impl Fn(&ServiceCall) -> Option<T>) -> T {
    let calls = h.take_calls();
    let Some(found) = calls.iter().rev().find_map(&pick) else {
        panic!("no such call in {calls:?}");
    };
    found
}

fn export_results(size: (f32, f32)) -> Harness {
    let mut h = export_searching(size);
    let generation = taken(&mut h, |call| match call {
        ServiceCall::SearchArchive(request) => Some(request.generation),
        _ => None,
    });
    h.deliver(Input::Archive(ArchiveInput::PageLoaded {
        generation,
        elapsed_ms: 1,
        result: Ok(page(messages(3), None)),
    }));
    h
}

pub(super) fn export_bulk_running(size: (f32, f32)) -> Harness {
    let mut h = export_results(size);
    h.press("bulk-export");
    h
}

fn export_bulk_done(size: (f32, f32)) -> Harness {
    let mut h = export_bulk_running(size);
    let export = taken(&mut h, |call| match call {
        ServiceCall::BulkExport(request) => Some(request.export),
        _ => None,
    });
    h.deliver(Input::Archive(ArchiveInput::BulkDone {
        export,
        elapsed_ms: 1,
        result: Ok(ExportOutcome {
            exported: 3,
            ..ExportOutcome::default()
        }),
    }));
    h
}

pub(super) fn send_idle(size: (f32, f32)) -> Harness {
    Harness::new(send_state().0, size)
}

fn send_issues(size: (f32, f32)) -> Harness {
    let (mut s, _) = send_state();
    s.send.composer.count.replace_text("0");
    let mut h = Harness::new(s, size);
    h.press(SEND_BUTTON_KEY);
    h
}

fn send_preparing(size: (f32, f32)) -> Harness {
    let mut h = send_idle(size);
    h.press(SEND_BUTTON_KEY);
    h
}

fn send_sending(size: (f32, f32)) -> Harness {
    send_sending_run(size).0
}

pub(super) fn send_sending_run(size: (f32, f32)) -> (Harness, u64) {
    let mut h = send_preparing(size);
    let run = taken(&mut h, |call| match call {
        ServiceCall::StartSend(request) => Some(request.run),
        _ => None,
    });
    let at = h.now();
    h.deliver(Input::Send(SendInput::Started {
        run,
        target: orders(),
        at,
    }));
    h.deliver(Input::Send(SendInput::Progress { run, sent: 1 }));
    h.tick();
    (h, run)
}

pub(super) fn send_done(size: (f32, f32), end: SendEnd) -> Harness {
    let (mut h, run) = send_sending_run(size);
    h.deliver(Input::Send(SendInput::Finished {
        run,
        summary: SendSummary {
            sent: 1,
            batches: 1,
            elapsed: Duration::from_millis(5),
            end,
        },
        close_error: None,
    }));
    h
}

fn send_completed(size: (f32, f32)) -> Harness {
    send_done(size, SendEnd::Completed)
}

fn send_failed(size: (f32, f32)) -> Harness {
    send_done(
        size,
        SendEnd::Failed("the hub refused the batch".to_owned()),
    )
}

fn template(size: (f32, f32), body: &str) -> Harness {
    let (mut s, _) = send_state();
    s.send.composer.body.replace_text(body);
    s.send.composer.set_template(true);
    Harness::new(s, size)
}

fn send_template_ready(size: (f32, f32)) -> Harness {
    template(size, "{\"id\": {{random.integer min=1 max=9}}}")
}

pub(super) fn send_template_invalid(size: (f32, f32)) -> Harness {
    template(size, "{{person.fullname}}")
}

fn send_placeholders_open(size: (f32, f32)) -> Harness {
    let mut h = send_template_ready(size);
    h.press(crate::views::PLACEHOLDERS_KEY);
    h
}

pub(super) fn sheet(size: (f32, f32), auth: AuthChoice) -> Harness {
    let mut s = state();
    s.form.set_auth(auth);
    s.form.credential = CredentialChoice::ClientSecret;
    s.sheet = Some(SheetPurpose::Connect);
    Harness::new(s, size)
}

fn sheet_connection_string(size: (f32, f32)) -> Harness {
    sheet(size, AuthChoice::ConnectionString)
}

fn sheet_credential(size: (f32, f32)) -> Harness {
    sheet(size, AuthChoice::Credential)
}

fn sheet_key_vault(size: (f32, f32)) -> Harness {
    sheet(size, AuthChoice::KeyVault)
}

pub(super) fn sheet_send_target(size: (f32, f32)) -> Harness {
    let (mut s, _) = send_state();
    s.sheet = Some(SheetPurpose::SendTarget);
    Harness::new(s, size)
}

fn pill_open(size: (f32, f32)) -> Harness {
    let mut h = read_disconnected(size);
    h.press("pill");
    h
}

const ROWS: &[Row] = &[
    Row {
        name: "read disconnected",
        build: read_disconnected,
        reachable: &[
            "empty-connect",
            "connect",
            "pill",
            TOOLBAR[0],
            TOOLBAR[1],
            TOOLBAR[2],
        ],
        disabled: &[],
        covered: &[],
    },
    Row {
        name: "read preparing",
        build: read_preparing,
        reachable: &["disconnect"],
        disabled: &["apply-rules"],
        covered: &[],
    },
    Row {
        name: "read connected with a selection",
        build: read_selected,
        reachable: &["disconnect", "inspector-close", "export-message", "p0-s39"],
        disabled: &["apply-rules"],
        covered: &[],
    },
    Row {
        name: "read failed",
        build: read_failed,
        reachable: &["empty-connect", "connect"],
        disabled: &[],
        covered: &[],
    },
    Row {
        name: "export not searched",
        build: export_not_searched,
        reachable: &["archive-search-button", TOOLBAR[1]],
        disabled: &[],
        covered: &[],
    },
    Row {
        name: "export searching",
        build: export_searching,
        reachable: &[],
        disabled: &["archive-search-button"],
        covered: &[],
    },
    Row {
        name: "export results",
        build: export_results,
        reachable: &[
            "archive-search-button",
            "bulk-export",
            "bulk-options",
            "p0-s0",
        ],
        disabled: &[],
        covered: &[],
    },
    Row {
        name: "export bulk running",
        build: export_bulk_running,
        reachable: &["bulk-cancel"],
        disabled: &[],
        covered: &[],
    },
    Row {
        name: "export bulk done",
        build: export_bulk_done,
        reachable: &["bulk-done"],
        disabled: &[],
        covered: &[],
    },
    Row {
        name: "send idle",
        build: send_idle,
        reachable: &[SEND_BUTTON_KEY, "send-edit-target", "connect", TOOLBAR[2]],
        disabled: &[],
        covered: &[],
    },
    Row {
        name: "send with issues",
        build: send_issues,
        reachable: &[SEND_BUTTON_KEY],
        disabled: &[],
        covered: &[],
    },
    Row {
        name: "send preparing",
        build: send_preparing,
        reachable: &["send-cancel"],
        disabled: &[SEND_BUTTON_KEY, "connect"],
        covered: &[],
    },
    Row {
        name: "send sending",
        build: send_sending,
        reachable: &["send-cancel"],
        disabled: &[SEND_BUTTON_KEY, "connect"],
        covered: &[],
    },
    Row {
        name: "send completed",
        build: send_completed,
        reachable: &[SEND_BUTTON_KEY],
        disabled: &[],
        covered: &[],
    },
    Row {
        name: "send failed",
        build: send_failed,
        reachable: &[SEND_BUTTON_KEY],
        disabled: &[],
        covered: &[],
    },
    Row {
        name: "send template ready",
        build: send_template_ready,
        reachable: &[
            SEND_BUTTON_KEY,
            crate::views::SWITCH_KEY,
            crate::views::PLACEHOLDERS_KEY,
            crate::views::REGENERATE_KEY,
        ],
        disabled: &[],
        covered: &[],
    },
    Row {
        name: "send template invalid",
        build: send_template_invalid,
        reachable: &[SEND_BUTTON_KEY, crate::views::SWITCH_KEY],
        disabled: &[],
        covered: &[],
    },
    Row {
        name: "send placeholders open",
        build: send_placeholders_open,
        reachable: &[],
        disabled: &[],
        covered: &[SEND_BUTTON_KEY],
    },
    Row {
        name: "sheet, connection string",
        build: sheet_connection_string,
        reachable: &["sheet-connect", "save-config", "cancel"],
        disabled: &[],
        covered: &["empty-connect", "connect"],
    },
    Row {
        name: "sheet, credential",
        build: sheet_credential,
        reachable: &["sheet-connect", "save-config", "cancel"],
        disabled: &[],
        covered: &["empty-connect", "connect"],
    },
    Row {
        name: "sheet, key vault",
        build: sheet_key_vault,
        reachable: &["sheet-connect", "save-config", "cancel"],
        disabled: &[],
        covered: &["empty-connect", "connect"],
    },
    Row {
        name: "sheet, send target",
        build: sheet_send_target,
        reachable: &["sheet-done", "cancel"],
        disabled: &[],
        covered: &[SEND_BUTTON_KEY],
    },
    Row {
        name: "pill popover",
        build: pill_open,
        reachable: &[],
        disabled: &[],
        covered: &["empty-connect"],
    },
    Row {
        name: "error details open",
        build: details_open,
        reachable: &[COPY_KEY, DONE_KEY],
        disabled: &[],
        covered: &["empty-connect", "connect"],
    },
];

fn details_open(size: (f32, f32)) -> Harness {
    let mut h = long_connect_failure(size);
    h.press(FAILURE_KEY);
    h
}

/// The element `spec` names: a key, or `<path part>/<key>` for a key that repeats.
fn element(rt: &Runtime<AppState>, spec: &str) -> Option<rune::ElementInfo> {
    let (within, key) = match spec.rsplit_once('/') {
        Some((within, key)) => (Some(format!("{within}/")), key),
        None => (None, spec),
    };
    rt.elements().into_iter().find(|element| {
        !element.exiting
            && element.key.as_deref() == Some(key)
            && within
                .as_deref()
                .is_none_or(|within| element.path.contains(within))
    })
}

/// The accessibility node of `spec` (its key part).
fn node<'a>(tree: &'a Value, spec: &str) -> Option<&'a Value> {
    let key = spec.rsplit_once('/').map_or(spec, |(_, key)| key);
    a11y_node(tree, key)
}

fn is_inside(spec: &str, rt: &Runtime<AppState>) -> bool {
    let Some(target) = element(rt, spec) else {
        return false;
    };
    let [x, y, w, h] = target.rect;
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    rt.element_at(cx, cy).is_some_and(|picked| {
        let path = picked.path();
        path == target.path
            || path
                .strip_prefix(target.path.as_str())
                .is_some_and(|rest| rest.starts_with('/'))
    })
}

fn on_screen(spec: &str, rt: &Runtime<AppState>, size: (f32, f32)) -> bool {
    element(rt, spec).is_some_and(|target| {
        let [x, y, w, h] = target.rect;
        x >= -0.5 && y >= -0.5 && x + w <= size.0 + 0.5 && y + h <= size.1 + 0.5
    })
}

fn accessibility_problems(row: &Row, h: &mut Harness) -> Vec<String> {
    let tree = h.a11y();
    let mut problems = Vec::new();
    for node in a11y_nodes(&tree) {
        let role = node["role"].as_str().unwrap_or_default();
        let named = node["name"]
            .as_str()
            .is_some_and(|name| !name.trim().is_empty());
        if INTERACTIVE.contains(&role) && !named {
            problems.push(format!("unnamed {role} keyed {:?}", node["key"]));
        }
    }
    let mut rows: Vec<&str> = a11y_nodes(&tree)
        .into_iter()
        .filter(|node| node["role"] == "list-item")
        .filter_map(|node| node["name"].as_str())
        .collect();
    let total = rows.len();
    rows.sort_unstable();
    rows.dedup();
    if rows.len() != total {
        problems.push("message rows share a name".to_owned());
    }
    for spec in row.reachable {
        match node(&tree, spec) {
            Some(node) if lists(node, "actions", "click") && !lists(node, "states", "disabled") => {
            }
            Some(node) => problems.push(format!("{spec} is not clickable: {node}")),
            None => problems.push(format!("{spec} is not in the tree")),
        }
    }
    for spec in row.disabled {
        match node(&tree, spec) {
            Some(node) if lists(node, "states", "disabled") && !lists(node, "actions", "click") => {
            }
            Some(node) => problems.push(format!("{spec} does not read as disabled: {node}")),
            None => problems.push(format!("{spec} is not in the tree")),
        }
    }
    if h.state().sheet.is_some() || h.state().details.is_some() {
        for hidden in ["content", "pill", "send-content", "read-rows"] {
            if a11y_node(&tree, hidden).is_some() {
                problems.push(format!("{hidden} is in the tree behind the modal"));
            }
        }
    }
    problems
}

fn layout_problems(row: &Row, h: &mut Harness, size: (f32, f32)) -> Vec<String> {
    let mut problems = Vec::new();
    let shown = row.reachable.iter().map(|spec| (spec, true));
    for (spec, pressable) in shown.chain(row.disabled.iter().map(|spec| (spec, false))) {
        let key = spec.rsplit_once('/').map_or(*spec, |(_, key)| key);
        h.rt.accessibility_action(key, Action::ScrollIntoView, None);
        h.settle();
        if !on_screen(spec, &h.rt, size) {
            problems.push(format!("{spec} is not inside the window"));
        } else if pressable && !is_inside(spec, &h.rt) {
            problems.push(format!("something covers {spec}"));
        }
    }
    for spec in row.covered {
        if element(&h.rt, spec).is_none() {
            problems.push(format!("{spec} is not mounted under the overlay"));
        } else if is_inside(spec, &h.rt) {
            problems.push(format!("{spec} is reachable under the overlay"));
        }
    }
    if let Some(content) = element(&h.rt, "content") {
        let [x, _, w, _] = content.rect;
        let sidebar_open = h.state().sidebar.is_open();
        if sidebar_open && x < SIDEBAR_MIN - 0.5 {
            problems.push(format!("the sidebar is {x} wide"));
        }
        if w <= 0.0 {
            problems.push(format!("the content is {w} wide"));
        }
        let inspector_open = h.has_key("inspector");
        if inspector_open && size.0 - (x + w) < INSPECTOR_MIN - 0.5 {
            problems.push(format!("the inspector is {} wide", size.0 - (x + w)));
        }
    }
    problems
}

/// The tables run in this many shards, so the test runner spreads them over its threads; the
/// `#[values]` lists below name each shard once.
const SHARDS: usize = 4;

/// Every `SHARDS`-th row of `rows`, from `shard`.
fn shard<'a>(rows: impl Iterator<Item = &'a Row>, shard: usize) -> impl Iterator<Item = &'a Row> {
    rows.skip(shard).step_by(SHARDS)
}

#[rstest::rstest]
fn every_state_names_its_controls_and_marks_what_it_disables(#[values(0, 1, 2, 3)] part: usize) {
    let mut problems = Vec::new();
    for row in shard(ROWS.iter(), part) {
        let mut h = (row.build)(WINDOW_SIZE);
        for problem in accessibility_problems(row, &mut h) {
            problems.push(format!("{}: {problem}", row.name));
        }
    }

    assert!(problems.is_empty(), "{problems:#?}");
}

#[rstest::rstest]
#[case::smallest(WINDOW_MIN)]
#[case::default_size(WINDOW_SIZE)]
fn a_long_connect_failure_keeps_connect_reachable(#[case] size: (f32, f32)) {
    let row = Row {
        name: "a long connect failure",
        build: long_connect_failure,
        reachable: &["empty-connect", "connect", FAILURE_KEY],
        disabled: &[],
        covered: &[],
    };
    let mut h = (row.build)(size);

    let problems = layout_problems(&row, &mut h, size);

    assert!(problems.is_empty(), "{problems:#?}");
}

#[rstest::rstest]
#[case::smallest(WINDOW_MIN)]
#[case::default_size(WINDOW_SIZE)]
fn every_state_keeps_its_actions_on_screen_and_uncovered(
    #[case] size: (f32, f32),
    #[values(0, 1, 2, 3)] part: usize,
) {
    let mut problems = Vec::new();
    for row in shard(ROWS.iter().chain(WORST), part) {
        let mut h = (row.build)(size);
        for problem in layout_problems(row, &mut h, size) {
            problems.push(format!("{}: {problem}", row.name));
        }
    }

    assert!(problems.is_empty(), "{problems:#?}");
}

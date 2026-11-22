//! The Send sidebar: where the events go, how they are routed, and the message settings.

use super::{send_keys, type_newline};
use crate::app::dispatch;
use crate::design::parts::{self, EditorHeight, EditorSpec};
use crate::design::tokens::{PROPERTY_LINES, SIDEBAR_FIELD_WIDTH, SPACE_LG, SPACE_MD, SPACE_SM};
use crate::send::Destination;
use crate::send::compose::{PacingChoice, RoutingChoice};
use crate::send::run::SendRun;
use crate::state::{AppState, Input, SheetPurpose, error_preview};
use crate::views::INVALID_CONNECTION_STRING;
use rune::{El, KeyEvent, ScrollSpec, TextEdit, Tx, col, row};
use rune_kit::{Button, ButtonVariant, Kit, ScrollBar, Select, SelectAppearance, TextInput};

/// The Edit… (or Set up…) button's key.
pub const EDIT_TARGET_KEY: &str = "send-edit-target";
pub const PARTITION_KEY_KEY: &str = "send-partition-key";
pub const COUNT_KEY: &str = "send-count";
pub const INTERVAL_KEY: &str = "send-interval";
pub const CONTENT_TYPE_KEY: &str = "send-content-type";
pub const CORRELATION_ID_KEY: &str = "send-correlation-id";
/// The Properties editor's key.
pub const PROPERTIES_KEY: &str = "send-properties";
const PROPERTIES_HINT: &str = "One key=value per line";

const STORED_WORDS: &str = "the hub in the saved connection string";

type EditAccess = fn(&mut AppState) -> &mut TextEdit;

/// The words naming the hub the connection form resolves to, when it names one.
pub fn hub_words(s: &AppState) -> Option<String> {
    match &s.send.destination {
        Destination::Named(names) => Some(names.hub()),
        Destination::Stored => Some(STORED_WORDS.to_owned()),
        Destination::Missing | Destination::Invalid => None,
    }
}

pub fn sidebar(s: &AppState, k: &Kit, layer: &mut Vec<El>) -> El {
    let issues = s.send.shown_issues();
    let composer = &s.send.composer;
    let mut form = col()
        .key("send-form")
        .gap(SPACE_MD)
        .px(SPACE_LG)
        .pb(SPACE_LG)
        .child(parts::section_header(k, "Destination"))
        .child(destination(s, k));
    if let Some(issue) = &issues.target {
        form = form.child(parts::issue(k, issue));
    }
    form = form
        .child(parts::field_label(k, "Routing"))
        .child(routing_select(s, k, layer));
    if composer.routing == RoutingChoice::PartitionKey {
        form = form.child(parts::reveal(field(
            k,
            PARTITION_KEY_KEY,
            "Partition key",
            &composer.partition_key,
            &issues.partition_key,
            |s| &mut s.send.composer.partition_key,
        )));
    }
    form = form
        .child(parts::section_header(k, "Message"))
        .child(field(
            k,
            CONTENT_TYPE_KEY,
            "Content type",
            &composer.content_type,
            &None,
            |s| &mut s.send.composer.content_type,
        ))
        .child(field(
            k,
            COUNT_KEY,
            "Count",
            &composer.count,
            &issues.count,
            |s| &mut s.send.composer.count,
        ))
        .child(parts::field_label(k, "Pacing"))
        .child(pacing_select(s, k, layer));
    if composer.pacing == PacingChoice::Paced {
        form = form.child(parts::reveal(
            col()
                .key("send-paced")
                .gap(SPACE_SM)
                .w_full()
                .child(field(
                    k,
                    INTERVAL_KEY,
                    "Interval (ms)",
                    &composer.interval,
                    &issues.interval,
                    |s| &mut s.send.composer.interval,
                ))
                .child(parts::caption(k, "One event per interval, start to start")),
        ));
    }
    form = form
        .child(field(
            k,
            CORRELATION_ID_KEY,
            "Correlation id",
            &composer.correlation_id,
            &None,
            |s| &mut s.send.composer.correlation_id,
        ))
        .child(properties(k, &composer.properties, &issues.properties));
    col()
        .key("send-sidebar-scroll")
        .w_full()
        .h_full()
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(form)
        .scroll_bar(k)
}

/// The destination block: the hub the events go to, its Edit… button, and why it cannot
/// change now when it cannot.
fn destination(s: &AppState, k: &Kit) -> El {
    let running = s.send.run.is_running();
    let session = s.connection.is_active();
    let shown = if let Some(names) = running_names(s) {
        names
    } else {
        match &s.send.destination {
            Destination::Named(names) => names.display(),
            Destination::Stored => STORED_WORDS.to_owned(),
            Destination::Invalid => INVALID_CONNECTION_STRING.to_owned(),
            Destination::Missing => "No event hub yet".to_owned(),
        }
    };
    let label = match s.send.destination {
        Destination::Missing => "Set up…",
        Destination::Named(_) | Destination::Invalid | Destination::Stored => "Edit…",
    };
    let edit = Button::new(label)
        .variant(ButtonVariant::Outline)
        .disabled(running || session)
        .on_press(|s: &mut AppState| dispatch(s, Input::OpenSheet(SheetPurpose::SendTarget)))
        .build(k)
        .key(EDIT_TARGET_KEY);
    let mut block = col().key("send-destination").gap(SPACE_SM).w_full().child(
        row()
            .gap(SPACE_SM)
            .w_full()
            .items_end()
            .child(
                parts::detail_row(k, "Event hub", &shown)
                    .grow(1.0)
                    .min_w(0.0),
            )
            .child(edit),
    );
    if running {
        block = block.child(parts::caption(
            k,
            "Wait for the send to finish to change the hub.",
        ));
    } else if session {
        block = block.child(parts::caption(
            k,
            "Sending goes to the hub you're reading. Disconnect to change it.",
        ));
    }
    block
}

/// The names a running run sends to, frozen at its press or resolved by its worker.
fn running_names(s: &AppState) -> Option<String> {
    match &s.send.run {
        SendRun::Preparing { target, .. } => Some(
            target
                .as_ref()
                .map_or_else(|| STORED_WORDS.to_owned(), |t| t.display()),
        ),
        SendRun::Sending { target, .. } => Some(target.display()),
        SendRun::Idle | SendRun::Done { .. } => None,
    }
}

fn routing_select(s: &AppState, k: &Kit, layer: &mut Vec<El>) -> El {
    let labels = RoutingChoice::ALL.map(RoutingChoice::label);
    let composer = &s.send.composer;
    Select::new("send-routing", &labels)
        .label("Routing")
        .appearance(SelectAppearance::Field)
        .width(SIDEBAR_FIELD_WIDTH)
        .current(Some(composer.routing.label()))
        .open(composer.routing_open)
        .on_toggle(|s: &mut AppState| {
            let composer = &mut s.send.composer;
            composer.routing_open = !composer.routing_open;
        })
        .on_close(|s: &mut AppState| s.send.composer.routing_open = false)
        .on_choose(|s: &mut AppState, label: &str, _| {
            let composer = &mut s.send.composer;
            if let Some(choice) = RoutingChoice::from_label(label) {
                composer.routing = choice;
            }
            composer.routing_open = false;
        })
        .build(k, layer)
}

fn pacing_select(s: &AppState, k: &Kit, layer: &mut Vec<El>) -> El {
    let labels = PacingChoice::ALL.map(PacingChoice::label);
    let composer = &s.send.composer;
    Select::new("send-pacing", &labels)
        .label("Pacing")
        .appearance(SelectAppearance::Field)
        .width(SIDEBAR_FIELD_WIDTH)
        .current(Some(composer.pacing.label()))
        .open(composer.pacing_open)
        .on_toggle(|s: &mut AppState| {
            let composer = &mut s.send.composer;
            composer.pacing_open = !composer.pacing_open;
        })
        .on_close(|s: &mut AppState| s.send.composer.pacing_open = false)
        .on_choose(|s: &mut AppState, label: &str, _| {
            let composer = &mut s.send.composer;
            if let Some(choice) = PacingChoice::from_label(label) {
                composer.pacing = choice;
            }
            composer.pacing_open = false;
        })
        .build(k, layer)
}

/// The Properties field: its label, the fixed-height lines editor, its issue cut to a short
/// line, and the syntax hint.
fn properties(k: &Kit, edit: &TextEdit, issue: &Option<String>) -> El {
    let spec = EditorSpec {
        key: PROPERTIES_KEY,
        label: "Properties",
        placeholder: "key=value",
        description: Some(PROPERTIES_HINT),
        invalid: issue.is_some(),
        height: EditorHeight::Fixed {
            lines: PROPERTY_LINES,
        },
    };
    let mut group = col()
        .key(format!("{PROPERTIES_KEY}-group"))
        .gap(SPACE_SM)
        .w_full()
        .child(parts::field_label(k, "Properties"))
        .child(parts::lines_editor(
            k,
            spec,
            edit,
            |s: &mut AppState| &mut s.send.composer.properties,
            properties_keys,
        ));
    if let Some(issue) = issue {
        group = group.child(parts::issue(k, &error_preview(issue).shown));
    }
    group.child(parts::caption(k, PROPERTIES_HINT))
}

/// The Properties editor's keys: the Send shortcuts, and Return typing a newline.
fn properties_keys(s: &mut AppState, key: &KeyEvent, tx: &mut Tx) -> bool {
    send_keys(s, key, tx) || type_newline(key, &mut s.send.composer.properties)
}

/// A labelled single-line field whose key handler takes the Send shortcuts, with its issue
/// under it.
fn field(
    k: &Kit,
    key: &str,
    label: &str,
    edit: &TextEdit,
    issue: &Option<String>,
    access: EditAccess,
) -> El {
    let mut group = col()
        .key(format!("{key}-group"))
        .gap(SPACE_SM)
        .w_full()
        .child(parts::field_label(k, label))
        .child(
            TextInput::new(edit)
                .label(label)
                .width(None)
                .invalid(issue.is_some())
                .on_edit(access)
                .on_key(send_keys)
                .build(k)
                .key(key),
        );
    if let Some(issue) = issue {
        group = group.child(parts::issue(k, issue));
    }
    group
}

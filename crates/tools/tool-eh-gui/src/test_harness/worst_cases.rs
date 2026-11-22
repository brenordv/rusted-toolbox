//! The layout sweep's worst cases: the largest inputs each surface takes.

use super::Harness;
use super::builders::{export_state, send_state, state, test_workspace};
use super::journeys::open_listed_sheet;
use super::profile_tests::profiles_sheet;
use super::services::ServiceCall;
use super::sweep_tests::{
    Row, TOOLBAR, cut_dangers, export_bulk_running, export_selected, failed_with, orders_form,
    read_selected, send_done, send_template_invalid, taken,
};
use crate::archive::ArchiveInput;
use crate::archive::results::Results;
use crate::bridge::UiDelivery;
use crate::design::tokens::{INSPECTOR_MAX, SIDEBAR_MAX};
use crate::send::compose::BODY_CAP;
use crate::state::{Input, ToastInput};
use crate::views::{EXPORT_METADATA_KEY, NEW_PROFILE_KEY, READ_METADATA_KEY, SEND_BUTTON_KEY};
use shared_eventhub_core::engine::EngineEvent;
use shared_eventhub_core::producer::SendEnd;
use shared_eventhub_persistence::export::ExportOutcome;
use std::path::PathBuf;

/// Words that wrap: about 2,000 characters.
pub(super) fn long_text() -> String {
    "the hub refused the batch because the namespace is throttling this client; ".repeat(27)
}

fn huge_body(size: (f32, f32)) -> Harness {
    let (mut s, _) = send_state();
    s.send.composer.body.replace_text("x".repeat(BODY_CAP));
    Harness::new(s, size)
}

fn template_issue_pressed(size: (f32, f32)) -> Harness {
    let mut h = send_template_invalid(size);
    h.press(SEND_BUTTON_KEY);
    h
}

fn long_send_failure(size: (f32, f32)) -> Harness {
    send_done(size, SendEnd::Failed(long_text()))
}

/// A connect that failed with about 2,000 characters of error text.
pub(super) fn long_connect_failure(size: (f32, f32)) -> Harness {
    failed_with(size, &long_text())
}

/// A bulk export that ended with one failed file and a long first error.
pub(super) fn long_bulk_failure(size: (f32, f32)) -> Harness {
    let mut h = export_bulk_running(size);
    let export = taken(&mut h, |call| match call {
        ServiceCall::BulkExport(request) => Some(request.export),
        _ => None,
    });
    h.deliver(Input::Archive(ArchiveInput::BulkDone {
        export,
        elapsed_ms: 1,
        result: Ok(ExportOutcome {
            exported: 2,
            failed: 1,
            first_error: Some(long_text()),
            ..ExportOutcome::default()
        }),
    }));
    h
}

/// The Export section with an archive that could not be searched, for a long reason.
pub(super) fn long_archive_error(size: (f32, f32)) -> Harness {
    Harness::new(
        export_state(Results::Unavailable {
            generation: 1,
            error: long_text(),
        }),
        size,
    )
}

fn many_partitions_with_warnings(size: (f32, f32)) -> Harness {
    let mut h = Harness::new(orders_form(), size);
    let (session, _) = h.start_session(32);
    let at = h.now();
    let errors = (0..3)
        .map(|partition| EngineEvent::Error {
            partition_id: Some(partition.to_string()),
            error: "link detached".to_owned(),
            fatal: false,
        })
        .collect();
    h.deliver(Input::Delivery(UiDelivery {
        at,
        session,
        events: errors,
        stats: None,
    }));
    h
}

fn widest_panes(size: (f32, f32)) -> Harness {
    let mut h = read_selected(size);
    h.rt.state.sidebar.set_width(SIDEBAR_MAX);
    h.rt.state.read.inspector.pane.set_width(INSPECTOR_MAX);
    h.rt.invalidate();
    h.settle();
    h
}

fn widest_panes_on_export(size: (f32, f32)) -> Harness {
    let mut h = export_selected(size);
    h.rt.state.sidebar.set_width(SIDEBAR_MAX);
    h.rt.state.archive.inspector.pane.set_width(INSPECTOR_MAX);
    h.rt.invalidate();
    h.settle();
    h
}

/// A launch folder about 300 characters deep, relative so it reads the same on every machine.
fn deep_folder() -> PathBuf {
    std::iter::repeat_n("a-checkout-folder-nested-under-another-one", 7).collect()
}

/// The listed connect sheet of a window launched in [`deep_folder`]: Save as profile shares the
/// Saves to row with the long path, and Change shares the data folder row with a second one.
fn sheet_in_a_deep_folder(size: (f32, f32)) -> Harness {
    let mut s = state();
    s.set_launch(deep_folder(), Ok(test_workspace()));
    let mut h = Harness::new(s, size);
    open_listed_sheet(&mut h);
    h
}

/// The listed connect sheet after a Save config that failed with about 2,000 characters of
/// error text, so the footer's save status is as long as a preview gets. The failure's toast is
/// closed: at the smallest window it lies over the footer's buttons.
fn long_save_failure(size: (f32, f32)) -> Harness {
    let mut h = profiles_sheet(size);
    h.deliver(Input::SavePressed);
    h.deliver(Input::ConfigSaved(Err(long_text())));
    h.press("toast-1-close");
    h
}

/// Four cut danger toasts at once (three stay), fanned out.
fn four_toasts_fanned(size: (f32, f32)) -> Harness {
    let mut h = cut_dangers(size, 4);
    h.deliver(Input::Toast(ToastInput::Hotkey));
    h
}

pub(super) const WORST: &[Row] = &[
    Row {
        name: "four cut danger toasts at once, fanned out",
        build: four_toasts_fanned,
        reachable: &[
            "toast-2-details",
            "toast-3-details",
            "toast-4-details",
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
        name: "a 1 MiB body",
        build: huge_body,
        reachable: &[SEND_BUTTON_KEY],
        disabled: &[],
        covered: &[],
    },
    Row {
        name: "a template issue after a press",
        build: template_issue_pressed,
        reachable: &[SEND_BUTTON_KEY],
        disabled: &[],
        covered: &[],
    },
    Row {
        name: "a long send failure",
        build: long_send_failure,
        reachable: &[SEND_BUTTON_KEY, "send-details"],
        disabled: &[],
        covered: &[],
    },
    Row {
        name: "a long bulk export failure",
        build: long_bulk_failure,
        reachable: &["bulk-done", "bulk-details"],
        disabled: &[],
        covered: &[],
    },
    Row {
        name: "a long archive error",
        build: long_archive_error,
        reachable: &["archive-error-details", TOOLBAR[1]],
        disabled: &[],
        covered: &[],
    },
    Row {
        name: "32 partitions and three warnings",
        build: many_partitions_with_warnings,
        reachable: &["disconnect", "pill", TOOLBAR[0], TOOLBAR[1], TOOLBAR[2]],
        disabled: &[],
        covered: &[],
    },
    Row {
        name: "both panes at their widest",
        build: widest_panes,
        reachable: &[
            "disconnect",
            "inspector-close",
            "export-message",
            READ_METADATA_KEY,
        ],
        disabled: &[],
        covered: &[],
    },
    Row {
        name: "the sheet in a deep launch folder",
        build: sheet_in_a_deep_folder,
        reachable: &[
            NEW_PROFILE_KEY,
            "change-folder",
            "sheet-connect",
            "save-config",
            "cancel",
        ],
        disabled: &[],
        covered: &["empty-connect", "connect"],
    },
    Row {
        name: "the sheet after a long save failure",
        build: long_save_failure,
        reachable: &["sheet-connect", "save-config", "cancel"],
        disabled: &[],
        covered: &["empty-connect", "connect"],
    },
    Row {
        name: "both panes at their widest on Export",
        build: widest_panes_on_export,
        reachable: &[
            "export-message",
            "inspector-close",
            "bulk-export",
            EXPORT_METADATA_KEY,
        ],
        disabled: &[],
        covered: &[],
    },
];

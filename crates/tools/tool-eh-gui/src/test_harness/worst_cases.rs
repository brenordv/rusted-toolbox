//! The layout sweep's worst cases: the largest inputs each surface takes.

use super::Harness;
use super::builders::send_state;
use super::sweep_tests::{
    Row, TOOLBAR, failed_with, orders_form, read_selected, send_done, send_template_invalid,
};
use crate::bridge::UiDelivery;
use crate::design::tokens::{INSPECTOR_MAX, SIDEBAR_MAX};
use crate::send::compose::BODY_CAP;
use crate::state::Input;
use crate::views::SEND_BUTTON_KEY;
use shared_eventhub_core::engine::EngineEvent;
use shared_eventhub_core::producer::SendEnd;

/// Words that wrap: about 2,000 characters.
fn long_text() -> String {
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

pub(super) const WORST: &[Row] = &[
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
        reachable: &[SEND_BUTTON_KEY],
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
        reachable: &["disconnect", "inspector-close", "export-message"],
        disabled: &[],
        covered: &[],
    },
];

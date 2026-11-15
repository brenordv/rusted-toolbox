//! The window frame: toolbar with the section switch and the connection controls, the sidebar,
//! the content area, and toasts.

use super::{details, export, inspector, sections, send};
use crate::app::dispatch;
use crate::design::tokens::{
    POPOVER_WIDTH, SPACE_LG, SPACE_MD, SPACE_SM, TYPE_CAPTION, TYPE_HEADLINE,
};
use crate::design::{Surface, icons, parts, surface};
use crate::read::{LogKind, LogLine};
use crate::state::{
    AppState, ConnectionState, Input, ListSection, Section, SheetPurpose, ToastKind, error_preview,
};
use common_cli::app_logger::FileLogging;
use rune::{Color, Cx, El, Tx, col, row, text};
use rune_kit::{
    Button, ButtonVariant, Kit, Placement, Popover, SegmentedControl, SplitView, Toast,
    ToastRegion, Toolbar,
};

pub fn shell(s: &AppState, cx: &Cx<AppState>, k: &Kit, dark: bool, layer: &mut Vec<El>) -> El {
    let sections = SegmentedControl::new(&Section::LABELS, Some(s.section.index()))
        .label("Section")
        .on_pick(|s: &mut AppState, index: usize, _| {
            if let Some(section) = Section::ALL.get(index) {
                dispatch(s, Input::SectionPicked(*section));
            }
        })
        .build(k);
    let trailing = row()
        .gap(SPACE_SM)
        .center_y()
        .pr(cx.chrome_insets().trailing)
        .child(pill(s, k, layer))
        .child(connection_button(s, k));
    // The section switch sits in the leading slot: the kit pins a centre slot across the whole
    // band, where the connection pill would draw over it.
    let leading = row()
        .gap(SPACE_LG)
        .center_y()
        .child(text("eh").font(TYPE_HEADLINE).color(k.c.text_base))
        .child(sections);
    let toolbar = Toolbar::new().leading(leading).trailing(trailing);

    let content = match s.section {
        Section::Read => sections::read(s, k, dark, layer).key("read"),
        Section::Export => export::content(s, k, dark, layer).key("export"),
        Section::Send => send::content(s, k, layer).key("send"),
    };
    let main = col()
        .key("content")
        .w_full()
        .h_full()
        .min_h(0.0)
        .fill(surface(k, Surface::Content))
        .child(content);

    let sidebar = sections::sidebar(s, k, layer);
    let split = SplitView::new(main)
        .sidebar(&s.sidebar, |s: &mut AppState| &mut s.sidebar, sidebar)
        .toolbar(toolbar)
        .titlebar_inset(cx.titlebar_inset())
        .reduced_motion(cx.is_reduced_motion());
    let split = match s.section.list() {
        Some(ListSection::Read) => split.inspector(
            &s.read.inspector.pane,
            |s: &mut AppState| &mut s.read.inspector.pane,
            inspector::inspector(s, k, ListSection::Read),
        ),
        Some(ListSection::Export) => split.inspector(
            &s.archive.inspector.pane,
            |s: &mut AppState| &mut s.archive.inspector.pane,
            inspector::inspector(s, k, ListSection::Export),
        ),
        None => split,
    };
    split.build(k)
}

/// The pill's dot color and label for each connection state; a connected session whose feed
/// stopped says so.
fn pill_status(k: &Kit, connection: &ConnectionState, feed_stale: bool) -> (Color, String) {
    match connection {
        ConnectionState::Disconnected => (k.c.icon_faint, "Not connected".to_owned()),
        ConnectionState::Connecting { session: None, .. } => {
            (k.c.warning.fg, "Preparing".to_owned())
        }
        ConnectionState::Connecting {
            session: Some(_), ..
        } => (k.c.warning.fg, "Connecting".to_owned()),
        ConnectionState::Connected { .. } if feed_stale => {
            (k.c.warning.fg, "Not receiving updates".to_owned())
        }
        ConnectionState::Connected { partitions, .. } => (
            k.c.success.fg,
            format!("Connected · {} partitions", partitions.len()),
        ),
        ConnectionState::Stopping { .. } => (k.c.warning.fg, "Finishing session".to_owned()),
        ConnectionState::Failed(_) => (k.c.danger.fg, "Failed".to_owned()),
    }
}

/// The connection pill; pressing it shows the data folder, log file, version, and any error.
fn pill(s: &AppState, k: &Kit, layer: &mut Vec<El>) -> El {
    let (dot, label) = pill_status(k, &s.connection, s.read.feed_stale);
    let mut face = parts::pill_face(k, dot, &label);
    if s.read.session_log_unseen > 0 {
        face = face.child(parts::warning_count(k, s.read.session_log_unseen));
    }
    let face = face
        .key("pill")
        .role("button")
        .label(format!("Connection: {label}"))
        .on_press(|s: &mut AppState| {
            s.pill_open = !s.pill_open;
            if s.pill_open {
                s.read.log_seen();
            }
        });

    let mut rows = col()
        .gap(SPACE_MD)
        .w(POPOVER_WIDTH)
        .child(parts::detail_row(k, "Data folder", &s.form.data_folder))
        .child(log_detail(k, &s.file_logging))
        .child(parts::detail_row(k, "Version", env!("CARGO_PKG_VERSION")));
    if let ConnectionState::Failed(failure) = &s.connection {
        let preview = error_preview(&failure.text);
        rows = rows.child(parts::detail_row(k, "Error", &preview.shown));
        if preview.cut {
            rows = rows.child(details::button(
                k,
                "pill-error-details",
                |s: &mut AppState, tx: &mut Tx| {
                    // The popover closes as the modal opens; focus returns to the pill.
                    if details::open_failure(s) {
                        tx.focus("pill", false);
                    }
                },
            ));
        }
    }
    if !s.read.session_log.is_empty() {
        rows = rows.child(session_log(s, k));
    }

    Popover::new("pill-popover")
        .trigger(face)
        .title("Connection")
        .child(rows)
        .open(s.pill_open)
        .placement(Placement::Bottom)
        .on_close(|s: &mut AppState| s.pill_open = false)
        .build(k, layer)
}

/// A session-log line's label: its time, what it reports, and its partition.
fn log_label(line: &LogLine) -> String {
    let what = match line.kind {
        LogKind::Error => "Error",
        LogKind::PartitionEnded => "Partition ended",
    };
    let partition = line
        .partition
        .as_deref()
        .map_or(String::new(), |partition| {
            format!(", partition {partition}")
        });
    format!("{} · {what}{partition}", line.at.format("%H:%M:%S UTC"))
}

/// The session's partition errors and ends, newest first; a line cut short offers its whole
/// text.
fn session_log(s: &AppState, k: &Kit) -> El {
    let mut lines = col().gap(SPACE_SM).w_full();
    for line in s.read.session_log.iter().rev() {
        let label = log_label(line);
        let preview = error_preview(&line.text);
        lines = lines.child(parts::detail_row(k, &label, &preview.shown));
        if preview.cut {
            let id = line.id;
            lines = lines.child(
                details::button(
                    k,
                    &format!("log-details-{id}"),
                    move |s: &mut AppState, tx: &mut Tx| open_log_line(s, tx, id),
                )
                .label(format!("View details: {label}")),
            );
        }
    }
    col()
        .gap(SPACE_SM)
        .w_full()
        .child(parts::section_header(k, "Session log"))
        .child(lines)
}

/// Opens the whole text of log line `id`; nothing (and no focus move) once it left the log.
pub(crate) fn open_log_line(s: &mut AppState, tx: &mut Tx, id: u64) {
    let Some(line) = s.read.session_log.iter().find(|line| line.id == id) else {
        return;
    };
    let (title, text) = (log_label(line), line.text.clone());
    // The popover closes as the modal opens; focus returns to the pill.
    tx.focus("pill", false);
    details::show(s, &title, &text);
}

fn log_detail(k: &Kit, logging: &FileLogging) -> El {
    match logging {
        FileLogging::Off => parts::detail_row(k, "Log file", "Logging disabled"),
        FileLogging::Writing(path) => parts::detail_row(k, "Log file", &path.display().to_string()),
        FileLogging::Failed { path, error } => col()
            .gap(SPACE_SM)
            .child(parts::detail_row(
                k,
                "Log file (unavailable)",
                &path.display().to_string(),
            ))
            .child(
                text(error.clone())
                    .font(TYPE_CAPTION)
                    .color(k.c.text_muted)
                    .wrap(),
            ),
    }
}

/// Connect opens the sheet (not while a send runs, which keeps every sheet closed); Disconnect
/// stops the session; nothing is pressable while finishing.
fn connection_button(s: &AppState, k: &Kit) -> El {
    match &s.connection {
        ConnectionState::Disconnected | ConnectionState::Failed(_) => Button::new("Connect")
            .variant(ButtonVariant::Neutral)
            .icon(&icons::CONNECT)
            .disabled(s.send.is_running())
            .on_press(|s: &mut AppState| dispatch(s, Input::OpenSheet(SheetPurpose::Connect)))
            .build(k)
            .key("connect"),
        ConnectionState::Connecting { .. } | ConnectionState::Connected { .. } => {
            Button::new("Disconnect")
                .variant(ButtonVariant::Ghost)
                .icon(&icons::DISCONNECT)
                .on_press(|s: &mut AppState| dispatch(s, Input::DisconnectPressed))
                .build(k)
                .key("disconnect")
        }
        ConnectionState::Stopping { .. } => Button::new("Finishing")
            .variant(ButtonVariant::Ghost)
            .disabled(true)
            .build(k)
            .key("finishing"),
    }
}

/// Toasts, bottom-right; each closes itself through its own id, and one whose text was cut
/// short offers the whole text.
pub fn toasts(s: &AppState, k: &Kit, layer: &mut Vec<El>) {
    if s.toasts.is_empty() {
        return;
    }
    let mut region = ToastRegion::new();
    for toast in &s.toasts {
        let id = toast.id;
        let title = match toast.kind {
            ToastKind::Info => "Done",
            ToastKind::Danger => "Something failed",
        };
        let preview = error_preview(&toast.text);
        let mut built = Toast::new(format!("toast-{id}"), title)
            .description(preview.shown)
            .on_close(move |s: &mut AppState| dispatch(s, Input::DismissToast(id)))
            .closable();
        if preview.cut {
            built = built.action(
                k,
                "View details",
                false,
                move |s: &mut AppState, _: &mut Tx| open_toast(s, id),
            );
        }
        region = region.toast(built.build(k));
    }
    layer.push(region.build(k));
}

/// Opens the whole text of toast `id`; nothing once it closed.
fn open_toast(s: &mut AppState, id: u64) {
    let Some(toast) = s.toasts.iter().find(|toast| toast.id == id) else {
        return;
    };
    let title = match toast.kind {
        ToastKind::Info => "Details",
        ToastKind::Danger => "Error details",
    };
    let text = toast.text.clone();
    details::show(s, title, &text);
}

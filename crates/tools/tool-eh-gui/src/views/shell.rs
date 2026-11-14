//! The window frame: toolbar with the section switch and the connection controls, the sidebar,
//! the content area, and toasts.

use super::sections;
use crate::app::dispatch;
use crate::design::tokens::{
    POPOVER_WIDTH, SPACE_LG, SPACE_MD, SPACE_SM, TYPE_CAPTION, TYPE_HEADLINE,
};
use crate::design::{Surface, icons, parts, surface};
use crate::state::{AppState, ConnectionState, Input, Section, ToastKind};
use common_cli::app_logger::FileLogging;
use rune::{Color, Cx, El, col, row, text};
use rune_kit::{
    Button, ButtonVariant, Kit, Placement, Popover, SegmentedControl, SplitView, Toast,
    ToastRegion, Toolbar,
};

pub fn shell(s: &AppState, cx: &Cx<AppState>, k: &Kit, layer: &mut Vec<El>) -> El {
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
        Section::Read => sections::read(s, k).key("read"),
        Section::Export => sections::export(k).key("export"),
    };
    let main = col()
        .key("content")
        .w_full()
        .h_full()
        .min_h(0.0)
        .fill(surface(k, Surface::Content))
        .child(content);

    SplitView::new(main)
        .sidebar(
            &s.sidebar,
            |s: &mut AppState| &mut s.sidebar,
            sections::sidebar(s, k),
        )
        .toolbar(toolbar)
        .titlebar_inset(cx.titlebar_inset())
        .reduced_motion(cx.is_reduced_motion())
        .build(k)
}

/// The pill's dot color and label for each connection state.
fn pill_status(k: &Kit, connection: &ConnectionState) -> (Color, String) {
    match connection {
        ConnectionState::Disconnected => (k.c.icon_faint, "Not connected".to_owned()),
        ConnectionState::Connecting { session: None, .. } => {
            (k.c.warning.fg, "Preparing".to_owned())
        }
        ConnectionState::Connecting {
            session: Some(_), ..
        } => (k.c.warning.fg, "Connecting".to_owned()),
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
    let (dot, label) = pill_status(k, &s.connection);
    let face = parts::pill_face(k, dot, &label)
        .key("pill")
        .role("button")
        .label(format!("Connection: {label}"))
        .on_press(|s: &mut AppState| s.pill_open = !s.pill_open);

    let mut details = col()
        .gap(SPACE_MD)
        .w(POPOVER_WIDTH)
        .child(parts::detail_row(k, "Data folder", &s.form.data_folder))
        .child(log_detail(k, &s.file_logging))
        .child(parts::detail_row(k, "Version", env!("CARGO_PKG_VERSION")));
    if let ConnectionState::Failed(failure) = &s.connection {
        details = details.child(parts::detail_row(k, "Error", &failure.text));
    }

    Popover::new("pill-popover")
        .trigger(face)
        .title("Connection")
        .child(details)
        .open(s.pill_open)
        .placement(Placement::Bottom)
        .on_close(|s: &mut AppState| s.pill_open = false)
        .build(k, layer)
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

/// Connect opens the sheet; Disconnect stops the session; nothing is pressable while finishing.
fn connection_button(s: &AppState, k: &Kit) -> El {
    match &s.connection {
        ConnectionState::Disconnected | ConnectionState::Failed(_) => Button::new("Connect")
            .variant(ButtonVariant::Neutral)
            .icon(&icons::CONNECT)
            .on_press(|s: &mut AppState| dispatch(s, Input::OpenSheet))
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

/// Toasts, bottom-right; each closes itself through its own id.
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
        region = region.toast(
            Toast::new(format!("toast-{id}"), title)
                .description(toast.text.clone())
                .on_close(move |s: &mut AppState| dispatch(s, Input::DismissToast(id)))
                .closable()
                .build(k),
        );
    }
    layer.push(region.build(k));
}

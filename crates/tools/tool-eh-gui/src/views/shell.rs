//! The window frame: toolbar with the section switch and the connection controls, the sidebar,
//! the content area, and toasts.

use super::{details, export, inspector, sections, send};
use crate::app::dispatch;
use crate::design::toast::{
    TOAST_INSET_BOTTOM, TOAST_INSET_RIGHT, TOAST_WIDTH, ToastActions, ToastFace, card_key,
    stack_extent, toast, toast_of_control, toast_presence, toast_size,
};
use crate::design::tokens::{FRAME_INSET, SPACE_LG, SPACE_SM, TYPE_CAPTION, TYPE_HEADLINE};
use crate::design::{Surface, icons, motion, parts, surface};
use crate::read::rows::{display_line, display_path};
use crate::read::{LogKind, LogLine};
use crate::state::{
    AppState, ConnectionState, Input, ListSection, Pane, Section, SheetPurpose, ToastBody,
    ToastInput, ToastKind, error_preview, home_label,
};
use common_cli::app_logger::FileLogging;
use rune::{Color, Cx, El, Key, KeyEvent, Tx, col, el, row, text};
use rune_kit::toast_stack::VISIBLE;
use rune_kit::{
    Button, ButtonVariant, Kit, PaneState, Placement, Popover, SegmentedControl, SplitView,
    ToastPlacement, Toolbar,
};
use tracing::debug;

/// The key of the kit split view's root, the element that fills the window.
const SHELL_KEY: &str = "shell";
/// The connection popover's scroll viewport.
pub const PILL_SCROLL_KEY: &str = "pill-popover-scroll";

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

    let window = cx.layout_with(SHELL_KEY, |rect| rect[2].floor());
    let fit = window.map(|window| s.pane_fit(window, s.section.list()));
    let shown = |pane: &PaneState, width: Option<f32>| {
        let mut pane = *pane;
        if let Some(width) = width {
            pane.set_width(width);
        }
        pane
    };

    let sidebar = sections::sidebar(s, k, layer);
    let split = SplitView::new(main)
        .sidebar(
            &shown(&s.sidebar, fit.map(|fit| fit.sidebar)),
            |s: &mut AppState| s.held_pane(Pane::Sidebar),
            sidebar,
        )
        .toolbar(toolbar)
        .titlebar_inset(cx.titlebar_inset())
        .frame_inset(FRAME_INSET)
        .reduced_motion(cx.is_reduced_motion());
    let inspector_shown = fit.and_then(|fit| fit.inspector);
    let split = match s.section.list() {
        Some(ListSection::Read) => split.inspector(
            &shown(&s.read.inspector.pane, inspector_shown),
            |s: &mut AppState| s.held_pane(Pane::Inspector(ListSection::Read)),
            inspector::inspector(s, k, ListSection::Read),
        ),
        Some(ListSection::Export) => split.inspector(
            &shown(&s.archive.inspector.pane, inspector_shown),
            |s: &mut AppState| s.held_pane(Pane::Inspector(ListSection::Export)),
            inspector::inspector(s, k, ListSection::Export),
        ),
        None => split,
    };
    // The panes' handlers cannot read the view's layout, so they read this copy; no view
    // reads it. The view reads the layout itself so the panes fit in the frame a resize lands
    // in; between reports the copy can be a point off that read.
    split
        .build(k)
        .on_layout_tx(|s: &mut AppState, rect: [f32; 4], tx: &mut Tx| {
            s.window_width = Some(rect[2].floor());
            tx.live_only();
        })
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
    let name = match s.home().profile_name() {
        Some(profile) => format!(
            "Connection: {label}, profile {}",
            display_line(profile.chars())
        ),
        None => format!("Connection: {label}"),
    };
    let face = face
        .key("pill")
        .role("button")
        .label(name)
        .on_press(|s: &mut AppState| {
            s.pill_open = !s.pill_open;
            if s.pill_open {
                s.read.log_seen();
            }
        });

    let folder = display_path(s.data_paths().base());
    let mut rows = vec![
        parts::detail_row(k, "Profile", &home_label(s.home())),
        parts::detail_row(k, "Data folder", &folder),
        log_detail(k, &s.file_logging),
        parts::detail_row(k, "Version", env!("CARGO_PKG_VERSION")),
    ];
    if let ConnectionState::Failed(failure) = &s.connection {
        let preview = error_preview(&failure.text);
        rows.push(parts::detail_row(k, "Error", &preview.shown));
        if preview.cut {
            rows.push(details::button(
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
        rows.push(session_log(s, k));
    }

    Popover::new("pill-popover")
        .trigger(face)
        .title("Connection")
        .padding(0.0)
        .child(parts::popover_body(k, PILL_SCROLL_KEY, rows))
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

/// How a toast card names itself when it has no title of its own.
fn toast_title(kind: ToastKind) -> &'static str {
    match kind {
        ToastKind::Info => "Done",
        ToastKind::Danger => "Something failed",
    }
}

/// A card's face as the view builds it: its preview, whether it offers the whole text, and
/// its height.
struct CardFace {
    id: u64,
    title: &'static str,
    shown: String,
    details: bool,
    height: f32,
    pulse: Option<u64>,
}

fn card_face(k: &Kit, id: u64, body: &ToastBody) -> CardFace {
    let preview = error_preview(&body.text);
    let (height, overflow) = toast_size(k, &preview.shown);
    CardFace {
        id,
        title: body.title.unwrap_or(toast_title(body.kind)),
        details: preview.cut || overflow,
        shown: preview.shown,
        height,
        pulse: body.pulse,
    }
}

/// The toasts, bottom right: a stack with the newest card in front and up to two older ones
/// peeking above it, fanned out while the pointer is over it or after Alt+T. Cards come newest
/// first in the tree, so Tab and assistive technology go from the front card up the deck,
/// and paint oldest under.
pub fn toasts(s: &AppState, k: &Kit, layer: &mut Vec<El>) {
    let stack = s.toasts();
    if stack.entries().is_empty() {
        return;
    }
    let faces: Vec<CardFace> = stack
        .entries()
        .iter()
        .rev()
        .map(|entry| card_face(k, entry.id, &entry.options))
        .collect();
    // Every face comes from an entry and `place` asks only about entries, so the lookup finds
    // one; a miss would cost that card its height rather than the whole stack.
    let height = |id: u64| match faces.iter().find(|face| face.id == id) {
        Some(face) => face.height,
        None => {
            debug!(id, "no face for a toast; it takes no height");
            0.0
        }
    };
    let expanded = stack.stack().expanded();
    let heights: Vec<f32> = faces.iter().map(|face| face.height).collect();
    let mut region = el()
        .key("toast-stack")
        .role("region")
        .label("Notifications")
        .pin(
            None,
            None,
            Some(TOAST_INSET_RIGHT),
            Some(TOAST_INSET_BOTTOM),
        )
        .w(TOAST_WIDTH)
        .h(stack_extent(&heights, expanded))
        .motion(motion::CONTROL)
        .on_hover_change(|s: &mut AppState, inside: bool, tx: &mut Tx| {
            dispatch(s, Input::Toast(ToastInput::Pointer(inside)));
            keep_toast_focus(s, tx);
        })
        .on_key(fold_toasts_on_escape);
    for face in &faces {
        // Faces come from the stack's own entries, so `place` finds each one; with
        // `MAX_TOASTS` equal to the kit's `VISIBLE`, every entry also shows.
        let Some(placement) = stack.place(face.id, height) else {
            debug!(id = face.id, "a toast has no place in the stack");
            continue;
        };
        if !placement.visible {
            continue;
        }
        region = region.child(card(k, face, placement));
    }
    layer.push(region);
}

/// One card at `placement`, pinned to the stack's foot.
fn card(k: &Kit, face: &CardFace, placement: ToastPlacement) -> El {
    let id = face.id;
    // Visible cards are the first VISIBLE from the front, so the order is 1 to VISIBLE.
    let order = i16::try_from(VISIBLE.saturating_sub(placement.index)).unwrap_or(i16::MAX);
    toast(
        k,
        id,
        ToastFace {
            title: face.title,
            shown: &face.shown,
            details: face.details,
            pulse: face.pulse,
        },
        placement,
        ToastActions {
            close: move |s: &mut AppState, tx: &mut Tx| {
                dispatch(s, Input::Toast(ToastInput::Dismiss(id)));
                keep_toast_focus(s, tx);
            },
            details: move |s: &mut AppState, _: &mut Tx| open_toast(s, id),
        },
    )
    .pin(Some(0.0), None, None, Some(0.0))
    .offset(0.0, -placement.lift)
    .scale(placement.scale)
    .origin(0.5, 0.0)
    .motion(motion::CONTROL)
    .paint_order(order)
    .transition(toast_presence())
}

/// Opens the whole text of toast `id`, holding new toasts while it shows; nothing once the
/// toast closed.
fn open_toast(s: &mut AppState, id: u64) {
    let Some(toast) = s.toasts().entries().iter().find(|toast| toast.id == id) else {
        return;
    };
    let title = match toast.options.kind {
        ToastKind::Info => "Details",
        ToastKind::Danger => "Error details",
    };
    let text = toast.options.text.clone();
    dispatch(
        s,
        Input::ShowDetails {
            title: title.to_owned(),
            text,
            toast: Some(id),
        },
    );
}

/// The toast whose control has focus; `None` for any other focus.
fn focused_toast(tx: &Tx) -> Option<u64> {
    tx.focused().and_then(toast_of_control)
}

/// Whether toast `id`'s controls show and take focus: it is in the stack, in front or with the
/// stack fanned out.
fn toast_controls_shown(s: &AppState, id: u64) -> bool {
    let stack = s.toasts();
    let present = stack.entries().iter().any(|entry| entry.id == id);
    let front = stack.front().map(|entry| entry.id) == Some(id);
    present && (front || stack.stack().expanded())
}

/// Focus to the front card's first control (View details, else Close), or nowhere when no
/// toast is left, showing the ring when the keyboard has been driving.
fn focus_front_toast(s: &AppState, tx: &mut Tx) {
    match s.toasts().front() {
        Some(front) => {
            let ring = tx.focus_visible();
            tx.focus(card_key(front.id), ring);
        }
        None => tx.blur(),
    }
}

/// Alt+T: fans the toasts out and focuses the front card's first control; false when there is
/// nothing to fan out (no toast, the window closed, a details modal open).
pub(crate) fn fan_out_toasts(s: &mut AppState, tx: &mut Tx) -> bool {
    dispatch(s, Input::Toast(ToastInput::Hotkey));
    let fanned = s.toasts().stack().expanded() && s.details().is_none();
    if fanned {
        focus_front_toast(s, tx);
    }
    fanned
}

/// Escape while the toasts are fanned out: folds them and keeps focus on a shown card; false
/// (the key passes on) for any other key or a folded stack.
pub(crate) fn fold_toasts_on_escape(s: &mut AppState, key: &KeyEvent, tx: &mut Tx) -> bool {
    let fold = matches!(key.key, Key::Escape) && s.toasts().stack().expanded();
    if fold {
        dispatch(s, Input::Toast(ToastInput::Escape));
        keep_toast_focus(s, tx);
    }
    fold
}

/// Keeps focus on a toast control that takes it: focus on a control of a card that left, or
/// one folded behind the front, moves to the front card. Focus anywhere else stays.
pub(crate) fn keep_toast_focus(s: &AppState, tx: &mut Tx) {
    let Some(id) = focused_toast(tx) else {
        return;
    };
    if !toast_controls_shown(s, id) {
        focus_front_toast(s, tx);
    }
}

/// After the details modal opened from toast `opener` closed: when the toasts it held folded
/// that card behind the front or pushed it out, focus goes to the front card; otherwise the
/// modal hands focus back to the card's View details.
pub(crate) fn return_toast_focus(s: &AppState, tx: &mut Tx, opener: Option<u64>) {
    let Some(id) = opener else {
        return;
    };
    if s.details().is_some() || toast_controls_shown(s, id) {
        return;
    }
    focus_front_toast(s, tx);
}

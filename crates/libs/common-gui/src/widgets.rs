use crate::tokens;
use crate::tokens::{Palette, SPACE_LG, SPACE_MD, SPACE_SM, SPACE_XS};
use egui::{
    Align2, Area, Button, Color32, Context, Frame, Id, Label, Order, Response, RichText, Stroke, Ui,
};
use std::time::{Duration, Instant};

/// Width of the fixed label column in [`form_row`], so stacked rows stay
/// aligned across sections.
const FORM_LABEL_WIDTH: f32 = 140.0;

/// A titled, bordered box: the basic grouping every view is built from.
pub fn section(ui: &mut Ui, title: &str, add_contents: impl FnOnce(&mut Ui)) {
    let p = Palette::default();
    Frame::new()
        .fill(p.bg_panel)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(6)
        .inner_margin(SPACE_MD)
        .show(ui, |ui| {
            ui.heading(title);
            ui.separator();
            add_contents(ui);
        });
}

/// One labeled form row: a fixed-width muted label, then the widget(s).
pub fn form_row(ui: &mut Ui, label: &str, widget: impl FnOnce(&mut Ui)) {
    let p = Palette::default();
    ui.horizontal(|ui| {
        let row_height = ui.spacing().interact_size.y;
        ui.add_sized(
            [FORM_LABEL_WIDTH, row_height],
            Label::new(RichText::new(label).color(p.text_muted)),
        );
        widget(ui);
    });
}

/// The filled call-to-action button: the one place the accent colors an
/// action. At most one per view.
pub fn primary_button(ui: &mut Ui, text: &str) -> Response {
    let p = Palette::default();
    ui.add(Button::new(RichText::new(text).color(Color32::WHITE)).fill(p.accent))
}

/// The quiet secondary button: transparent fill with a border stroke.
pub fn ghost_button(ui: &mut Ui, text: &str) -> Response {
    let p = Palette::default();
    ui.add(
        Button::new(text)
            .fill(Color32::TRANSPARENT)
            .stroke(Stroke::new(1.0, p.border)),
    )
}

/// The destructive-confirm button: ghost-shaped, error-colored. Reserve it
/// for the confirming click of a two-step destructive action.
pub fn danger_button(ui: &mut Ui, text: &str) -> Response {
    let p = Palette::default();
    ui.add(
        Button::new(RichText::new(text).color(p.error))
            .fill(Color32::TRANSPARENT)
            .stroke(Stroke::new(1.0, p.error)),
    )
}

/// Which status color a [`status_badge`] carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeKind {
    Ok,
    Warn,
    Err,
    Neutral,
}

/// A small pill naming a status. Status colors color text only here and in
/// toasts, never elsewhere.
pub fn status_badge(ui: &mut Ui, kind: BadgeKind, text: &str) {
    let p = Palette::default();
    let color = match kind {
        BadgeKind::Ok => p.ok,
        BadgeKind::Warn => p.warning,
        BadgeKind::Err => p.error,
        BadgeKind::Neutral => p.text_muted,
    };
    Frame::new()
        .fill(p.bg_raised)
        .stroke(Stroke::new(1.0, color))
        .corner_radius(4)
        .inner_margin(egui::vec2(SPACE_SM, SPACE_XS))
        .show(ui, |ui| {
            ui.label(RichText::new(text).font(tokens::text_small()).color(color));
        });
}

/// A frame that reads as a drop zone: quiet by default, accent-stroked while
/// `active` (a drag hovering the window). The caller owns the hover signal;
/// this recipe only styles it.
pub fn drop_target(ui: &mut Ui, active: bool, add_contents: impl FnOnce(&mut Ui)) {
    let p = Palette::default();
    let stroke_color = if active { p.accent } else { p.border };
    Frame::new()
        .fill(p.bg_panel)
        .stroke(Stroke::new(1.0, stroke_color))
        .corner_radius(6)
        .inner_margin(SPACE_MD)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            add_contents(ui);
        });
}

/// Full-width command strip for the top of a window.
pub fn toolbar(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui)) {
    let p = Palette::default();
    Frame::new()
        .fill(p.bg_panel)
        .inner_margin(egui::vec2(SPACE_MD, SPACE_SM))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(add_contents);
        });
}

/// What a [`Toast`] announces; picks its status color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Ok,
    Warn,
    Err,
}

/// One transient notification. The app owns the queue; [`show_toasts`]
/// renders it and drops expired entries.
#[derive(Debug, Clone)]
pub struct Toast {
    pub kind: ToastKind,
    pub text: String,
    pub expires_at: Instant,
}

impl Toast {
    /// A toast that expires `ttl` from now.
    pub fn new(kind: ToastKind, text: impl Into<String>, ttl: Duration) -> Self {
        Self {
            kind,
            text: text.into(),
            expires_at: Instant::now() + ttl,
        }
    }
}

/// Floor for the toast repaint wake: an on-demand renderer asked to wake
/// sooner than a frame budget repaints continuously instead of sleeping.
const MIN_TOAST_WAKE: Duration = Duration::from_millis(50);

/// Renders the toast stack bottom-right, newest first. Call it last in the
/// app's `ui` method. Retains only unexpired toasts and schedules exactly one
/// wake, for the earliest remaining expiry.
pub fn show_toasts(ctx: &Context, toasts: &mut Vec<Toast>) {
    let now = Instant::now();
    toasts.retain(|t| t.expires_at > now);
    if toasts.is_empty() {
        return;
    }

    let p = Palette::default();
    Area::new(Id::new("toasts"))
        .anchor(Align2::RIGHT_BOTTOM, egui::vec2(-SPACE_LG, -SPACE_LG))
        .order(Order::Foreground)
        .interactable(false)
        .show(ctx, |ui| {
            for toast in toasts.iter().rev() {
                let color = match toast.kind {
                    ToastKind::Ok => p.ok,
                    ToastKind::Warn => p.warning,
                    ToastKind::Err => p.error,
                };
                Frame::new()
                    .fill(p.bg_raised)
                    .stroke(Stroke::new(1.0, color))
                    .corner_radius(6)
                    .inner_margin(SPACE_MD)
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(&toast.text)
                                .font(tokens::text_body())
                                .color(p.text_primary),
                        );
                    });
            }
        });

    if let Some(earliest) = toasts.iter().map(|t| t.expires_at).min() {
        ctx.request_repaint_after(toast_wake(earliest, now));
    }
}

/// The single scheduled wake for the earliest expiry, floored at
/// [`MIN_TOAST_WAKE`].
fn toast_wake(earliest_expiry: Instant, now: Instant) -> Duration {
    earliest_expiry
        .saturating_duration_since(now)
        .max(MIN_TOAST_WAKE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::RawInput;

    /// Runs one headless pass. The returned deltas must be cleared: epaint's
    /// debug build panics when a `TexturesDelta` is dropped unhandled, and a
    /// headless test has no renderer to apply it.
    fn run_headless(ctx: &Context, add_contents: impl FnMut(&mut Ui)) {
        let mut output = ctx.run_ui(RawInput::default(), add_contents);
        output.textures_delta.clear();
    }

    #[test]
    fn recipes_render_headless_without_panicking() {
        let ctx = Context::default();

        run_headless(&ctx, |ui| {
            toolbar(ui, |ui| {
                ui.heading("title");
            });
            section(ui, "section", |ui| {
                form_row(ui, "label", |ui| {
                    ui.label("value");
                });
                status_badge(ui, BadgeKind::Ok, "ready");
                status_badge(ui, BadgeKind::Warn, "attention");
                status_badge(ui, BadgeKind::Err, "broken");
                status_badge(ui, BadgeKind::Neutral, "idle");
                let _ = primary_button(ui, "go");
                let _ = ghost_button(ui, "cancel");
                let _ = danger_button(ui, "confirm delete");
            });
            drop_target(ui, false, |ui| {
                ui.label("drop here");
            });
            drop_target(ui, true, |ui| {
                ui.label("hovering");
            });
        });
    }

    #[test]
    fn show_toasts_drops_expired_and_keeps_live_toasts() {
        let ctx = Context::default();
        let mut toasts = vec![
            Toast {
                kind: ToastKind::Ok,
                text: "stale".to_string(),
                expires_at: Instant::now() - Duration::from_secs(1),
            },
            Toast::new(ToastKind::Ok, "fresh ok", Duration::from_secs(60)),
            Toast::new(ToastKind::Warn, "fresh warn", Duration::from_secs(60)),
            Toast::new(ToastKind::Err, "fresh err", Duration::from_secs(60)),
        ];

        run_headless(&ctx, |ui| {
            show_toasts(ui.ctx(), &mut toasts);
        });

        assert_eq!(toasts.len(), 3);
        assert!(toasts.iter().all(|t| t.text.starts_with("fresh")));
    }

    #[test]
    fn show_toasts_with_an_empty_queue_is_a_no_op() {
        let ctx = Context::default();
        let mut toasts: Vec<Toast> = Vec::new();

        run_headless(&ctx, |ui| {
            show_toasts(ui.ctx(), &mut toasts);
        });

        assert!(toasts.is_empty());
    }

    #[test]
    fn toast_wake_is_the_remaining_time_to_the_earliest_expiry() {
        let now = Instant::now();

        let wake = toast_wake(now + Duration::from_secs(2), now);

        assert_eq!(wake, Duration::from_secs(2));
    }

    #[test]
    fn toast_wake_floors_short_and_past_deadlines() {
        let now = Instant::now();

        assert_eq!(toast_wake(now, now), MIN_TOAST_WAKE);
        assert_eq!(
            toast_wake(now + Duration::from_millis(1), now),
            MIN_TOAST_WAKE
        );
    }
}

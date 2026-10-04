use crate::tokens::SPACE_LG;

pub enum ToastKind { Ok, Warn, Err }
pub struct Toast { pub kind: ToastKind, pub text: String, pub expires_at: std::time::Instant }

/// Call last in App::ui. Retains unexpired toasts, renders newest first,
/// and schedules exactly one wake for the earliest expiry.
pub fn show_toasts(ctx: &egui::Context, toasts: &mut Vec<Toast>) {
    let now = std::time::Instant::now();
    toasts.retain(|t| t.expires_at > now);
    if toasts.is_empty() { return; }

    egui::containers::Area::new(egui::Id::new("toasts"))
        .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-SPACE_LG, -SPACE_LG))
        .order(egui::Order::Foreground)
        .interactable(false)
        .show(ctx, |ui| {
            for toast in toasts.iter().rev() { /* badge-colored Frame + body text */ }
        });

    // One wake at the earliest expiry, floored at 50 ms: a shorter request
    // degenerates into a busy repaint loop (house lesson from the last egui tool).
    if let Some(next) = toasts.iter().map(|t| t.expires_at).min() {
        let wait = next.saturating_duration_since(now).max(std::time::Duration::from_millis(50));
        ctx.request_repaint_after(wait);
    }
}
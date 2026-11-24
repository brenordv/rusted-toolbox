//! The toast card: a 320 pt card whose height is a pure function of its text, so a stack can
//! place every card before layout. The kit's `ToastStack` says where each card sits; the kit's
//! own `Toast` can't take a height from outside or fade and clip its contents apart from its
//! card, which a folded stack needs.

use super::tokens::{HAIRLINE, RADIUS_TILE, SPACE_MD, SPACE_SM, SPACE_XS, SPACE_XXL};
use super::{Surface, motion, surface};
use rune::text::{RichLayoutOptions, measure_with};
use rune::{El, MotionIntent, Sequence, TextStyle, Transition, Tx, col, el, row, text};
use rune_kit::toast_stack::ToastPlacement;
use rune_kit::{
    Button, ButtonVariant, ControlSize, IconButton, IconButtonVariant, Kit, toast_stack,
};

/// The card's width, as the kit's toast.
pub const TOAST_WIDTH: f32 = 320.0;
/// The most description lines a card shows; more ends the last in "…".
pub const TOAST_LINES: usize = 4;
/// Padding around the card's contents.
pub const TOAST_PADDING: f32 = SPACE_MD;
/// The title row: tall enough for a View details button at the control minimum.
pub const TOAST_HEADER: f32 = 28.0;
/// Between the title row and the description.
pub const TOAST_HEADER_GAP: f32 = SPACE_XS;
/// The stack's distance from the window's right edge.
pub const TOAST_INSET_RIGHT: f32 = SPACE_XXL;
/// The stack's distance from the window's bottom edge.
pub const TOAST_INSET_BOTTOM: f32 = 48.0;
/// How far a card rises as it arrives and sinks as it leaves.
pub const TOAST_TRAVEL: f32 = SPACE_SM;

/// The key of toast `id`'s card.
pub fn card_key(id: u64) -> String {
    format!("toast-{id}")
}

/// The key of toast `id`'s View details button.
pub fn details_key(id: u64) -> String {
    format!("toast-{id}-details")
}

/// The key of toast `id`'s Close button.
pub fn close_key(id: u64) -> String {
    format!("toast-{id}-close")
}

/// The toast a control keyed `key` belongs to: `Some(id)` for [`details_key`] and
/// [`close_key`], `None` for any other key.
pub fn toast_of_control(key: &str) -> Option<u64> {
    let rest = key.strip_prefix("toast-")?;
    let id = rest
        .strip_suffix("-details")
        .or_else(|| rest.strip_suffix("-close"))?;
    id.parse().ok()
}

/// The description's style; the measure and the card both use it.
fn description_style(k: &Kit) -> TextStyle {
    k.ty.ui
}

/// The description's width inside the padding.
const DESCRIPTION_WIDTH: f32 = TOAST_WIDTH - 2.0 * TOAST_PADDING;

/// A card's height for `shown` in its description, and whether the description runs past
/// [`TOAST_LINES`] lines at the card's width. The description takes its measured height, at
/// least one line and at most [`TOAST_LINES`] lines, so a line a fallback font draws taller
/// still fits.
pub fn toast_size(k: &Kit, shown: &str) -> (f32, bool) {
    let style = description_style(k);
    let line = style.line_height;
    let (_, measured) = measure_with(
        shown,
        &[],
        style,
        Some(DESCRIPTION_WIDTH),
        RichLayoutOptions::new().wrap_anywhere(),
    );
    let most = TOAST_LINES as f32 * line;
    let description = measured.clamp(line, most);
    let height = 2.0 * TOAST_PADDING + TOAST_HEADER + TOAST_HEADER_GAP + description;
    (height, measured > most + 0.5)
}

/// What a card shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToastFace<'a> {
    pub title: &'a str,
    /// The description as it shows in place.
    pub shown: &'a str,
    /// Whether the title row offers View details.
    pub details: bool,
    /// The repeat ring's key, while the card is ringing for its latest repeat.
    pub pulse: Option<u64>,
}

/// What a card's buttons run.
pub struct ToastActions<C, D> {
    pub close: C,
    pub details: D,
}

/// The card keyed `toast-<id>` at `placement`: a `status` named by the title and described by
/// `face.shown`, with View details (`toast-<id>-details`, when `face.details`) and Close
/// (`toast-<id>-close`) in its title row, each described by `face.shown` too, so focus on
/// either names its toast. Its contents are inert and faded out while
/// `placement.content` is false; the card clips them to its rounded rect, so a card shown
/// shorter than its text never draws past its edge.
pub fn toast<S: 'static>(
    k: &Kit,
    id: u64,
    face: ToastFace<'_>,
    placement: ToastPlacement,
    actions: ToastActions<impl Fn(&mut S, &mut Tx) + 'static, impl Fn(&mut S, &mut Tx) + 'static>,
) -> El {
    let mut header = row()
        .key("header")
        .w_full()
        .h(TOAST_HEADER)
        .gap(SPACE_SM)
        .center_y()
        .child(
            text(face.title)
                .key("title")
                .font(k.ty.ui_medium)
                .color(k.c.text_base)
                .truncate()
                .grow(1.0)
                .shrink()
                .min_w(0.0)
                .a11y_hidden(),
        );
    if face.details {
        header = header.child(
            Button::new("View details")
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Large)
                .on_press_tx(actions.details)
                .build(k)
                .key(details_key(id))
                .description(face.shown),
        );
    }
    header = header.child(
        IconButton::new(k.glyph.close)
            .variant(IconButtonVariant::Ghost)
            .control_size(ControlSize::Regular)
            .label("Close")
            .build(k)
            .key(close_key(id))
            .description(face.shown)
            .on_press_tx(actions.close),
    );
    let description = text(face.shown)
        .key("description")
        .font(description_style(k))
        .color(k.c.text_muted)
        .wrap_anywhere()
        .max_lines(TOAST_LINES)
        .w_full()
        .a11y_hidden();
    // The card's own status reads the title and the description; only the buttons inside
    // stay in the accessibility tree.
    let content = col()
        .key("body")
        .w_full()
        .padding(TOAST_PADDING)
        .gap(TOAST_HEADER_GAP)
        .child(header)
        .child(description);
    let content = if placement.content {
        content.opacity(1.0).opacity_motion(motion::CONTROL)
    } else {
        content
            .opacity(0.0)
            .opacity_motion(motion::CONTROL_EXIT)
            .inert()
    };
    let clip = col()
        .key("clip")
        .w_full()
        .h_full()
        .clip_rounded(RADIUS_TILE)
        .child(content);

    let mut card = col()
        .key(card_key(id))
        .role("status")
        .label(face.title)
        .description(face.shown)
        .w(TOAST_WIDTH)
        .h(placement.height)
        .radius(RADIUS_TILE)
        .fill(surface(k, Surface::Raised))
        .border(HAIRLINE, k.c.border_weak)
        .child(clip);
    for shadow in k.shadow_layers(&k.elevation.overlay) {
        card = card.box_shadow(shadow);
    }
    if let Some(pulse) = face.pulse {
        card = card.child(ring(k, pulse, face));
    }
    card
}

/// The repeat ring around a card, keyed by `pulse` so each repeat mounts a new one: a polite
/// status of its own, so a screen reader hears the repeat.
fn ring(k: &Kit, pulse: u64, face: ToastFace<'_>) -> El {
    el().key(format!("pulse-{pulse}"))
        .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
        .radius(RADIUS_TILE)
        .border(HAIRLINE, k.c.border_strong)
        .sequence(Sequence::new(&motion::TOAST_PULSE))
        .motion_intent(MotionIntent::PULSE)
        .role("status")
        .label(format!("Again: {}", face.title))
        .description(face.shown)
}

/// How a card enters and leaves: it rises and fades in, and sinks and fades out.
pub fn toast_presence() -> Transition {
    Transition::offset(0.0, TOAST_TRAVEL)
        .combined(Transition::opacity())
        .springs(motion::CONTROL_PAIR)
}

/// The stack's height for cards of `heights` (front first): fanned out, every card and a gap
/// between each; folded, the front card and a peek per card behind it.
pub fn stack_extent(heights: &[f32], expanded: bool) -> f32 {
    let Some(front) = heights.first() else {
        return 0.0;
    };
    let behind = (heights.len() - 1) as f32;
    if expanded {
        heights.iter().sum::<f32>() + behind * toast_stack::GAP
    } else {
        front + behind * toast_stack::PEEK
    }
}

#[cfg(test)]
mod tests;

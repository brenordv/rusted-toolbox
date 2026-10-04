use crate::tokens;
use crate::tokens::{Palette, SPACE_LG, SPACE_MD, SPACE_SM};

pub fn apply_theme(ctx: &egui::Context, palette: &Palette) {
    ctx.set_global_style(build_style(palette)); // takes impl Into<Arc<Style>>
    ctx.set_theme(egui::ThemePreference::Dark);
}

pub fn build_style(p: &Palette) -> egui::Style {
    let mut style = egui::Style::default();

    // Step 2: text styles. Map all five egui TextStyles onto the four tokens
    // (Button reuses the body token).
    style.text_styles = [
        (egui::TextStyle::Small, tokens::text_small()),
        (egui::TextStyle::Body, tokens::text_body()),
        (egui::TextStyle::Button, tokens::text_body()),
        (egui::TextStyle::Heading, tokens::text_heading()),
        (egui::TextStyle::Monospace, tokens::text_mono()),
    ]
    .into();

    // Step 3: spacing, all from the scale.
    style.spacing.item_spacing = egui::vec2(SPACE_SM, SPACE_SM);
    style.spacing.button_padding = egui::vec2(SPACE_MD, 6.0);
    style.spacing.interact_size.y = 28.0;
    style.spacing.indent = SPACE_LG;
    // window_margin / menu_margin from SPACE_LG; full field list on the Spacing page.

    // Step 4: visuals, starting from the dark baseline.
    let mut v = egui::Visuals::dark();
    v.panel_fill = p.bg_base;
    v.window_fill = p.bg_panel;
    v.extreme_bg_color = p.bg_panel; // text inputs
    v.faint_bg_color = p.bg_raised; // striped rows
    v.selection.bg_fill = p.accent;
    v.striped = true;
    // window/widget strokes from p.border; corner radius 4 widgets / 6 windows.

    // Step 5: all five widget states, explicitly.
    v.widgets.inactive = widget_visuals(p.bg_panel, p);
    v.widgets.hovered = widget_visuals(p.bg_raised, p);
    v.widgets.active = widget_visuals(p.bg_raised, p);
    v.widgets.open = widget_visuals(p.bg_raised, p);
    v.widgets.noninteractive = widget_visuals(p.bg_base, p);

    style.visuals = v;
    style
}

fn widget_visuals(bg: egui::Color32, p: &Palette) -> egui::style::WidgetVisuals {
    egui::style::WidgetVisuals {
        bg_fill: bg,
        weak_bg_fill: bg,
        bg_stroke: egui::Stroke::new(1.0, p.border),
        fg_stroke: egui::Stroke::new(1.0, p.text_primary),
        corner_radius: 4.into(),
        expansion: 0.0,
    }
}
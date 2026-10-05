use crate::tokens;
use crate::tokens::{Palette, SPACE_LG, SPACE_MD, SPACE_SM};

/// Installs the design system on `ctx`: the [`build_style`] output as the
/// global style, with the theme pinned dark. This is the only place global
/// style is mutated; call it once, in the app constructor.
pub fn apply_theme(ctx: &egui::Context, palette: &Palette) {
    ctx.set_global_style(build_style(palette));
    ctx.set_theme(egui::ThemePreference::Dark);
}

/// Builds the workspace style from the palette. Pure, so tests can assert on
/// the returned [`egui::Style`] without a window.
pub fn build_style(p: &Palette) -> egui::Style {
    let mut style = egui::Style {
        // All five egui text styles map onto the four type tokens (Button
        // reuses the body token).
        text_styles: [
            (egui::TextStyle::Small, tokens::text_small()),
            (egui::TextStyle::Body, tokens::text_body()),
            (egui::TextStyle::Button, tokens::text_body()),
            (egui::TextStyle::Heading, tokens::text_heading()),
            (egui::TextStyle::Monospace, tokens::text_mono()),
        ]
        .into(),
        visuals: build_visuals(p),
        ..egui::Style::default()
    };

    style.spacing.item_spacing = egui::vec2(SPACE_SM, SPACE_SM);
    style.spacing.button_padding = egui::vec2(SPACE_MD, 6.0);
    style.spacing.interact_size.y = 28.0;
    style.spacing.indent = SPACE_LG;
    style.spacing.window_margin = SPACE_LG.into();
    style.spacing.menu_margin = SPACE_LG.into();

    style
}

/// The dark baseline, re-tinted with the palette's three background steps.
fn build_visuals(p: &Palette) -> egui::Visuals {
    let mut v = egui::Visuals::dark();
    v.panel_fill = p.bg_base;
    v.window_fill = p.bg_panel;
    v.extreme_bg_color = p.bg_panel; // text inputs
    v.faint_bg_color = p.bg_raised; // striped rows
    v.selection.bg_fill = p.accent;
    v.striped = true;
    v.window_stroke = egui::Stroke::new(1.0, p.border);
    v.window_corner_radius = 6.into();

    // All five widget states, set explicitly and grey-scale only: hovered,
    // active, and open step the background up one level. The accent enters
    // through recipes, never through widget state.
    v.widgets.inactive = widget_visuals(p.bg_panel, p);
    v.widgets.hovered = widget_visuals(p.bg_raised, p);
    v.widgets.active = widget_visuals(p.bg_raised, p);
    v.widgets.open = widget_visuals(p.bg_raised, p);
    v.widgets.noninteractive = widget_visuals(p.bg_base, p);

    v
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

#[cfg(test)]
mod tests {
    use super::*;
    use egui::TextStyle;

    #[test]
    fn apply_theme_installs_the_style_and_the_dark_theme() {
        let ctx = egui::Context::default();

        apply_theme(&ctx, &Palette::default());

        assert_eq!(ctx.global_style().text_styles.len(), 5);
        assert_eq!(ctx.theme(), egui::Theme::Dark);
    }

    #[test]
    fn build_style_maps_all_five_text_styles_onto_the_tokens() {
        let style = build_style(&Palette::default());

        assert_eq!(style.text_styles.len(), 5);
        let cases = [
            (TextStyle::Small, tokens::text_small()),
            (TextStyle::Body, tokens::text_body()),
            (TextStyle::Button, tokens::text_body()),
            (TextStyle::Heading, tokens::text_heading()),
            (TextStyle::Monospace, tokens::text_mono()),
        ];
        for (text_style, token) in cases {
            let font = style.text_styles.get(&text_style).unwrap();
            assert_eq!(font.size, token.size);
            assert_eq!(font.family, token.family);
        }
    }

    #[test]
    fn build_style_spacing_comes_from_the_scale() {
        let style = build_style(&Palette::default());

        assert_eq!(style.spacing.item_spacing, egui::vec2(SPACE_SM, SPACE_SM));
        assert_eq!(style.spacing.button_padding.x, SPACE_MD);
        assert_eq!(style.spacing.interact_size.y, 28.0);
        assert_eq!(style.spacing.indent, SPACE_LG);
        assert_eq!(style.spacing.window_margin, egui::Margin::same(16));
        assert_eq!(style.spacing.menu_margin, egui::Margin::same(16));
    }

    #[test]
    fn build_style_fills_track_the_palette() {
        let p = Palette::default();

        let v = build_style(&p).visuals;

        assert_eq!(v.panel_fill, p.bg_base);
        assert_eq!(v.window_fill, p.bg_panel);
        assert_eq!(v.extreme_bg_color, p.bg_panel);
        assert_eq!(v.faint_bg_color, p.bg_raised);
        assert_eq!(v.selection.bg_fill, p.accent);
        assert_eq!(v.window_stroke.color, p.border);
        assert!(v.striped);
        assert!(v.dark_mode);
    }

    #[test]
    fn build_style_sets_all_five_widget_states_from_the_palette() {
        let p = Palette::default();

        let w = build_style(&p).visuals.widgets;

        assert_eq!(w.inactive.bg_fill, p.bg_panel);
        assert_eq!(w.hovered.bg_fill, p.bg_raised);
        assert_eq!(w.active.bg_fill, p.bg_raised);
        assert_eq!(w.open.bg_fill, p.bg_raised);
        assert_eq!(w.noninteractive.bg_fill, p.bg_base);
        for state in [w.noninteractive, w.inactive, w.hovered, w.active, w.open] {
            assert_eq!(state.weak_bg_fill, state.bg_fill);
            assert_eq!(state.bg_stroke.color, p.border);
            assert_eq!(state.fg_stroke.color, p.text_primary);
            assert_eq!(state.expansion, 0.0);
        }
    }
}

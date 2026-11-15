use super::*;
use tokens::{SPACE_LG, SPACE_MD, SPACE_SM, SPACE_XL, SPACE_XS, SPACE_XXL};

const TEXT: f32 = 4.5;
const NON_TEXT: f32 = 3.0;
const LEVELS: [Surface; 3] = [Surface::Window, Surface::Content, Surface::Raised];

/// WCAG contrast ratio; `Color` holds linear light, so `luminance` is relative luminance.
fn contrast(a: Color, b: Color) -> f32 {
    let (a, b) = (a.luminance(), b.luminance());
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// `top` (straight alpha) composited over opaque `bottom`.
fn over(top: Color, bottom: Color) -> Color {
    let blend = |t: f32, b: f32| t * top.a + b * (1.0 - top.a);
    Color {
        r: blend(top.r, bottom.r),
        g: blend(top.g, bottom.g),
        b: blend(top.b, bottom.b),
        a: 1.0,
    }
}

struct Contract {
    failures: Vec<String>,
}

impl Contract {
    fn check(&mut self, name: &str, fg: Color, bg: Color, minimum: f32) {
        let ratio = contrast(fg, bg);
        if ratio < minimum {
            self.failures
                .push(format!("{name}: {ratio:.2}:1 (needs {minimum}:1)"));
        }
    }
}

fn appearance(dark: bool) -> &'static str {
    if dark { "dark" } else { "light" }
}

#[test]
fn every_contract_pair_meets_its_minimum_in_both_appearances() {
    let ds = Ds::new();
    let mut contract = Contract {
        failures: Vec::new(),
    };

    for dark in [true, false] {
        let k = ds.kit(dark);
        let mode = appearance(dark);
        for level in LEVELS {
            let bg = surface(k, level);
            contract.check(&format!("{mode} fg on {level:?}"), k.c.text_base, bg, TEXT);
            contract.check(
                &format!("{mode} muted on {level:?}"),
                k.c.text_muted,
                bg,
                TEXT,
            );
            contract.check(
                &format!("{mode} accent on {level:?}"),
                k.c.text_accent,
                bg,
                TEXT,
            );
            contract.check(
                &format!("{mode} muted icon on {level:?}"),
                k.c.icon_muted,
                bg,
                NON_TEXT,
            );
            contract.check(
                &format!("{mode} focus ring on {level:?}"),
                k.c.border_focus,
                bg,
                NON_TEXT,
            );
            contract.check(
                &format!("{mode} live dot on {level:?}"),
                k.c.success.fg,
                bg,
                NON_TEXT,
            );
            contract.check(
                &format!("{mode} danger dot on {level:?}"),
                k.c.danger.fg,
                bg,
                NON_TEXT,
            );
        }

        let content = surface(k, Surface::Content);
        for level in [Surface::Window, Surface::Content] {
            contract.check(
                &format!("{mode} danger caption on {level:?}"),
                k.c.danger.fg,
                surface(k, level),
                TEXT,
            );
        }
        contract.check(
            &format!("{mode} warning caption on Content"),
            k.c.warning.fg,
            content,
            TEXT,
        );
        contract.check(
            &format!("{mode} success check on Content"),
            k.c.success.fg,
            content,
            NON_TEXT,
        );
        let selected = over(k.selection(), content);
        contract.check(
            &format!("{mode} fg on selection"),
            k.c.text_base,
            selected,
            TEXT,
        );
        contract.check(
            &format!("{mode} secondary on selection"),
            selected_secondary_text(k),
            selected,
            TEXT,
        );

        let tint = floating_tint(k);
        for (name, backdrop) in [("white", Color::WHITE), ("black", Color::BLACK)] {
            let floating = over(tint, backdrop);
            contract.check(
                &format!("{mode} fg on floating over {name}"),
                k.c.text_base,
                floating,
                TEXT,
            );
        }

        let fill = k.c.bg_icon_button_contrast;
        for (state, face) in [
            ("rest", fill),
            ("sheen", over(k.c.submit_highlight, fill)),
            ("hover", over(k.c.submit_hover, fill)),
            ("pressed", over(k.c.contrast_pressed, fill)),
        ] {
            contract.check(
                &format!("{mode} Submit label {state}"),
                k.c.submit_fg,
                face,
                TEXT,
            );
        }
    }

    assert!(
        contract.failures.is_empty(),
        "contrast contract failures:\n{}",
        contract.failures.join("\n")
    );
}

#[test]
fn the_surface_ladder_rises_away_from_the_window() {
    let ds = Ds::new();
    let (dark, light) = (ds.kit(true), ds.kit(false));
    let lum = |k: &Kit, level| surface(k, level).luminance();

    assert!(lum(dark, Surface::Window) < lum(dark, Surface::Content));
    assert!(lum(dark, Surface::Content) < lum(dark, Surface::Raised));
    assert!(lum(light, Surface::Window) < lum(light, Surface::Content));
    assert_eq!(
        surface(light, Surface::Content),
        surface(light, Surface::Raised)
    );
}

#[test]
fn spacing_ascends() {
    let scale = [SPACE_XS, SPACE_SM, SPACE_MD, SPACE_LG, SPACE_XL, SPACE_XXL];

    assert!(scale.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn both_appearances_share_the_submit_fill() {
    let ds = Ds::new();

    assert_eq!(
        ds.kit(true).c.bg_icon_button_contrast,
        ds.kit(false).c.bg_icon_button_contrast
    );
    assert!(ds.kit(true).dark);
    assert!(!ds.kit(false).dark);
}

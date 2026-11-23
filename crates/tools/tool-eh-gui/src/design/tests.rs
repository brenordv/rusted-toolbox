use super::*;
use common_gui_rune::contrast::{Contract, LEVELS, NON_TEXT, TEXT, appearance_name, over};

#[test]
fn every_eh_contract_pair_meets_its_minimum_in_both_appearances() {
    let ds = Ds::new();
    let mut contract = Contract::default();

    for dark in [true, false] {
        let k = ds.kit(dark);
        let mode = appearance_name(dark);
        for level in LEVELS {
            let bg = surface(k, level);
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

        for (name, fg) in [("fg", k.c.text_base), ("muted", k.c.text_muted)] {
            contract.check(
                &format!("{mode} {name} on the popover fill"),
                fg,
                k.c.surface_raised,
                TEXT,
            );
        }

        let content = surface(k, Surface::Content);
        contract.check(
            &format!("{mode} danger caption on the sheet"),
            k.c.danger.fg,
            k.c.layer_01,
            TEXT,
        );
        contract.check(
            &format!("{mode} warning caption on the sheet"),
            k.c.warning.fg,
            k.c.layer_01,
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
    }

    contract.assert_met();
}

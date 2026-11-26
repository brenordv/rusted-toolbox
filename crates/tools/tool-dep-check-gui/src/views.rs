//! The dep-check window: Ecosystem, Packages, and Results in a form column over the window's
//! surface, and a fixed action row with the status and Check.

use crate::design::{
    BADGE_WIDTH, FORM_MAX_WIDTH, PACKAGES_LINES, RESULT_ROW_ESTIMATE, RESULTS_MIN_ROWS,
};
use crate::services::dispatch;
use crate::state::{DepCheckState, Input, PACKAGES_KEY, every_source_answered, issue_words};
use common_gui_rune::keys::{is_chord, type_newline};
use common_gui_rune::motion::CONTROL_PAIR;
use common_gui_rune::parts::{
    BadgeTone, EditorHeight, EditorSpec, ListHeight, ListRows, StatusLines, action_row, badge,
    caption, field_label, issue, list_box, status_line, text_editor, warning,
};
use common_gui_rune::tokens::{
    CONTROL_HEIGHT, SPACE_LG, SPACE_SM, SPACE_XL, SPACE_XS, TYPE_CAPTION, TYPE_MONO,
};
use common_gui_rune::{Surface, surface};
use common_utils::string_utils::{display_line, error_preview};
use dep_check_core::advisory::AdvisoryKind;
use dep_check_core::ecosystem::Ecosystem;
use dep_check_core::render::{finding_lines, not_found_sentence, sources_line, supply_chain_line};
use dep_check_core::report::{PackageReport, PackageStatus, Report, VersionFrom};
use rune::{Cx, El, GrowthSpec, KeyEvent, ScrollSpec, Transition, Tx, col, row, text};
use rune_kit::{Button, ButtonVariant, Kit, ScrollBar, Select, SelectAppearance};
use std::borrow::Cow;
use tracing::info;

/// The window's title in the OS.
pub const OS_TITLE: &str = "dep-check";
/// The ecosystem picker; the kit keys its trigger [`ECOSYSTEM_TRIGGER_KEY`].
pub const ECOSYSTEM_KEY: &str = "dep-check-ecosystem";
/// The ecosystem picker's trigger, the control that takes focus.
pub const ECOSYSTEM_TRIGGER_KEY: &str = "dep-check-ecosystem-trigger";
/// The issue caption under Packages.
pub const PACKAGES_ISSUE_KEY: &str = "dep-check-packages-issue";
/// The outer scroll viewport.
pub const SCROLL_KEY: &str = "dep-check-scroll";
/// The column inside the outer scroll.
pub const FORM_KEY: &str = "dep-check-form";
/// The caption that says what each source did.
pub const SOURCES_KEY: &str = "dep-check-sources";
/// The group around a sources caption that was cut, holding the whole line.
pub const SOURCES_GROUP_KEY: &str = "dep-check-sources-group";
/// The Results box.
pub const RESULTS_KEY: &str = "dep-check-results";
/// The virtual column of result rows.
pub const RESULT_ROWS_KEY: &str = "dep-check-result-rows";
/// The fixed row under the form.
pub const ACTIONS_KEY: &str = "dep-check-actions";
/// The Check button.
pub const CHECK_KEY: &str = "dep-check-check";
/// The hint under Packages, also its accessible description.
pub const PACKAGES_HINT: &str = "One per line: name or name@version. No version checks the latest.";

/// The status line's key while it says `kind`, so its live region and fade follow the run's
/// state rather than its text.
pub fn status_key(kind: &str) -> String {
    format!("dep-check-status-{kind}")
}

/// A result row's key, from the package as it was typed (unique once duplicates are dropped).
pub fn row_key(input: &str) -> String {
    format!("dep-check-row-{input}")
}

/// Runs when the window is first on screen: focus goes to Packages. True, so the commit happens.
pub fn shown(_: &mut DepCheckState, tx: &mut Tx) -> bool {
    tx.focus(PACKAGES_KEY, false);
    true
}

/// Sets the appearance the window draws in (true: dark); true when it changed.
pub fn set_appearance(s: &mut DepCheckState, dark: bool) -> bool {
    s.theme.set(dark)
}

/// Runs as the window closes. A check still running is dropped with the runtime.
pub fn on_close(s: &mut DepCheckState, _: &mut Tx) {
    if s.is_checking() {
        info!("check dropped: the window closed");
    }
}

/// The window's shortcut: `mod+enter` runs Check. True when `key` was it. A focused field takes
/// chords for itself, so the Packages editor runs this before its own keys.
pub fn window_key(s: &mut DepCheckState, key: &KeyEvent, tx: &mut Tx) -> bool {
    if is_chord("mod+enter", key, tx) {
        dispatch(s, tx, Input::CheckPressed);
        return true;
    }
    false
}

/// Keys in Packages, before the editor sees them: the window's shortcut, and Return typing a
/// line break, which counts as an edit (the editor's change hook does not run for it).
fn packages_key(s: &mut DepCheckState, key: &KeyEvent, tx: &mut Tx) -> bool {
    if window_key(s, key, tx) {
        return true;
    }
    if type_newline(&mut s.packages, key) {
        dispatch(s, tx, Input::PackagesEdited);
        return true;
    }
    false
}

/// The window's root.
pub fn view(s: &DepCheckState, cx: &mut Cx<DepCheckState>) -> El {
    let system_dark = cx.system_dark();
    let k = s.theme.kit(system_dark);
    let viewport = cx.layout_with(SCROLL_KEY, |rect| rect[3]);
    let mut layer = Vec::new();
    let form = col()
        .w_full()
        .max_w(FORM_MAX_WIDTH)
        .grow(1.0)
        .gap(SPACE_XL)
        .child(ecosystem_field(s, k, &mut layer))
        .child(packages_field(s, k))
        .child(results(s, k, system_dark));
    let body = col()
        .key(SCROLL_KEY)
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(
            col()
                .key(FORM_KEY)
                .w_full()
                .min_h(viewport.unwrap_or(0.0))
                .padding(SPACE_LG)
                .items_center()
                .child(form),
        )
        .scroll_bar(k);
    let window = col()
        .w_full()
        .h_full()
        .fill(surface(k, Surface::Window))
        .child(body)
        .child(actions(s, k));
    rune_kit::root(window, layer)
}

/// Ecosystem: the kit picker under its label; the overlay goes in `layer`.
fn ecosystem_field(s: &DepCheckState, k: &Kit, layer: &mut Vec<El>) -> El {
    let labels: Vec<&str> = Ecosystem::ALL
        .iter()
        .map(|ecosystem| ecosystem.label())
        .collect();
    let picker = Select::new(ECOSYSTEM_KEY, &labels)
        .label("Ecosystem")
        .appearance(SelectAppearance::Field)
        .current(Some(s.ecosystem.label()))
        .open(s.picker_open)
        .on_toggle(|s: &mut DepCheckState| s.picker_open = !s.picker_open)
        .on_close(|s: &mut DepCheckState| s.picker_open = false)
        .on_choose(|s: &mut DepCheckState, label: &str, tx: &mut Tx| {
            s.picker_open = false;
            if let Some(ecosystem) = Ecosystem::ALL
                .into_iter()
                .find(|ecosystem| ecosystem.label() == label)
            {
                dispatch(s, tx, Input::EcosystemPicked(ecosystem));
            }
        })
        .build(k, layer);
    col()
        .w_full()
        .gap(SPACE_XS)
        .child(field_label(k, "Ecosystem").a11y_hidden())
        .child(picker)
}

/// Packages: the editor, the hint under it, and after a refused press the issue under the hint.
/// The issue leads the editor's description, so focus landing there reads it first; the status
/// line is the press's live region.
fn packages_field(s: &DepCheckState, k: &Kit) -> El {
    let shown_issue = s
        .packages_issue
        .as_ref()
        .map(|issue| display_line(issue_words(issue).chars()));
    let description = match &shown_issue {
        Some(issue) => format!("{issue} {PACKAGES_HINT}"),
        None => PACKAGES_HINT.to_owned(),
    };
    let spec = EditorSpec {
        key: PACKAGES_KEY,
        label: "Packages",
        placeholder: "serde@1.0.215",
        description: Some(&description),
        invalid: shown_issue.is_some(),
        height: EditorHeight::Fixed {
            lines: PACKAGES_LINES,
        },
        font: TYPE_MONO,
    };
    let mut field = col()
        .w_full()
        .gap(SPACE_XS)
        .child(field_label(k, "Packages").a11y_hidden())
        .child(text_editor(
            k,
            spec,
            &s.packages,
            |s: &mut DepCheckState| &mut s.packages,
            packages_key,
            |s: &mut DepCheckState, tx: &mut Tx| {
                dispatch(s, tx, Input::PackagesEdited);
            },
        ))
        .child(caption(k, PACKAGES_HINT).a11y_hidden());
    if let Some(shown_issue) = shown_issue {
        field = field.child(issue(k, &shown_issue).key(PACKAGES_ISSUE_KEY).a11y_hidden());
    }
    field
}

/// Results: the label, the sources caption while a report is on screen, and the list, which
/// takes the height the form leaves.
fn results(s: &DepCheckState, k: &Kit, system_dark: bool) -> El {
    let report = s.report();
    let label = match s.results_ecosystem() {
        Some(ecosystem) => format!("Results for {}", ecosystem.label()),
        None => "Results".to_owned(),
    };
    let empty = if s.is_checking() {
        "Results show here when the check finishes."
    } else {
        "Results show here after a check."
    };
    let count = report.map_or(0, |report| report.packages.len());
    let column = col()
        .key(RESULT_ROWS_KEY)
        .w_full()
        .gap(SPACE_XS)
        .growth(GrowthSpec::default())
        .virtual_rows(
            count,
            RESULT_ROW_ESTIMATE,
            |s: &DepCheckState, index| Cow::Owned(row_key_at(s, index)),
            move |s: &DepCheckState, index| result_row_at(s, system_dark, index),
        )
        .virtual_rows_version(s.answers);
    let mut group = col()
        .w_full()
        .grow(1.0)
        .gap(SPACE_XS)
        .child(field_label(k, &label).a11y_hidden());
    if let Some(report) = report {
        group = group.child(sources_caption(k, report));
    }
    group.child(list_box(
        k,
        (RESULTS_KEY, &label),
        ListRows::Virtual { count, column },
        empty,
        ListHeight::Fill {
            min_rows: RESULTS_MIN_ROWS,
        },
    ))
}

/// What each source did, in the warning tone when one did not fully answer, so a "Clean" badge
/// is not read as checked by every source when it was not. Failure text can be any length: the
/// line shows at most 140 characters, and once cut, a group named by what shows carries the
/// whole line as its description.
fn sources_caption(k: &Kit, report: &Report) -> El {
    let line = sources_line(report);
    let preview = error_preview(&line);
    let words = if every_source_answered(report) {
        caption(k, &preview.shown)
    } else {
        warning(k, &preview.shown)
    }
    .key(SOURCES_KEY);
    if !preview.cut {
        return words;
    }
    col()
        .key(SOURCES_GROUP_KEY)
        .role("group")
        .label(preview.shown.as_str())
        .description(display_line(line.chars()))
        .w_full()
        .child(words.a11y_hidden())
}

fn row_key_at(s: &DepCheckState, index: usize) -> String {
    match s.report().and_then(|report| report.packages.get(index)) {
        Some(package) => row_key(&package.input),
        None => format!("dep-check-row-gone-{index}"),
    }
}

/// The row at `index` of the results, drawn with the kit for `system_dark`. An index past the
/// rows gets an empty row a control tall.
fn result_row_at(s: &DepCheckState, system_dark: bool, index: usize) -> El {
    match s.report().and_then(|report| report.packages.get(index)) {
        Some(package) => result_row(s.theme.kit(system_dark), package),
        None => row().h(CONTROL_HEIGHT),
    }
}

/// The badge's words and tone for a package's status.
pub fn badge_for(package: &PackageReport) -> (&'static str, BadgeTone) {
    match package.status {
        PackageStatus::Vulnerable if package.findings.iter().any(is_malware) => {
            ("Malicious", BadgeTone::Failed)
        }
        PackageStatus::Vulnerable => ("Vulnerable", BadgeTone::Failed),
        PackageStatus::Clean => ("Clean", BadgeTone::Finished),
        PackageStatus::NotFound => ("Not found", BadgeTone::Attention),
        PackageStatus::NotChecked => ("Not checked", BadgeTone::Attention),
    }
}

fn is_malware(finding: &dep_check_core::report::Finding) -> bool {
    match finding.kind {
        AdvisoryKind::Malware => true,
        AdvisoryKind::Vulnerability => false,
    }
}

/// How a row line is set: a value to copy, or prose.
#[derive(Debug, Clone, Copy)]
enum LineKind {
    Value,
    Prose,
}

/// One line under a package's name: what shows, and the whole of it for the row's description.
struct RowLine {
    kind: LineKind,
    shown: String,
    whole: String,
}

impl RowLine {
    fn of(kind: LineKind, text: String) -> Self {
        Self {
            kind,
            whole: text.clone(),
            shown: text,
        }
    }
}

/// The lines under a package's name, every outside string cleaned by the renderer or here.
/// Failure text in a not-checked reason shows at most 140 characters.
fn row_lines(package: &PackageReport) -> Vec<RowLine> {
    let mut lines = Vec::new();
    for finding in &package.findings {
        let finding = finding_lines(finding);
        lines.push(RowLine::of(LineKind::Value, finding.head));
        if let Some(summary) = finding.summary {
            lines.push(RowLine::of(LineKind::Prose, summary));
        }
        if let Some(aliases) = finding.aliases {
            lines.push(RowLine::of(LineKind::Value, aliases));
        }
        lines.push(RowLine::of(LineKind::Prose, finding.scores));
    }
    if let Some(chain) = &package.supply_chain {
        lines.push(RowLine::of(LineKind::Prose, supply_chain_line(chain)));
    }
    match package.status {
        PackageStatus::NotFound => lines.push(RowLine::of(
            LineKind::Prose,
            not_found_sentence(package).to_owned(),
        )),
        PackageStatus::NotChecked => {
            if let Some(reason) = &package.reason {
                lines.push(RowLine {
                    kind: LineKind::Prose,
                    shown: error_preview(reason).shown,
                    whole: display_line(reason.chars()),
                });
            }
        }
        PackageStatus::Vulnerable | PackageStatus::Clean => {}
    }
    lines
}

/// A package's name and version as the row shows them, " (latest)" when deps.dev named it.
pub fn row_name(package: &PackageReport) -> String {
    let mut name = display_line(package.name.chars());
    if let Some(version) = &package.version {
        name.push(' ');
        name.push_str(&display_line(version.chars()));
    }
    if package.version_from == Some(VersionFrom::DepsDevLatest) {
        name.push_str(" (latest)");
    }
    name
}

/// One package: its badge, then its name and every line under it, top-aligned and wrapping.
/// The row is one node named by its badge and name and described by its lines.
fn result_row(k: &Kit, package: &PackageReport) -> El {
    let (status, tone) = badge_for(package);
    let name = row_name(package);
    let lines = row_lines(package);
    let description = lines
        .iter()
        .map(|line| line.whole.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    // The name and the badge sit centred on a control's height and stay at the top when the
    // lines under them wrap.
    let mono_inset = (CONTROL_HEIGHT - TYPE_MONO.line_px()) / 2.0;
    let badge_inset = (CONTROL_HEIGHT - TYPE_CAPTION.line_px()) / 2.0;
    let mut main = col()
        .grow(1.0)
        .min_w(0.0)
        .gap(SPACE_XS)
        .pt(mono_inset)
        .child(
            text(name.clone())
                .font(TYPE_MONO)
                .color(k.c.text_base)
                .wrap_anywhere()
                .a11y_hidden(),
        );
    for RowLine { kind, shown, .. } in lines {
        let line = match kind {
            LineKind::Value => text(shown)
                .font(TYPE_MONO)
                .color(k.c.text_muted)
                .wrap_anywhere()
                .selectable(true),
            LineKind::Prose => text(shown).font(TYPE_CAPTION).color(k.c.text_muted).wrap(),
        };
        main = main.child(line.a11y_hidden());
    }
    row()
        .key(row_key(&package.input))
        .role("group")
        .label(format!("{status}, {name}"))
        .description(description)
        .w_full()
        .min_h(CONTROL_HEIGHT)
        .gap(SPACE_SM)
        .child(badge(k, status, tone, BADGE_WIDTH).mt(badge_inset))
        .child(main)
}

/// The fixed row: the status leading, Check trailing. While a check runs Check is `Loading`,
/// and a second press is refused by the state.
fn actions(s: &DepCheckState, k: &Kit) -> El {
    let check = Button::new("Check")
        .variant(if s.is_checking() {
            ButtonVariant::Loading
        } else {
            ButtonVariant::Submit
        })
        .on_press_tx(|s: &mut DepCheckState, tx: &mut Tx| {
            dispatch(s, tx, Input::CheckPressed);
        })
        .build(k)
        .key(CHECK_KEY);
    let status = s.status().map(|words| {
        status_line(
            k,
            &status_key(words.kind),
            words.tone,
            StatusLines::Wrap,
            &words.text,
        )
        .transition(Transition::opacity().springs(CONTROL_PAIR))
    });
    action_row(k, ACTIONS_KEY, status, vec![check])
}

#[cfg(test)]
mod tests;

use super::*;
use crate::design::{WINDOW_MIN, WINDOW_SIZE};
use crate::state::Run;
use crate::test_support::{
    all_answered, all_blocked_report, failed, finding, later, package, rate_limited, recording,
    report, sample_report, supply_chain,
};
use common_gui_rune::test_harness::{
    a11y_node, a11y_nodes, command, deliver, focus_is_inside, has_key, has_text, label_of, press,
    press_key, rect, runtime_with_keys, settle, type_into,
};
use common_gui_rune::tokens::SPACE_MD;
use dep_check_core::advisory::{Cvss, SourceName};
use dep_check_core::http::SourceFailure;
use dep_check_core::report::{BlockOrigin, SourceRepo, SourceStatus};
use rstest::rstest;
use rune::{Key, Runtime, TextEdit};
use serde_json::Value;
use std::time::{Duration, Instant};

const EVIL: &str = "a\x1b[2Jb\nc\u{202E}d";
/// [`EVIL`] once cleaned: each control or bidirectional character a space.
const CLEANED: &str = "a [2Jb c d";

fn window(state: DepCheckState, size: (f32, f32)) -> Runtime<DepCheckState> {
    runtime_with_keys(state, view, window_key, size)
}

fn answer(rt: &mut Runtime<DepCheckState>, report: Report) {
    deliver(rt, move |s, tx| {
        dispatch(s, tx, Input::Checked(Ok(Box::new(report))))
    });
}

/// A window that checked `report`'s packages and shows `report`.
fn showing(report: Report, size: (f32, f32)) -> Runtime<DepCheckState> {
    let (mut state, _) = recording("serde");
    state.apply(Input::CheckPressed);
    state.apply(Input::Checked(Ok(Box::new(report))));
    window(state, size)
}

fn states_of(node: &Value) -> Vec<String> {
    node["states"]
        .as_array()
        .map(|states| {
            states
                .iter()
                .filter_map(|state| state.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// Every element text and every accessible name and description on screen.
fn every_string(rt: &Runtime<DepCheckState>) -> Vec<String> {
    let mut strings: Vec<String> = rt
        .elements()
        .into_iter()
        .filter_map(|element| element.text)
        .collect();
    let tree = rt.accessibility_json();
    for node in a11y_nodes(&tree) {
        for field in ["name", "description"] {
            if let Some(text) = node[field].as_str() {
                strings.push(text.to_owned());
            }
        }
    }
    strings
}

/// The box of the element whose whole text is `words`.
fn text_rect(rt: &Runtime<DepCheckState>, words: &str) -> [f32; 4] {
    rt.elements()
        .into_iter()
        .find(|element| element.text.as_deref() == Some(words) && !element.exiting)
        .map(|element| element.rect)
        .unwrap_or_else(|| panic!("no element reads {words}"))
}

fn row_node<'a>(tree: &'a Value, input: &str) -> &'a Value {
    a11y_node(tree, &row_key(input)).unwrap_or_else(|| panic!("no row for {input}: {tree}"))
}

/// G10: every outside string a row shows, with a terminal escape, a newline, and a
/// right-to-left override in it.
fn hostile_report() -> Report {
    let mut bad = finding(&format!("ID-{EVIL}"), AdvisoryKind::Vulnerability);
    bad.aliases = vec![EVIL.to_owned()];
    bad.fixed_in = vec![EVIL.to_owned()];
    bad.summary = Some(EVIL.to_owned());
    bad.cvss = vec![Cvss {
        version: "3".to_owned(),
        vector: Some(EVIL.to_owned()),
        // With no score the renderer shows the vector, so it reaches the screen.
        score: None,
        from: SourceName::Osv,
    }];
    let mut chain = supply_chain(EVIL);
    chain.source_repo = Some(SourceRepo {
        id: EVIL.to_owned(),
        provenance: EVIL.to_owned(),
    });
    chain.deprecated = Some(EVIL.to_owned());
    let mut vulnerable = package("evil-one", Some(EVIL), PackageStatus::Vulnerable);
    vulnerable.version_from = Some(VersionFrom::DepsDevLatest);
    vulnerable.findings = vec![bad];
    vulnerable.supply_chain = Some(chain);
    let mut unchecked = package("evil-two@1.0.0", Some("1.0.0"), PackageStatus::NotChecked);
    unchecked.reason = Some(EVIL.to_owned());
    report(
        Ecosystem::Cargo,
        all_answered(),
        vec![vulnerable, unchecked],
    )
}

#[test]
fn g10_outside_text_reaches_rows_and_descriptions_on_one_line_with_its_controls_as_spaces() {
    let rt = showing(hostile_report(), WINDOW_SIZE);

    let strings = every_string(&rt);
    let tree = rt.accessibility_json();
    let described = row_node(&tree, "evil-one")["description"]
        .as_str()
        .unwrap_or_default()
        .to_owned();

    assert!(has_text(&rt, CLEANED), "the cleaned text is not on screen");
    assert!(described.contains(CLEANED), "{described}");
    assert!(
        row_node(&tree, "evil-two@1.0.0")["description"]
            .as_str()
            .is_some_and(|text| text.contains(CLEANED))
    );
    for text in &strings {
        assert!(
            !text.contains(['\x1b', '\n', '\u{202E}']),
            "an unclean string: {text:?}"
        );
    }
}

#[test]
fn g11_type_check_and_read_the_results() {
    let (state, services) = recording("");
    let mut rt = window(state, WINDOW_SIZE);

    type_into(&mut rt, PACKAGES_KEY, "tokio@1.20.0\nserde\nleft-padd");
    press(&mut rt, CHECK_KEY);
    let requests = services.requests();
    answer(&mut rt, sample_report());

    assert_eq!(requests.len(), 1);
    let inputs: Vec<&str> = requests[0]
        .packages
        .iter()
        .map(|package| package.input.as_str())
        .collect();
    assert_eq!(inputs, ["tokio@1.20.0", "serde", "left-padd"]);
    for words in [
        "Vulnerable",
        "tokio 1.20.0",
        "Clean",
        "serde 1.0.215 (latest)",
        "Not found",
    ] {
        assert!(has_text(&rt, words), "{words} is not on screen");
    }
    assert_eq!(
        label_of(&rt, RESULTS_KEY).as_deref(),
        Some("Results for Cargo")
    );
    assert!(has_key(&rt, SOURCES_KEY));
    assert_eq!(
        label_of(&rt, &status_key("done")).as_deref(),
        Some("3 packages: 1 vulnerable, 1 clean, 1 not found.")
    );
}

#[test]
fn g12_mod_enter_runs_check_from_the_editor() {
    let (state, services) = recording("serde");
    let mut rt = window(state, WINDOW_SIZE);
    press(&mut rt, PACKAGES_KEY);
    assert!(focus_is_inside(&rt, PACKAGES_KEY));

    command(&mut rt, Key::Enter);

    assert_eq!(services.requests().len(), 1);
}

#[test]
fn g12_mod_enter_runs_check_from_the_picker_trigger() {
    let (state, services) = recording("serde");
    let mut rt = window(state, WINDOW_SIZE);
    press(&mut rt, ECOSYSTEM_TRIGGER_KEY);
    press_key(&mut rt, Key::Escape);
    assert!(focus_is_inside(&rt, ECOSYSTEM_TRIGGER_KEY));

    command(&mut rt, Key::Enter);

    assert_eq!(services.requests().len(), 1);
}

#[test]
fn g12_return_in_the_editor_types_a_newline_clears_the_issue_and_runs_nothing() {
    let (state, services) = recording("not a name");
    let mut rt = window(state, WINDOW_SIZE);
    press(&mut rt, CHECK_KEY);
    assert!(has_key(&rt, PACKAGES_ISSUE_KEY));
    assert!(focus_is_inside(&rt, PACKAGES_KEY));

    press_key(&mut rt, Key::Enter);

    assert!(!has_key(&rt, PACKAGES_ISSUE_KEY));
    assert!(rt.state.packages.text.contains('\n'));
    assert!(services.requests().is_empty());
}

#[test]
fn g13_escape_closes_the_open_picker() {
    let (state, _) = recording("serde");
    let mut rt = window(state, WINDOW_SIZE);
    press(&mut rt, ECOSYSTEM_TRIGGER_KEY);
    assert!(rt.state.picker_open);

    press_key(&mut rt, Key::Escape);

    assert!(!rt.state.picker_open);
}

#[test]
fn g13_escape_with_nothing_open_changes_nothing() {
    let (state, services) = recording("serde");
    let mut rt = window(state, WINDOW_SIZE);
    press(&mut rt, PACKAGES_KEY);

    press_key(&mut rt, Key::Escape);

    assert!(!rt.state.picker_open);
    assert_eq!(rt.state.packages.text, "serde");
    assert!(matches!(rt.state.run, Run::Idle));
    assert!(services.requests().is_empty());
}

#[test]
fn g13_picking_an_ecosystem_closes_the_list_and_labels_the_next_check() {
    let (state, services) = recording("left-pad@1.3.0");
    let mut rt = window(state, WINDOW_SIZE);
    press(&mut rt, ECOSYSTEM_TRIGGER_KEY);
    let [x, y, width, height] = text_rect(&rt, "npm");
    rt.pointer_down(x + width / 2.0, y + height / 2.0);
    rt.pointer_up(x + width / 2.0, y + height / 2.0);
    settle(&mut rt);

    press(&mut rt, CHECK_KEY);

    assert!(!rt.state.picker_open);
    assert_eq!(rt.state.ecosystem, Ecosystem::Npm);
    assert_eq!(services.requests()[0].ecosystem, Ecosystem::Npm);
}

/// The sources line of a report whose GitHub block came from an earlier run: two caption lines
/// at the minimum width.
fn report_with_a_saved_block() -> Report {
    let mut partial = sample_report();
    partial.sources[2] = crate::test_support::source(
        SourceName::GitHub,
        SourceStatus::RateLimited {
            until: later(60),
            origin: BlockOrigin::Saved,
        },
    );
    partial
}

/// A report two sources could not reach, each with a 300-character error chain.
fn report_with_long_failures() -> Report {
    let unreachable = || SourceFailure::Unreachable {
        detail: "x".repeat(300),
    };
    let mut partial = sample_report();
    partial.sources[0] = failed(SourceName::Osv, unreachable());
    partial.sources[2] = failed(SourceName::GitHub, unreachable());
    partial
}

#[derive(Debug, Clone, Copy)]
enum Shown {
    Idle,
    Checking,
    DoneWithAWarning,
    DoneWithLongFailures,
    Failed,
    Refused,
    PickerOpen,
}

fn window_in(shown: Shown, size: (f32, f32)) -> Runtime<DepCheckState> {
    let (mut state, _) = recording("tokio@1.20.0\nserde\nleft-padd");
    match shown {
        Shown::Idle => {}
        Shown::Checking => {
            state.apply(Input::CheckPressed);
        }
        Shown::DoneWithAWarning => {
            state.apply(Input::CheckPressed);
            state.apply(Input::Checked(Ok(Box::new(report_with_a_saved_block()))));
        }
        Shown::DoneWithLongFailures => {
            state.apply(Input::CheckPressed);
            state.apply(Input::Checked(Ok(Box::new(report_with_long_failures()))));
        }
        Shown::Failed => {
            state.apply(Input::CheckPressed);
            state.apply(Input::Checked(Err("the service broke. ".repeat(105))));
        }
        Shown::Refused => {
            state.packages = TextEdit::new("serde\nnot a name");
            state.apply(Input::CheckPressed);
        }
        Shown::PickerOpen => state.picker_open = true,
    }
    window(state, size)
}

fn within_window(rt: &Runtime<DepCheckState>, key: &str, size: (f32, f32)) -> bool {
    let [x, y, width, height] = rect(rt, key);
    x >= -0.5 && y >= -0.5 && x + width <= size.0 + 0.5 && y + height <= size.1 + 0.5
}

#[rstest]
#[case::idle_at_the_minimum(Shown::Idle, WINDOW_MIN)]
#[case::done_at_the_minimum(Shown::DoneWithAWarning, WINDOW_MIN)]
#[case::idle_at_the_default(Shown::Idle, WINDOW_SIZE)]
#[case::done_at_the_default(Shown::DoneWithAWarning, WINDOW_SIZE)]
#[case::long_failures_at_the_minimum(Shown::DoneWithLongFailures, WINDOW_MIN)]
fn g14_a_rest_state_does_not_scroll_and_holds_results_inside_the_window(
    #[case] shown: Shown,
    #[case] size: (f32, f32),
) {
    let rt = window_in(shown, size);

    let viewport = rect(&rt, SCROLL_KEY)[3];
    let content = rect(&rt, FORM_KEY)[3];
    let results_bottom = rect(&rt, RESULTS_KEY)[1] + rect(&rt, RESULTS_KEY)[3];

    assert!(
        content <= viewport + 0.5,
        "{shown:?} at {size:?}: the form is {content} tall in a {viewport} viewport"
    );
    assert!(
        results_bottom <= rect(&rt, ACTIONS_KEY)[1] + 0.5,
        "{shown:?} at {size:?}: Results ends at {results_bottom}"
    );
}

#[test]
fn g14_the_minimum_height_holds_under_the_ceiling() {
    assert!(WINDOW_MIN.1 <= 680.0);
    assert!(WINDOW_MIN.0 <= WINDOW_SIZE.0 && WINDOW_MIN.1 <= WINDOW_SIZE.1);
}

/// G14's worst case: a long name and forty findings in the report.
fn crowded_report() -> Report {
    let mut long = package(&"n".repeat(256), Some("1.0.0"), PackageStatus::Vulnerable);
    long.findings = (0..40)
        .map(|index| finding(&format!("ID-{index}"), AdvisoryKind::Vulnerability))
        .collect();
    report(Ecosystem::Cargo, all_answered(), vec![long])
}

#[rstest]
#[case::checking(Shown::Checking)]
#[case::failed(Shown::Failed)]
#[case::refused(Shown::Refused)]
#[case::picker_open(Shown::PickerOpen)]
#[case::idle(Shown::Idle)]
fn g14_with_worst_case_text_check_and_the_picker_can_be_hit(#[case] shown: Shown) {
    for size in [WINDOW_MIN, WINDOW_SIZE] {
        let rt = window_in(shown, size);

        assert!(within_window(&rt, CHECK_KEY, size), "{shown:?} at {size:?}");
        assert!(
            within_window(&rt, ECOSYSTEM_TRIGGER_KEY, size),
            "{shown:?} at {size:?}"
        );
        assert!(
            rect(&rt, ACTIONS_KEY)[3] <= 2.0 * TYPE_CAPTION.line_px() + 2.0 * SPACE_MD + 0.5,
            "{shown:?} at {size:?}: the action row is {} tall",
            rect(&rt, ACTIONS_KEY)[3]
        );
    }
}

#[test]
fn g14_a_crowded_report_keeps_check_on_screen() {
    let rt = showing(crowded_report(), WINDOW_MIN);

    assert!(within_window(&rt, CHECK_KEY, WINDOW_MIN));
    assert!(has_text(&rt, "ID-0"));
}

#[rstest]
#[case::failed(Shown::Failed)]
#[case::refused(Shown::Refused)]
fn g14_the_outer_scroll_reaches_the_results(#[case] shown: Shown) {
    let mut rt = window_in(shown, WINDOW_MIN);

    deliver(&mut rt, |_, tx| {
        tx.scroll_to_bottom(SCROLL_KEY);
        true
    });

    let [_, view_y, _, view_h] = rect(&rt, SCROLL_KEY);
    let [_, results_y, _, results_h] = rect(&rt, RESULTS_KEY);
    assert!(
        results_y >= view_y - 0.5 && results_y + results_h <= view_y + view_h + 0.5,
        "{shown:?}: Results at {results_y}+{results_h}, the viewport at {view_y}+{view_h}"
    );
}

#[test]
fn a_cut_sources_line_shows_140_characters_and_describes_the_whole_line() {
    let rt = window_in(Shown::DoneWithLongFailures, WINDOW_SIZE);

    let shown = rt
        .elements()
        .into_iter()
        .find(|element| element.key.as_deref() == Some(SOURCES_KEY))
        .and_then(|element| element.text)
        .expect("the sources caption");
    let tree = rt.accessibility_json();
    let group = a11y_node(&tree, SOURCES_GROUP_KEY).expect("the cut caption's group");

    assert!(
        shown.chars().count() <= 141,
        "{} characters",
        shown.chars().count()
    );
    assert!(shown.ends_with('…'), "{shown}");
    assert_eq!(group["name"].as_str(), Some(shown.as_str()));
    assert!(
        group["description"]
            .as_str()
            .is_some_and(|whole| whole.matches(&"x".repeat(300)).count() == 2),
        "{group}"
    );
}

#[test]
fn a_long_not_checked_reason_is_cut_in_the_row_and_whole_in_its_description() {
    let mut skipped = package("skipped@1.0.0", Some("1.0.0"), PackageStatus::NotChecked);
    skipped.reason = Some(format!("no source answered: {}", "y".repeat(400)));
    // A checked package beside it, so the report is on screen rather than a failure.
    let fine = package("fine@1.0.0", Some("1.0.0"), PackageStatus::Clean);
    let rt = showing(
        report(Ecosystem::Cargo, all_answered(), vec![skipped, fine]),
        WINDOW_SIZE,
    );

    let shown = rt
        .elements()
        .into_iter()
        .filter_map(|element| element.text)
        .find(|text| text.starts_with("no source answered"))
        .expect("the reason line");
    let tree = rt.accessibility_json();

    assert!(
        shown.chars().count() <= 141,
        "{} characters",
        shown.chars().count()
    );
    assert!(
        row_node(&tree, "skipped@1.0.0")["description"]
            .as_str()
            .is_some_and(|whole| whole.contains(&"y".repeat(400)))
    );
}

#[test]
fn g6_a_check_no_service_answered_shows_the_next_step_and_describes_the_whole_message() {
    let rt = showing(all_blocked_report(), WINDOW_SIZE);

    let tree = rt.accessibility_json();
    let status = a11y_node(&tree, &status_key("failed")).expect("the status line");
    let Run::Failed { message, .. } = &rt.state.run else {
        panic!("not failed");
    };

    assert!(message.chars().count() > 140, "{message}");
    assert!(
        status["name"].as_str().is_some_and(|name| name
            .starts_with("No service answered. Try again after 2026-11-26 14:30:00 UTC.")),
        "{status}"
    );
    assert_eq!(status["description"].as_str(), Some(message.as_str()));
}

#[test]
fn tab_never_stops_inside_a_result_row() {
    let mut rt = showing(sample_report(), WINDOW_SIZE);
    press(&mut rt, ECOSYSTEM_TRIGGER_KEY);
    press_key(&mut rt, Key::Escape);
    let mut stops = Vec::new();

    for _ in 0..8 {
        press_key(&mut rt, Key::Tab);
        stops.push(common_gui_rune::test_harness::focused_key(&rt));
        assert!(
            !focus_is_inside(&rt, RESULTS_KEY),
            "focus stopped in Results after {stops:?}"
        );
    }

    let mut distinct = stops.clone();
    distinct.sort();
    distinct.dedup();
    assert!(distinct.len() >= 2, "Tab did not move focus: {stops:?}");
}

#[test]
fn the_window_s_first_focus_is_packages() {
    let mut rt = window(DepCheckState::detached(), WINDOW_SIZE);

    deliver(&mut rt, shown);

    assert!(focus_is_inside(&rt, PACKAGES_KEY));
}

#[test]
fn set_appearance_says_whether_it_changed_anything() {
    let mut s = DepCheckState::detached();

    let first = set_appearance(&mut s, false);
    let again = set_appearance(&mut s, false);

    assert!(first);
    assert!(!again);
}

#[test]
fn g15_every_focusable_node_is_named_and_rows_and_status_read_right() {
    let rt = showing(sample_report(), WINDOW_SIZE);

    let tree = rt.accessibility_json();
    let unnamed: Vec<&Value> = a11y_nodes(&tree)
        .into_iter()
        .filter(|node| {
            states_of(node).iter().any(|state| state == "focusable")
                && node["name"].as_str().is_none_or(str::is_empty)
        })
        .collect();
    let status = a11y_node(&tree, &status_key("done")).expect("the status line");

    assert!(unnamed.is_empty(), "{unnamed:?}");
    assert_eq!(
        row_node(&tree, "tokio@1.20.0")["name"],
        "Vulnerable, tokio 1.20.0"
    );
    assert_eq!(
        row_node(&tree, "serde")["name"],
        "Clean, serde 1.0.215 (latest)"
    );
    assert_eq!(row_node(&tree, "left-padd")["name"], "Not found, left-padd");
    assert_eq!(status["role"], "status");
    assert!(states_of(status).iter().any(|state| state == "live-polite"));
}

#[test]
fn g15_a_malware_finding_reads_malicious() {
    let mut bad = package("evil@1.0.0", Some("1.0.0"), PackageStatus::Vulnerable);
    bad.findings = vec![finding("MAL-1", AdvisoryKind::Malware)];
    let rt = showing(
        report(Ecosystem::Npm, all_answered(), vec![bad]),
        WINDOW_SIZE,
    );

    let tree = rt.accessibility_json();

    assert_eq!(
        row_node(&tree, "evil@1.0.0")["name"],
        "Malicious, evil 1.0.0"
    );
}

#[test]
fn g15_a_source_that_did_not_fully_answer_puts_the_caption_in_the_warning_tone() {
    let mut blocked = sample_report();
    blocked.sources[0] = rate_limited(SourceName::Osv, later(30));
    let warned = showing(blocked, WINDOW_SIZE);
    let calm = showing(sample_report(), WINDOW_SIZE);

    let colour = |rt: &Runtime<DepCheckState>| {
        rt.elements()
            .into_iter()
            .find(|element| element.key.as_deref() == Some(SOURCES_KEY))
            .and_then(|element| element.text_color)
    };

    assert_ne!(colour(&warned), colour(&calm));
}

fn many_packages(count: usize) -> Report {
    let packages = (0..count)
        .map(|index| {
            let mut row = package(
                &format!("crate{index}@1.0.0"),
                Some("1.0.0"),
                PackageStatus::Clean,
            );
            row.supply_chain = Some(supply_chain("MIT"));
            row
        })
        .collect();
    report(Ecosystem::Cargo, all_answered(), packages)
}

fn mounted_rows(rt: &Runtime<DepCheckState>) -> usize {
    let tree = rt.accessibility_json();
    a11y_nodes(&tree)
        .into_iter()
        .filter(|node| {
            node["key"]
                .as_str()
                .is_some_and(|key| key.starts_with("dep-check-row-crate"))
        })
        .count()
}

#[test]
fn g16_a_thousand_rows_build_only_the_ones_in_view() {
    let mut rt = showing(many_packages(1_000), WINDOW_SIZE);
    let tree = rt.accessibility_json();
    assert_eq!(
        row_node(&tree, "crate0@1.0.0")["name"],
        "Clean, crate0 1.0.0"
    );
    let at_the_top = mounted_rows(&rt);

    let viewport = format!("{RESULTS_KEY}-scroll");
    deliver(&mut rt, move |_, tx| {
        tx.scroll_to_bottom(viewport);
        true
    });
    settle(&mut rt);

    assert!(has_key(&rt, &row_key("crate999@1.0.0")));
    assert!(at_the_top < 50, "{at_the_top} rows at the top");
    assert!(
        mounted_rows(&rt) < 50,
        "{} rows at the end",
        mounted_rows(&rt)
    );
}

#[test]
fn g16_a_new_report_replaces_the_rows() {
    let mut rt = showing(many_packages(3), WINDOW_SIZE);
    rt.state.apply(Input::CheckPressed);

    answer(&mut rt, sample_report());

    assert!(!has_key(&rt, &row_key("crate0@1.0.0")));
    assert!(has_key(&rt, &row_key("tokio@1.20.0")));
}

#[test]
fn g17_every_badge_label_fits_the_badge_column() {
    let mut malicious = package("bad@1.0.0", Some("1.0.0"), PackageStatus::Vulnerable);
    malicious.findings = vec![finding("MAL-1", AdvisoryKind::Malware)];
    let mut vulnerable = package("old@1.0.0", Some("1.0.0"), PackageStatus::Vulnerable);
    vulnerable.findings = vec![finding("ID-1", AdvisoryKind::Vulnerability)];
    let packages = vec![
        malicious,
        vulnerable,
        package("fine@1.0.0", Some("1.0.0"), PackageStatus::Clean),
        package("gone@1.0.0", Some("1.0.0"), PackageStatus::NotFound),
        package("skipped@1.0.0", Some("1.0.0"), PackageStatus::NotChecked),
    ];
    let rt = showing(
        report(Ecosystem::Cargo, all_answered(), packages),
        WINDOW_SIZE,
    );

    for label in [
        "Malicious",
        "Vulnerable",
        "Clean",
        "Not found",
        "Not checked",
    ] {
        let width = text_rect(&rt, label)[2];
        assert!(
            width + 2.0 * SPACE_XS <= BADGE_WIDTH,
            "{label} is {width} wide, the column {BADGE_WIDTH}"
        );
    }
}

/// The median, 95th percentile, and slowest of `frames`.
fn spread(mut frames: Vec<Duration>) -> String {
    frames.sort();
    let at = |share: f64| frames[((frames.len() - 1) as f64 * share) as usize];
    format!(
        "median {:?}, p95 {:?}, max {:?}",
        at(0.5),
        at(0.95),
        at(1.0)
    )
}

/// G20: a 1,000-row report scrolled end to end at the minimum size, one viewport per frame. Run
/// in a release build: `cargo test -p dep-check-gui --release --lib -- --ignored --nocapture g20`.
#[test]
#[ignore = "a release-build measurement, recorded in the PR"]
fn g20_a_thousand_rows_scrolled_end_to_end_measured_frame_by_frame() {
    let mut rt = showing(many_packages(1_000), WINDOW_MIN);
    let [x, y, width, height] = rect(&rt, &format!("{RESULTS_KEY}-scroll"));
    let steps = (1_000.0 * RESULT_ROW_ESTIMATE / height).ceil() as usize + 10;

    let frames = (0..steps)
        .map(|_| {
            rt.wheel(x + width / 2.0, y + height / 2.0, -height);
            let started = Instant::now();
            rt.frame(common_gui_rune::test_harness::FRAME);
            started.elapsed()
        })
        .collect();

    eprintln!(
        "g20 1,000 rows at {WINDOW_MIN:?}: scrolling {}",
        spread(frames)
    );
    assert!(
        has_key(&rt, &row_key("crate999@1.0.0")),
        "the scroll did not reach the end"
    );
}

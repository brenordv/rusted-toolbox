use super::*;
use crate::test_support::{
    all_blocked_report, failed, later, package, rate_limited, recording, report, sample_report,
};
use dep_check_core::advisory::SourceName;
use dep_check_core::http::SourceFailure;
use dep_check_core::report::PackageStatus;
use rstest::rstest;

#[derive(Debug, Clone, Copy)]
enum Start {
    Idle,
    Done,
    Failed,
}

/// A window in `start` with `packages` in the editor, its last line shown.
fn state_in(start: Start, packages: &str) -> DepCheckState {
    let (mut s, _) = recording("serde");
    match start {
        Start::Idle => {}
        Start::Done => {
            s.apply(Input::CheckPressed);
            s.apply(Input::Checked(Ok(Box::new(sample_report()))));
        }
        Start::Failed => {
            s.apply(Input::CheckPressed);
            s.apply(Input::Checked(Err("the task died".to_owned())));
        }
    }
    s.packages = TextEdit::new(packages);
    s
}

fn checking(packages: &str) -> DepCheckState {
    let mut s = state_in(Start::Idle, packages);
    s.apply(Input::CheckPressed);
    s
}

fn the_request(transition: &Transition) -> &CheckRequest {
    match transition.effects.as_slice() {
        [Effect::Check(request)] => request,
        effects => panic!("expected one check, got {effects:?}"),
    }
}

#[rstest]
#[case::idle(Start::Idle)]
#[case::done(Start::Done)]
#[case::failed(Start::Failed)]
fn a_press_with_valid_packages_starts_a_check_of_them_in_order(#[case] start: Start) {
    let mut s = state_in(start, "tokio@1.20.0\n  serde\n\ntokio@1.20.0\n");

    let transition = s.apply(Input::CheckPressed);

    let request = the_request(&transition);
    let inputs: Vec<&str> = request
        .packages
        .iter()
        .map(|package| package.input.as_str())
        .collect();
    assert_eq!(request.ecosystem, Ecosystem::Cargo);
    assert_eq!(inputs, ["tokio@1.20.0", "serde"]);
    assert!(transition.changed);
    assert_eq!(transition.focus, None);
    assert!(matches!(
        s.run,
        Run::Checking {
            ecosystem: Ecosystem::Cargo,
            packages: 2
        }
    ));
    assert_eq!(
        s.status().map(|words| words.text),
        Some("Checking 2 packages…".to_owned())
    );
}

#[test]
fn a_press_while_checking_changes_nothing_and_asks_for_nothing() {
    let mut s = checking("serde");

    let transition = s.apply(Input::CheckPressed);

    assert!(!transition.changed);
    assert!(transition.effects.is_empty());
    assert!(matches!(s.run, Run::Checking { packages: 1, .. }));
}

#[test]
fn a_press_with_a_bad_line_marks_it_moves_focus_and_hides_the_last_line() {
    let mut s = state_in(Start::Done, "serde\nnot a name");

    let transition = s.apply(Input::CheckPressed);

    assert!(transition.effects.is_empty());
    assert_eq!(transition.focus, Some(PACKAGES_KEY));
    let issue = s.packages_issue.as_ref().expect("an issue");
    assert_eq!(issue.line, 2);
    assert!(issue_words(issue).starts_with("Line 2: "), "{issue:?}");
    assert!(matches!(
        s.run,
        Run::Done {
            line_shown: false,
            ..
        }
    ));
    assert_eq!(
        s.status(),
        Some(StatusWords {
            kind: "refused",
            tone: Tone::Danger,
            text: "Fix line 2 of Packages to check.".to_owned()
        })
    );
}

#[test]
fn a_press_with_no_packages_asks_for_one() {
    let mut s = state_in(Start::Idle, " \n\n");

    let transition = s.apply(Input::CheckPressed);

    assert_eq!(transition.focus, Some(PACKAGES_KEY));
    let issue = s.packages_issue.as_ref().expect("an issue");
    assert_eq!(
        issue_words(issue),
        "Packages is empty: type one package per line."
    );
    assert_eq!(
        s.status().map(|words| words.text),
        Some("Type a package in Packages to check.".to_owned())
    );
    assert!(matches!(s.run, Run::Idle));
}

#[rstest]
#[case::done(Start::Done)]
#[case::failed(Start::Failed)]
fn an_edit_clears_the_issue_and_hides_the_last_line(#[case] start: Start) {
    let mut s = state_in(start, "not a name");
    s.apply(Input::CheckPressed);
    assert!(s.packages_issue.is_some());

    let transition = s.apply(Input::PackagesEdited);

    assert!(transition.changed);
    assert!(s.packages_issue.is_none());
    assert_eq!(s.status(), None);
}

#[rstest]
#[case::done(Start::Done)]
#[case::failed(Start::Failed)]
fn an_edit_hides_a_shown_outcome_line(#[case] start: Start) {
    let mut s = state_in(start, "serde");
    assert!(s.status().is_some());

    let transition = s.apply(Input::PackagesEdited);

    assert!(transition.changed);
    assert_eq!(s.status(), None);
}

#[rstest]
#[case::done(Start::Done)]
#[case::failed(Start::Failed)]
fn a_pick_of_another_ecosystem_hides_a_shown_outcome_line(#[case] start: Start) {
    let mut s = state_in(start, "serde");
    assert!(s.status().is_some());

    let transition = s.apply(Input::EcosystemPicked(Ecosystem::Pypi));

    assert!(transition.changed);
    assert_eq!(s.status(), None);
}

#[test]
fn a_pick_of_the_same_ecosystem_only_closes_the_list() {
    let mut s = state_in(Start::Done, "serde\nnot a name");
    s.apply(Input::CheckPressed);
    s.picker_open = true;
    let refused = s.status();

    let transition = s.apply(Input::EcosystemPicked(Ecosystem::Cargo));

    assert!(transition.changed);
    assert!(!s.picker_open);
    assert!(s.packages_issue.is_some());
    assert_eq!(s.status(), refused);
}

#[test]
fn the_results_are_about_the_running_check_s_ecosystem_then_the_report_s() {
    let mut s = state_in(Start::Idle, "serde");
    assert_eq!(s.results_ecosystem(), None);

    s.apply(Input::CheckPressed);
    s.apply(Input::EcosystemPicked(Ecosystem::Npm));
    let while_checking = s.results_ecosystem();
    s.apply(Input::Checked(Ok(Box::new(sample_report()))));

    assert_eq!(while_checking, Some(Ecosystem::Cargo));
    assert_eq!(s.results_ecosystem(), Some(Ecosystem::Cargo));
}

#[test]
fn more_than_a_thousand_packages_asks_for_smaller_lists() {
    let lines: Vec<String> = (0..1_001).map(|index| format!("crate{index}")).collect();
    let mut s = state_in(Start::Idle, &lines.join("\n"));

    s.apply(Input::CheckPressed);

    let issue = s.packages_issue.as_ref().expect("an issue");
    assert_eq!(
        issue_words(issue),
        "More than 1,000 packages: check them in smaller lists."
    );
    assert_eq!(
        s.status().map(|words| words.text),
        Some("Check at most 1,000 packages at a time.".to_owned())
    );
}

#[test]
fn a_bad_line_past_999_is_named_with_a_separator() {
    let mut lines: Vec<String> = (0..1_200).map(|_| String::new()).collect();
    lines.push("not a name".to_owned());
    let mut s = state_in(Start::Idle, &lines.join("\n"));

    s.apply(Input::CheckPressed);

    assert_eq!(
        s.status().map(|words| words.text),
        Some("Fix line 1,201 of Packages to check.".to_owned())
    );
}

#[test]
fn an_edit_with_nothing_to_hide_changes_nothing() {
    let mut s = state_in(Start::Idle, "serde");

    assert!(!s.apply(Input::PackagesEdited).changed);
}

#[test]
fn a_report_while_checking_is_done_with_its_line_shown() {
    let mut s = checking("tokio@1.20.0\nserde\nleft-padd");

    let transition = s.apply(Input::Checked(Ok(Box::new(sample_report()))));

    assert!(transition.changed);
    assert!(matches!(
        s.run,
        Run::Done {
            line_shown: true,
            ..
        }
    ));
    assert_eq!(s.answers, 1);
    assert_eq!(
        s.status().map(|words| words.text),
        Some("3 packages: 1 vulnerable, 1 clean, 1 not found.".to_owned())
    );
}

#[test]
fn a_report_some_source_did_not_fully_answer_points_at_results() {
    let mut s = checking("serde");
    let mut partial = sample_report();
    partial.sources[2] = rate_limited(SourceName::GitHub, later(30));

    s.apply(Input::Checked(Ok(Box::new(partial))));

    assert_eq!(
        s.status().map(|words| words.text),
        Some(
            "3 packages: 1 vulnerable, 1 clean, 1 not found. Not every source answered; see Results."
                .to_owned()
        )
    );
}

#[test]
fn a_stopped_check_is_failed_with_the_reason() {
    let mut s = checking("serde");

    s.apply(Input::Checked(Err(
        "The check task stopped unexpectedly: boom".to_owned(),
    )));

    assert_eq!(
        s.status(),
        Some(StatusWords {
            kind: "failed",
            tone: Tone::Danger,
            text: "The check stopped. Press Check again. The check task stopped unexpectedly: boom"
                .to_owned()
        })
    );
}

#[rstest]
#[case::idle(Start::Idle)]
#[case::done(Start::Done)]
#[case::failed(Start::Failed)]
fn an_answer_with_nothing_pending_is_ignored(#[case] start: Start) {
    let mut s = state_in(start, "serde");
    let before = s.status();

    let transition = s.apply(Input::Checked(Ok(Box::new(sample_report()))));

    assert!(!transition.changed);
    assert_eq!(s.status(), before);
}

#[test]
fn every_source_blocked_says_when_to_try_again_then_each_reason() {
    let mut s = checking("tokio@1.20.0");

    s.apply(Input::Checked(Ok(Box::new(all_blocked_report()))));

    let Run::Failed { message, .. } = &s.run else {
        panic!("not failed");
    };
    assert_eq!(
        message,
        "No service answered. Try again after 2026-11-26 14:30:00 UTC. \
         OSV.dev is rate limited until 2026-11-26 15:00:00 UTC; \
         deps.dev is rate limited until 2026-11-26 14:30:00 UTC; \
         GitHub Advisory Database is rate limited until 2026-11-26 15:30:00 UTC."
    );
    assert!(s.report().is_none());
}

#[test]
fn a_source_that_failed_says_to_check_the_connection_and_leaves_out_a_source_not_asked() {
    let mut s = checking("tokio@1.20.0");
    let mut tokio = package("tokio@1.20.0", Some("1.20.0"), PackageStatus::NotChecked);
    tokio.reason = Some("no source answered".to_owned());
    let down = report(
        Ecosystem::Cargo,
        vec![
            failed(SourceName::Osv, SourceFailure::TimedOut),
            rate_limited(SourceName::DepsDev, later(30)),
            crate::test_support::source(SourceName::GitHub, SourceStatus::NotAsked),
        ],
        vec![tokio],
    );

    s.apply(Input::Checked(Ok(Box::new(down))));

    let Run::Failed { message, .. } = &s.run else {
        panic!("not failed");
    };
    assert_eq!(
        message,
        "No service answered. Check the connection, then press Check again. \
         OSV.dev did not answer in time; deps.dev is rate limited until 2026-11-26 14:30:00 UTC."
    );
}

#[test]
fn a_pick_clears_the_issue_both_fields_feed_and_hides_the_line() {
    let mut s = state_in(Start::Done, "@types/node@20.1.0");
    s.apply(Input::CheckPressed);
    s.picker_open = true;
    assert!(s.packages_issue.is_some());

    let transition = s.apply(Input::EcosystemPicked(Ecosystem::Npm));

    assert!(transition.changed);
    assert_eq!(s.ecosystem, Ecosystem::Npm);
    assert!(!s.picker_open);
    assert!(s.packages_issue.is_none());
    assert_eq!(s.status(), None);
    let request = the_request(&s.apply(Input::CheckPressed)).clone();
    assert_eq!(request.ecosystem, Ecosystem::Npm);
}

#[test]
fn a_pick_while_checking_leaves_the_check_and_its_report_on_its_ecosystem() {
    let mut s = checking("serde");

    s.apply(Input::EcosystemPicked(Ecosystem::Pypi));

    assert!(matches!(
        s.run,
        Run::Checking {
            ecosystem: Ecosystem::Cargo,
            ..
        }
    ));
    s.apply(Input::Checked(Ok(Box::new(sample_report()))));
    assert_eq!(s.ecosystem, Ecosystem::Pypi);
    assert_eq!(
        s.report().map(|report| report.ecosystem),
        Some(Ecosystem::Cargo)
    );
}

#[test]
fn an_edit_during_a_check_does_not_hide_the_line_its_answer_brings() {
    let mut s = checking("serde");

    s.apply(Input::PackagesEdited);
    s.apply(Input::Checked(Ok(Box::new(sample_report()))));

    assert!(matches!(
        s.run,
        Run::Done {
            line_shown: true,
            ..
        }
    ));
}

#[test]
fn the_debug_of_an_input_and_an_effect_holds_counts_not_package_names() {
    let mut s = state_in(Start::Idle, "zebra-secret-name@1.0.0");
    let transition = s.apply(Input::CheckPressed);

    let effect = format!("{:?}", transition.effects);
    let input = format!("{:?}", Input::Checked(Ok(Box::new(sample_report()))));

    assert!(effect.contains("1 packages"), "{effect}");
    assert!(!effect.contains("zebra"), "{effect}");
    assert!(input.contains("3 packages"), "{input}");
    assert!(!input.contains("tokio"), "{input}");
}

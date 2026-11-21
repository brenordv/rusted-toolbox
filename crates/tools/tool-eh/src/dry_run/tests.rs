use super::*;
use common_cli::test_writers::{ClosedPipe, FailingDisk};

const CAP: usize = 1 << 20;

fn template(source: &str) -> Template {
    Template::compile(source).unwrap()
}

fn printed(source: &str, count: u64) -> (Printed, String) {
    let mut out = Vec::new();
    let printed = print_renders(
        &template(source),
        count,
        CAP,
        &mut out,
        &AtomicBool::new(false),
    )
    .unwrap();
    (printed, String::from_utf8(out).unwrap())
}

#[test]
fn each_event_prints_with_a_blank_line_between_events() {
    let (printed, out) = printed("{\"n\": {{random.integer min=1 max=1}}}", 3);

    assert_eq!(printed, Printed::All);
    assert_eq!(out, "{\"n\": 1}\n\n{\"n\": 1}\n\n{\"n\": 1}\n");
}

#[test]
fn each_event_shows_its_index_as_the_sequence() {
    let (_, out) = printed("{{sequence}}", 3);

    assert_eq!(out, "0\n\n1\n\n2\n");
}

#[test]
fn a_two_line_template_prints_two_lines_per_event() {
    let (_, out) = printed("{\n  \"n\": {{random.integer min=2 max=2}}\n}", 2);

    assert_eq!(out, "{\n  \"n\": 2\n}\n\n{\n  \"n\": 2\n}\n");
}

#[test]
fn a_crlf_template_prints_no_carriage_returns() {
    let (_, out) = printed("a\r\nb {{random.integer min=3 max=3}}\r\n", 1);

    assert_eq!(out, "a\nb 3\n");
}

#[test]
fn control_bytes_are_escaped_line_by_line() {
    let (_, out) = printed("x\u{1b}]52;c;y\u{7}\nnext", 1);

    assert_eq!(out, "x\\u{1b}]52;c;y\\u{7}\nnext\n");
}

#[test]
fn a_set_shutdown_flag_stops_before_the_next_event() {
    let mut out = Vec::new();

    let printed = print_renders(&template("x"), 5, CAP, &mut out, &AtomicBool::new(true)).unwrap();

    assert_eq!(printed, Printed::Interrupted(0));
    assert!(out.is_empty());
}

#[test]
fn a_closed_reader_ends_the_run_quietly() {
    let printed =
        print_renders(&template("x"), 5, CAP, ClosedPipe, &AtomicBool::new(false)).unwrap();

    assert_eq!(printed, Printed::ReaderClosed(0));
}

#[test]
fn any_other_write_error_fails_the_run() {
    let error =
        print_renders(&template("x"), 5, CAP, FailingDisk, &AtomicBool::new(false)).unwrap_err();

    assert!(
        error.to_string().contains("Failed to write the dry run"),
        "{error:#}"
    );
}

#[rstest::rstest]
#[case::all(
    Printed::All,
    "Dry run: 5 event(s) rendered, nothing sent.",
    RunOutcome::Completed
)]
#[case::interrupted(
    Printed::Interrupted(2),
    "Dry run interrupted: 2 of 5 event(s) rendered, nothing sent.",
    RunOutcome::Interrupted
)]
#[case::reader_closed(
    Printed::ReaderClosed(3),
    "Dry run: output closed after 3 event(s), nothing sent.",
    RunOutcome::Completed
)]
fn each_ending_has_its_summary_and_outcome(
    #[case] printed: Printed,
    #[case] summary: &str,
    #[case] outcome: RunOutcome,
) {
    assert_eq!(ending(&printed, 5), (summary.to_owned(), outcome));
}

#[test]
fn a_render_error_names_the_event_index_and_writes_nothing_for_it() {
    let mut out = Vec::new();

    let error = print_renders(
        &template("{{internet.password length=64}}"),
        3,
        16,
        &mut out,
        &AtomicBool::new(false),
    )
    .unwrap_err();

    assert_eq!(
        format!("{error:#}"),
        "event 0: line 1, column 1: internet.password: the rendered body passes the 16-byte limit"
    );
    assert!(out.is_empty());
}

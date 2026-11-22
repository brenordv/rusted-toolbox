use super::*;
use chrono::TimeZone;
use rstest::rstest;

fn at(hour: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 7, 2, hour, 0, 0).unwrap()
}

fn draft(mode: TimeMode, start: &str, end: &str) -> RulesForm {
    RulesForm {
        time_mode: mode,
        time_start: TextEdit::new(start),
        time_end: TextEdit::new(end),
        ..RulesForm::default()
    }
}

fn text_draft(mode: TextMode, text: &str) -> RulesForm {
    RulesForm {
        text_mode: mode,
        text: TextEdit::new(text),
        ..RulesForm::default()
    }
}

#[rstest]
#[case::older(TimeMode::OlderThan, TimeFilter::OlderThan(at(12)))]
#[case::older_or_equal(TimeMode::OlderOrEqual, TimeFilter::OlderOrEqual(at(12)))]
#[case::newer(TimeMode::NewerThan, TimeFilter::NewerThan(at(12)))]
#[case::newer_or_equal(TimeMode::NewerOrEqual, TimeFilter::NewerOrEqual(at(12)))]
#[case::during(TimeMode::During, TimeFilter::During(at(12)))]
fn each_single_time_mode_maps_to_its_filter(#[case] mode: TimeMode, #[case] expected: TimeFilter) {
    let form = draft(mode, "2026-07-02T12:00:00Z", "");

    assert_eq!(form.parsed().unwrap().time, Some(expected));
}

#[test]
fn between_maps_to_an_inclusive_window() {
    let form = draft(TimeMode::Between, "2026-07-02", "2026-07-02T03:00:00Z");

    assert_eq!(
        form.parsed().unwrap().time,
        Some(TimeFilter::Between {
            start: at(0),
            end: at(3)
        })
    );
}

#[test]
fn between_with_the_end_first_names_the_end_field() {
    let issues = draft(TimeMode::Between, "2026-07-02T03:00:00Z", "2026-07-02")
        .parsed()
        .unwrap_err();

    assert_eq!(
        issues.time_end.as_deref(),
        Some("The end is before the start")
    );
    assert!(issues.time_start.is_none());
}

#[test]
fn during_refuses_a_date_alone() {
    let issues = draft(TimeMode::During, "2026-07-02", "")
        .parsed()
        .unwrap_err();

    assert!(issues.time_start.unwrap().contains("first hour"));
}

#[rstest]
#[case::blank("", "Enter a time")]
#[case::garbage("noon", "Expected an RFC3339")]
fn a_bad_time_names_the_start_field(#[case] value: &str, #[case] expected: &str) {
    let issues = draft(TimeMode::NewerThan, value, "").parsed().unwrap_err();

    assert!(issues.time_start.unwrap().starts_with(expected));
}

#[test]
fn contains_splits_trimmed_keywords_and_drops_empties() {
    let form = text_draft(TextMode::Contains, " bacon, ,eggs ");

    assert_eq!(
        form.parsed().unwrap().text,
        Some(TextFilter::Contains(vec![
            "bacon".to_owned(),
            "eggs".to_owned()
        ]))
    );
}

#[test]
fn contains_without_keywords_is_an_issue() {
    let issues = text_draft(TextMode::Contains, " , ").parsed().unwrap_err();

    assert!(issues.text.is_some());
}

#[test]
fn a_regex_compiles_through_the_capture_filter() {
    let form = text_draft(TextMode::Regex, r"order-\d+");

    assert_eq!(
        form.parsed().unwrap().text,
        Some(TextFilter::regex(r"order-\d+").unwrap())
    );
}

#[test]
fn an_invalid_regex_names_the_text_field() {
    let issues = text_draft(TextMode::Regex, "(unclosed")
        .parsed()
        .unwrap_err();

    assert!(issues.text.unwrap().contains("Invalid text filter regex"));
}

#[test]
fn a_regex_past_the_length_limit_is_refused() {
    let issues = text_draft(TextMode::Regex, &"a".repeat(MAX_PATTERN_CHARS + 1))
        .parsed()
        .unwrap_err();

    assert!(issues.text.unwrap().contains("limited"));
}

#[test]
fn no_modes_parse_to_no_rules() {
    assert_eq!(
        RulesForm::default().parsed().unwrap(),
        CaptureFilters::default()
    );
}

#[test]
fn an_edit_is_parsed_again() {
    let mut form = text_draft(TextMode::Regex, "(unclosed");
    assert!(form.parsed().is_err());

    form.text.replace_text("closed");

    assert!(form.parsed().is_ok());
}

#[rstest]
#[case::time(CaptureFilters { time: Some(TimeFilter::Between { start: at(1), end: at(2) }), text: None })]
#[case::contains(CaptureFilters { time: None, text: Some(TextFilter::Contains(vec!["a".to_owned(), "b".to_owned()])) })]
#[case::regex(CaptureFilters { time: Some(TimeFilter::During(at(5))), text: Some(TextFilter::regex("x+").unwrap()) })]
fn from_filters_round_trips(#[case] filters: CaptureFilters) {
    assert_eq!(RulesForm::from_filters(&filters).parsed().unwrap(), filters);
}

#[test]
fn the_rule_count_counts_set_conditions() {
    let filters = text_draft(TextMode::Contains, "bacon").parsed().unwrap();

    assert_eq!(rule_count(&filters), 1);
    assert_eq!(rule_count(&CaptureFilters::default()), 0);
}

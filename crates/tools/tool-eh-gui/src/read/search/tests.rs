use super::*;
use crate::read::test_support::{message, message_with_ids};

fn buffer(bodies: &[&str]) -> RowBuffer {
    let mut rows = RowBuffer::default();
    for (sequence, body) in bodies.iter().enumerate() {
        rows.push(message("0", sequence as i64, body));
    }
    rows.flush();
    rows
}

fn contains(text: &str) -> Matcher {
    Matcher::build(text, false).unwrap().unwrap()
}

fn active(rebuilding: u64) -> SearchState {
    SearchState::Active {
        last_good: contains("bacon"),
        visible: Vec::new(),
        rebuilding: Some(rebuilding),
    }
}

fn rebuilt(generation: u64, result: Result<Vec<u64>, String>, upto: Option<u64>) -> Rebuilt {
    Rebuilt {
        generation,
        matcher: contains("bacon"),
        result,
        upto,
        follow_up: false,
        scanned: 0,
        elapsed_ms: 0,
    }
}

fn visible(search: &SearchState) -> Vec<u64> {
    match search {
        SearchState::Active { visible, .. } => visible.clone(),
        SearchState::Off => panic!("the search is off"),
    }
}

#[test]
fn contains_ignores_case_and_finds_a_match_past_the_preview_cut() {
    let body = format!("{}BaCoN", "x".repeat(500));

    assert!(contains("bacon").matches(&message("0", 1, &body)));
}

#[test]
fn contains_also_looks_at_the_message_and_correlation_ids() {
    let found = message_with_ids("0", 1, "eggs", Some("order-7"), Some("trace-bacon"));

    assert!(contains("bacon").matches(&found));
    assert!(contains("ORDER-7").matches(&found));
}

#[test]
fn a_regex_matches_case_insensitively() {
    let matcher = Matcher::build(r"order-\d+", true).unwrap().unwrap();

    assert!(matcher.matches(&message("0", 1, "ORDER-42 placed")));
    assert!(!matcher.matches(&message("0", 2, "order-x")));
}

#[test]
fn blank_text_is_no_search() {
    assert!(Matcher::build("   ", true).unwrap().is_none());
}

#[test]
fn an_invalid_regex_returns_the_field_error() {
    let error = Matcher::build("(unclosed", true).unwrap_err();

    assert!(error.starts_with("Invalid regular expression"), "{error}");
}

#[test]
fn a_pattern_past_the_length_limit_is_refused_before_compiling() {
    let error = Matcher::build(&"a".repeat(MAX_PATTERN_CHARS + 1), true).unwrap_err();

    assert!(error.contains("limited"), "{error}");
}

#[test]
fn a_scan_returns_the_matching_indices() {
    let rows = buffer(&["bacon", "eggs", "more bacon"]);
    let request = RebuildRequest {
        generation: Arc::new(AtomicU64::new(3)),
        current: 3,
        matcher: contains("bacon"),
        rows: snapshot(&rows),
        upto: rows.last_displayed(),
        follow_up: false,
    };

    assert_eq!(request.scan(), Some(vec![0, 2]));
}

#[test]
fn a_scan_whose_search_moved_on_stops_without_a_result() {
    let rows = buffer(&["bacon"; 200]);
    let generation = Arc::new(AtomicU64::new(4));
    let request = RebuildRequest {
        generation: Arc::clone(&generation),
        current: 3,
        matcher: contains("bacon"),
        rows: snapshot(&rows),
        upto: rows.last_displayed(),
        follow_up: false,
    };

    assert_eq!(request.scan(), None);
}

#[test]
fn a_result_for_another_generation_is_dropped() {
    let rows = buffer(&["bacon"]);
    let mut search = active(2);

    let landing = search.land(
        rebuilt(1, Ok(vec![0]), Some(0)),
        2,
        &rows,
        &Arc::new(AtomicU64::new(2)),
    );

    assert!(matches!(landing, Landing::Dropped));
    assert_eq!(search.rebuilding(), Some(2));
}

#[test]
fn a_result_after_the_search_was_turned_off_is_dropped() {
    let rows = buffer(&["bacon"]);
    let mut search = SearchState::Off;

    let landing = search.land(
        rebuilt(2, Ok(vec![0]), Some(0)),
        2,
        &rows,
        &Arc::new(AtomicU64::new(2)),
    );

    assert!(matches!(landing, Landing::Dropped));
    assert!(matches!(search, SearchState::Off));
}

#[test]
fn rows_that_arrived_during_the_scan_are_matched_when_it_lands() {
    let mut rows = buffer(&["bacon", "eggs"]);
    rows.push(message("0", 2, "late bacon"));
    rows.push(message("0", 3, "late eggs"));
    rows.flush();
    let mut search = active(5);

    let landing = search.land(
        rebuilt(5, Ok(vec![0]), Some(1)),
        5,
        &rows,
        &Arc::new(AtomicU64::new(5)),
    );

    assert!(matches!(landing, Landing::Landed));
    assert_eq!(visible(&search), [0, 2]);
    assert_eq!(search.rebuilding(), None);
}

#[test]
fn matches_evicted_during_the_scan_are_left_out() {
    let mut rows = RowBuffer::with_caps(2, usize::MAX);
    for sequence in 0..3 {
        rows.push(message("0", sequence, "bacon"));
    }
    rows.flush();
    let mut search = active(1);

    search.land(
        rebuilt(1, Ok(vec![0, 1, 2]), Some(2)),
        1,
        &rows,
        &Arc::new(AtomicU64::new(1)),
    );

    assert_eq!(visible(&search), [1, 2]);
}

#[test]
fn a_failed_scan_turns_the_search_off_and_moves_the_generation_on() {
    let rows = buffer(&["bacon", "eggs"]);
    let mut search = active(1);
    let generation = Arc::new(AtomicU64::new(1));

    let landing = search.land(
        rebuilt(1, Err("boom".to_owned()), Some(1)),
        1,
        &rows,
        &generation,
    );

    assert!(matches!(landing, Landing::Failed(error) if error == "boom"));
    assert!(matches!(search, SearchState::Off));
    assert_eq!(generation.load(Ordering::Acquire), 2);
}

#[test]
fn too_many_late_bytes_land_the_result_and_start_a_follow_up_over_the_rest() {
    let mut rows = buffer(&["bacon"]);
    let big = "bacon".repeat(UI_MATCH_BYTES / 5 + 1);
    rows.push(message("0", 1, &big));
    rows.push(message("0", 2, "eggs"));
    rows.flush();
    let generation = Arc::new(AtomicU64::new(1));
    let mut search = active(1);

    let landing = search.land(rebuilt(1, Ok(vec![0]), Some(0)), 1, &rows, &generation);

    let Landing::FollowUp(request) = landing else {
        panic!("expected a follow-up, got {landing:?}");
    };
    assert_eq!(visible(&search), [0]);
    assert_eq!(
        request
            .rows
            .iter()
            .map(|(index, _)| *index)
            .collect::<Vec<_>>(),
        [1, 2]
    );
    assert!(request.follow_up);
    assert_eq!(request.upto, Some(2));
    assert_eq!(search.rebuilding(), Some(request.current));
    assert_eq!(generation.load(Ordering::Acquire), request.current);
}

#[test]
fn a_follow_up_result_appends_without_duplicates() {
    let rows = buffer(&["bacon", "bacon", "eggs"]);
    let mut search = SearchState::Active {
        last_good: contains("bacon"),
        visible: vec![0],
        rebuilding: Some(2),
    };
    let follow_up = Rebuilt {
        follow_up: true,
        ..rebuilt(2, Ok(vec![1]), Some(2))
    };

    search.land(follow_up, 2, &rows, &Arc::new(AtomicU64::new(2)));

    assert_eq!(visible(&search), [0, 1]);
}

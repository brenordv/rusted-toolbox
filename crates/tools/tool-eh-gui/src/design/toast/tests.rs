use super::*;
use crate::design::Ds;

#[rstest::rstest]
#[case::one_line("Exported 3 messages", 76.0)]
#[case::two_lines("a\nb", 96.0)]
#[case::three_lines("a\nb\nc", 116.0)]
#[case::four_lines("a\nb\nc\nd", 136.0)]
#[case::five_lines_stop_at_four("a\nb\nc\nd\ne", 136.0)]
fn toast_size_steps_by_a_line_and_stops_at_four(#[case] shown: &str, #[case] expected: f32) {
    let ds = Ds::new();

    let (height, _) = toast_size(ds.kit(true), shown);

    assert!((height - expected).abs() < 0.01, "{height}");
}

#[rstest::rstest]
#[case::short("Exported 3 messages", false)]
#[case::four_lines("a\nb\nc\nd", false)]
#[case::five_lines("a\nb\nc\nd\ne", true)]
#[case::wide_glyphs(&"W".repeat(140), true)]
fn toast_size_reports_a_description_past_four_lines(#[case] shown: &str, #[case] overflows: bool) {
    let ds = Ds::new();

    let (_, overflow) = toast_size(ds.kit(true), shown);

    assert_eq!(overflow, overflows);
}

#[test]
fn a_long_token_with_no_spaces_wraps_inside_the_card() {
    let ds = Ds::new();
    let token = format!("sb://{}", "a".repeat(135));

    let (height, _) = toast_size(ds.kit(true), &token);

    assert!(height > 76.0, "{height}");
}

#[rstest::rstest]
#[case::details(&details_key(7), Some(7))]
#[case::close(&close_key(7), Some(7))]
#[case::the_card(&"toast-7".to_owned(), None)]
#[case::another_control(&"pill".to_owned(), None)]
#[case::not_a_number(&"toast-x-close".to_owned(), None)]
fn a_control_key_names_its_toast(#[case] key: &str, #[case] expected: Option<u64>) {
    assert_eq!(toast_of_control(key), expected);
}

#[rstest::rstest]
#[case::none(&[], false, 0.0)]
#[case::folded(&[76.0, 136.0, 96.0], false, 76.0 + 2.0 * toast_stack::PEEK)]
#[case::fanned(&[76.0, 136.0, 96.0], true, 308.0 + 2.0 * toast_stack::GAP)]
fn the_stack_extent_covers_its_cards(
    #[case] heights: &[f32],
    #[case] expanded: bool,
    #[case] expected: f32,
) {
    assert!((stack_extent(heights, expanded) - expected).abs() < 0.01);
}

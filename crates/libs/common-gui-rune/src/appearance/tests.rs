use super::*;
use anyhow::anyhow;
use rstest::rstest;

#[rstest]
#[case::white(255, 255, 255, true)]
#[case::black(0, 0, 0, false)]
#[case::just_over(128, 128, 129, true)]
#[case::at_the_threshold(128, 128, 128, false)]
fn is_light_follows_microsofts_brightness_check(
    #[case] r: u8,
    #[case] g: u8,
    #[case] b: u8,
    #[case] light: bool,
) {
    assert_eq!(is_light(r, g, b), light);
}

#[test]
fn the_first_read_is_applied() {
    let mut watch = AppearanceWatch::default();

    assert_eq!(watch.observe(Ok(true)), Some(true));
}

#[test]
fn an_unchanged_read_applies_nothing() {
    let mut watch = AppearanceWatch::default();
    watch.observe(Ok(false));

    assert_eq!(watch.observe(Ok(false)), None);
}

#[test]
fn a_change_is_applied() {
    let mut watch = AppearanceWatch::default();
    watch.observe(Ok(true));

    assert_eq!(watch.observe(Ok(false)), Some(false));
}

#[test]
fn failed_reads_apply_nothing_and_keep_the_last_value() {
    let mut watch = AppearanceWatch::default();
    watch.observe(Ok(true));

    let first = watch.observe(Err(anyhow!("unavailable")));
    let again = watch.observe(Err(anyhow!("unavailable")));

    assert_eq!((first, again), (None, None));
    assert_eq!(watch.observe(Ok(true)), None);
}

#[test]
fn a_read_after_failures_applies_a_changed_value() {
    let mut watch = AppearanceWatch::default();
    watch.observe(Ok(true));
    watch.observe(Err(anyhow!("unavailable")));

    assert_eq!(watch.observe(Ok(false)), Some(false));
}

#[test]
fn a_failure_before_any_read_applies_nothing() {
    let mut watch = AppearanceWatch::default();

    assert_eq!(watch.observe(Err(anyhow!("unavailable"))), None);
    assert_eq!(watch.observe(Ok(false)), Some(false));
}

#[test]
fn set_appearance_reports_a_change_once_and_again_when_it_flips() {
    let mut current = None;

    let first = set_appearance(&mut current, true);
    let again = set_appearance(&mut current, true);
    let flipped = set_appearance(&mut current, false);

    assert_eq!((first, again, flipped), (true, false, true));
    assert_eq!(current, Some(false));
}

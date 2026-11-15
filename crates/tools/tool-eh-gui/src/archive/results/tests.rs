use super::*;
use crate::archive::test_support::{cursor, messages};
use crate::read::test_support::captured;

#[test]
fn append_skips_messages_already_held_without_counting_them() {
    let mut rows = ArchiveRows::with_caps(3, usize::MAX);
    rows.append(messages(2));

    let appended = rows.append(messages(3));

    assert_eq!(
        appended,
        Appended {
            added: 1,
            skipped: 2,
            full: false
        }
    );
    assert_eq!(rows.len(), 3);
}

#[test]
fn append_stops_at_the_row_cap() {
    let mut rows = ArchiveRows::with_caps(2, usize::MAX);

    let appended = rows.append(messages(3));

    assert!(appended.full);
    assert_eq!(rows.len(), 2);
}

#[test]
fn append_stops_before_a_body_that_would_pass_the_byte_cap() {
    let mut rows = ArchiveRows::with_caps(10, 8);
    let items = vec![captured("0", 0, "1234"), captured("0", 1, "123456")];

    let appended = rows.append(items);

    assert!(appended.full);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows.body_bytes(), 4);
}

#[test]
fn rows_are_keyed_and_indexed_by_position() {
    let mut rows = ArchiveRows::default();

    rows.append(messages(2));

    assert_eq!(rows.by_key("p0-s1").unwrap().index, 1);
    assert!(rows.by_key("p0-s9").is_none());
}

#[test]
fn a_full_page_is_capped_whatever_follows() {
    let mut rows = ArchiveRows::with_caps(1, usize::MAX);
    let appended = rows.append(messages(2));

    assert!(matches!(more_after(&rows, appended, None), More::Capped));
}

#[test]
fn rows_exactly_at_the_cap_with_more_to_come_are_capped() {
    let mut rows = ArchiveRows::with_caps(2, usize::MAX);
    let appended = rows.append(messages(2));

    assert!(!appended.full);
    assert!(matches!(
        more_after(&rows, appended, Some(cursor())),
        More::Capped
    ));
}

#[test]
fn rows_exactly_at_the_cap_with_nothing_more_are_the_end() {
    let mut rows = ArchiveRows::with_caps(2, usize::MAX);
    let appended = rows.append(messages(2));

    assert!(matches!(more_after(&rows, appended, None), More::End));
}

#[test]
fn rows_under_the_cap_continue_after_the_cursor() {
    let mut rows = ArchiveRows::default();
    let appended = rows.append(messages(2));

    assert!(matches!(
        more_after(&rows, appended, Some(cursor())),
        More::Ready(_)
    ));
}

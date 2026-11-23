use super::*;
use crate::read::test_support::message;

fn filled(count: usize) -> RowBuffer {
    let mut rows = RowBuffer::default();
    for sequence in 0..count {
        rows.push(message("0", sequence as i64, "body"));
    }
    rows
}

#[test]
fn pushed_rows_wait_until_a_flush_displays_them() {
    let mut rows = filled(3);

    assert!(rows.displayed().is_empty());
    assert_eq!(rows.flush(), 3);
    assert_eq!(
        rows.displayed()
            .iter()
            .map(|row| row.index)
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
}

#[test]
fn the_row_cap_evicts_the_oldest_displayed_row_and_bumps_the_epoch() {
    let mut rows = filled(ROW_CAP);
    rows.flush();
    let epoch = rows.epoch;

    let pushed = rows.push(message("0", 99_999, "new"));
    rows.flush();

    assert_eq!(pushed.evicted_displayed, 1);
    assert_eq!(pushed.trigger, Some(EvictTrigger::Rows));
    assert_eq!(rows.displayed().len(), ROW_CAP);
    assert_eq!(rows.base_index(), 1);
    assert_eq!(rows.dropped, 1);
    assert_eq!(rows.epoch, epoch + 1);
}

#[test]
fn the_cap_counts_arrived_rows_before_any_flush() {
    let rows = filled(ROW_CAP + 10);

    assert_eq!(rows.incoming_len(), ROW_CAP);
    assert_eq!(rows.dropped, 10);
    assert_eq!(rows.base_index(), 10);
}

#[test]
fn the_byte_budget_evicts_large_bodies_oldest_first() {
    let mut rows = RowBuffer::with_caps(ROW_CAP, 2_000);
    let large = "x".repeat(1_000);
    rows.push(message("0", 1, &large));
    rows.push(message("0", 2, &large));
    rows.flush();

    let pushed = rows.push(message("0", 3, "small"));
    rows.flush();

    assert_eq!(pushed.trigger, Some(EvictTrigger::Bytes));
    assert_eq!(pushed.evicted_displayed, 1);
    assert_eq!(
        rows.displayed()
            .iter()
            .map(|row| row.message.key.sequence_number)
            .collect::<Vec<_>>(),
        [2, 3]
    );
    assert_eq!(rows.body_bytes(), large.len() + "small".len());
}

#[test]
fn rows_are_found_by_global_index_and_key_after_eviction() {
    let mut rows = filled(ROW_CAP + 5);
    rows.flush();

    assert!(rows.by_index(4).is_none());
    assert_eq!(rows.by_index(5).unwrap().message.key.sequence_number, 5);
    assert_eq!(rows.by_key("p0-s7").unwrap().index, 7);
    assert_eq!(rows.last_displayed(), Some((ROW_CAP + 4) as u64));
}

#[test]
fn a_preview_is_one_safe_line_from_the_start_of_the_body() {
    let mut rows = RowBuffer::default();
    let body = format!("a\nb\u{202E}c\u{2028}d{}", "e".repeat(500));
    rows.push(message("0", 1, &body));
    rows.flush();

    let preview = &rows.displayed()[0].preview;

    assert!(preview.starts_with("a b c d"));
    assert_eq!(preview.chars().count(), PREVIEW_CHARS);
}

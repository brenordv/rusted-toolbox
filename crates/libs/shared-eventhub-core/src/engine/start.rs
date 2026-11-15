//! Where a partition receiver starts, and whether a bounded window leaves it
//! anything to read. Pure functions over the partition snapshot.

use crate::engine::Checkpoint;
use crate::filters::CaptureFilters;
use azure_messaging_eventhubs::{StartLocation, StartPosition};
use chrono::{DateTime, SubsecRound, Utc};
use std::time::{Duration, UNIX_EPOCH};

/// The source of a partition's start position.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum StartChoice {
    /// Resume after the checkpointed sequence number.
    Checkpoint(i64),
    /// Start at the filter's lower bound, floored to milliseconds.
    EnqueuedTime(DateTime<Utc>),
    Earliest,
}

/// The partition's state, read once when its task starts.
#[derive(Clone, Debug)]
pub(crate) struct PartitionSnapshot {
    pub(crate) is_empty: bool,
    pub(crate) last_sequence_number: i64,
    pub(crate) last_enqueued_time: Option<DateTime<Utc>>,
    /// Local clock when the snapshot was read.
    pub(crate) taken_at: DateTime<Utc>,
}

/// What a partition task does after its snapshot.
#[derive(Debug, PartialEq)]
pub(crate) enum StartDecision {
    /// The time window is closed and nothing the receiver could read is
    /// inside it.
    NothingInWindow,
    /// Open the receiver. With `tail` set, the window is closed and the task
    /// ends once it has processed the event at or past that sequence number.
    Consume { tail: Option<i64> },
}

/// Picks the start: a checkpoint (unless ignored) beats the filter's lower
/// bound, which beats `Earliest`. A lower bound before the Unix epoch maps to
/// `Earliest`, which covers the same events.
pub(crate) fn choose_start(
    checkpoint: Option<&Checkpoint>,
    filters: &CaptureFilters,
    ignore_checkpoint: bool,
) -> StartChoice {
    if let Some(checkpoint) = checkpoint.filter(|_| !ignore_checkpoint) {
        return StartChoice::Checkpoint(checkpoint.sequence_number);
    }

    match filters.lower_time_bound() {
        Some(bound) if bound >= DateTime::UNIX_EPOCH => {
            StartChoice::EnqueuedTime(bound.trunc_subsecs(3))
        }
        _ => StartChoice::Earliest,
    }
}

/// Converts a choice into the SDK's start position.
///
/// A checkpoint resumes exclusively (`>`), since that event is already
/// stored. An enqueued-time start is inclusive (`>=`): the server-side filter
/// may only over-fetch, because an event it skips is unrecoverable, and the
/// capture filters re-check every event exactly.
pub(crate) fn to_start_position(choice: &StartChoice) -> StartPosition {
    match choice {
        StartChoice::Checkpoint(sequence_number) => StartPosition {
            location: StartLocation::SequenceNumber(*sequence_number),
            inclusive: false,
        },
        StartChoice::EnqueuedTime(time) => {
            let system_time = u64::try_from(time.timestamp_millis())
                .ok()
                .and_then(|millis| UNIX_EPOCH.checked_add(Duration::from_millis(millis)));
            match system_time {
                Some(system_time) => StartPosition {
                    location: StartLocation::EnqueuedTime(system_time),
                    inclusive: true,
                },
                // A time before the epoch would panic inside the SDK when it
                // renders the start filter, and one past the platform's
                // `SystemTime` range cannot be expressed at all. `Earliest`
                // only over-fetches, which the capture filters correct.
                None => earliest(),
            }
        }
        StartChoice::Earliest => earliest(),
    }
}

fn earliest() -> StartPosition {
    StartPosition {
        location: StartLocation::Earliest,
        inclusive: false,
    }
}

/// Decides whether a bounded window leaves the partition anything to read.
///
/// The window counts as closed when its upper bound lies before the
/// partition's last enqueue time (broker time proves it) or is not after the
/// snapshot's local time. Only then can the task end: immediately when the
/// partition is empty or the start is already past its tail, otherwise once
/// it reaches the tail's sequence number. Sequence numbers rise within a
/// partition, so every event that existed at snapshot time is seen first.
pub(crate) fn decide(
    choice: &StartChoice,
    snapshot: &PartitionSnapshot,
    upper_bound: Option<DateTime<Utc>>,
) -> StartDecision {
    let Some(upper_bound) = upper_bound else {
        return StartDecision::Consume { tail: None };
    };
    let closed_by_broker_time = snapshot
        .last_enqueued_time
        .is_some_and(|last| upper_bound < last);
    if !closed_by_broker_time && upper_bound > snapshot.taken_at {
        return StartDecision::Consume { tail: None };
    }

    let start_past_tail = match choice {
        StartChoice::Checkpoint(sequence_number) => {
            *sequence_number >= snapshot.last_sequence_number
        }
        StartChoice::EnqueuedTime(start) => snapshot
            .last_enqueued_time
            .is_some_and(|last| *start > last),
        StartChoice::Earliest => false,
    };
    if snapshot.is_empty || start_past_tail {
        return StartDecision::NothingInWindow;
    }

    StartDecision::Consume {
        tail: Some(snapshot.last_sequence_number),
    }
}

/// The replay-guard seed: the checkpointed sequence number when the
/// checkpoint chose the start (everything up to it is already stored), and
/// nothing otherwise, so an `ignore_checkpoint` run re-reads what it asked for.
pub(crate) fn initial_high_water(choice: &StartChoice) -> Option<i64> {
    match choice {
        StartChoice::Checkpoint(sequence_number) => Some(*sequence_number),
        StartChoice::EnqueuedTime(_) | StartChoice::Earliest => None,
    }
}

/// The stored checkpoint an `ignore_checkpoint` run must not move backwards:
/// re-reading events at or below it re-stores them with the same checkpoint,
/// so the next normal run still resumes where it left off. There is no floor
/// in a normal run (it starts past the checkpoint) or when the checkpoint is
/// past the partition's last sequence number, empty partition included (a
/// recreated event hub, where moving back is the point).
pub(crate) fn checkpoint_floor(
    checkpoint: Option<&Checkpoint>,
    snapshot: &PartitionSnapshot,
    ignore_checkpoint: bool,
) -> Option<Checkpoint> {
    checkpoint
        .filter(|checkpoint| {
            ignore_checkpoint && checkpoint.sequence_number <= snapshot.last_sequence_number
        })
        .cloned()
}

/// Whether a checkpoint is beyond the partition's last sequence number: the
/// same comparison the service makes when it refuses a receiver's start ("The
/// supplied sequence number '24' is invalid. The last sequence number in the
/// system is '-1'"), whether or not the partition is empty. The usual cause is
/// an event hub deleted and recreated, where numbering starts over at -1.
pub(crate) fn checkpoint_ahead_of_partition(
    checkpoint: &Checkpoint,
    snapshot: &PartitionSnapshot,
) -> bool {
    checkpoint.sequence_number > snapshot.last_sequence_number
}

/// Refuses to resume after a checkpoint the partition never reached: the
/// service rejects a start past its last sequence number, and reading the new
/// hub from its start would collide with the old hub's events in the archive.
///
/// # Errors
/// Fails when `checkpoint` is ahead of the partition, naming both sequence
/// numbers.
pub(crate) fn ensure_resumable(
    partition_id: &str,
    checkpoint: &Checkpoint,
    snapshot: &PartitionSnapshot,
) -> anyhow::Result<()> {
    if checkpoint_ahead_of_partition(checkpoint, snapshot) {
        anyhow::bail!(
            "The stored checkpoint of partition {partition_id} (sequence number {}) is ahead of \
             the event hub (last sequence number {}). The event hub was likely deleted and \
             recreated. Use another data folder.",
            checkpoint.sequence_number,
            snapshot.last_sequence_number
        );
    }
    Ok(())
}

/// True when the whole time window ends before the checkpointed event, so a
/// resumed read cannot see any of it.
pub(crate) fn window_before_checkpoint(
    checkpoint: &Checkpoint,
    upper_bound: Option<DateTime<Utc>>,
) -> bool {
    upper_bound
        .zip(checkpoint.enqueued_time)
        .is_some_and(|(upper, checkpointed)| upper < checkpointed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filters::TimeFilter;
    use chrono::TimeZone;
    use rstest::rstest;

    fn at(hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 2, hour, minute, 0).unwrap()
    }

    fn checkpoint(sequence_number: i64, enqueued_time: Option<DateTime<Utc>>) -> Checkpoint {
        Checkpoint {
            partition_id: "0".to_owned(),
            sequence_number,
            offset: None,
            enqueued_time,
            updated_at: Utc::now(),
        }
    }

    fn filters(time: TimeFilter) -> CaptureFilters {
        CaptureFilters {
            time: Some(time),
            text: None,
        }
    }

    fn snapshot(is_empty: bool, last_sequence_number: i64) -> PartitionSnapshot {
        PartitionSnapshot {
            is_empty,
            last_sequence_number,
            last_enqueued_time: Some(at(12, 0)),
            taken_at: at(14, 0),
        }
    }

    #[test]
    fn checkpoint_beats_the_filter_lower_bound() {
        let choice = choose_start(
            Some(&checkpoint(41, None)),
            &filters(TimeFilter::NewerOrEqual(at(10, 0))),
            false,
        );

        assert_eq!(choice, StartChoice::Checkpoint(41));
    }

    #[test]
    fn ignored_checkpoint_falls_back_to_the_filter_lower_bound() {
        let choice = choose_start(
            Some(&checkpoint(41, None)),
            &filters(TimeFilter::NewerOrEqual(at(10, 0))),
            true,
        );

        assert_eq!(choice, StartChoice::EnqueuedTime(at(10, 0)));
    }

    #[test]
    fn no_checkpoint_and_no_lower_bound_starts_at_earliest() {
        let choice = choose_start(None, &filters(TimeFilter::OlderThan(at(10, 0))), false);

        assert_eq!(choice, StartChoice::Earliest);
    }

    #[rstest]
    #[case::newer_or_equal(TimeFilter::NewerOrEqual(at(10, 0)))]
    #[case::between_start(TimeFilter::Between { start: at(10, 0), end: at(11, 0) })]
    #[case::newer_than(TimeFilter::NewerThan(at(10, 0)))]
    fn lower_bounds_become_inclusive_enqueued_time_starts(#[case] time: TimeFilter) {
        let choice = choose_start(None, &filters(time), false);

        let position = to_start_position(&choice);

        assert_eq!(
            position,
            StartPosition {
                location: StartLocation::EnqueuedTime(at(10, 0).into()),
                inclusive: true,
            }
        );
    }

    #[test]
    fn enqueued_time_start_is_floored_to_milliseconds() {
        let bound = at(10, 0) + chrono::Duration::microseconds(1_999);

        let choice = choose_start(None, &filters(TimeFilter::NewerOrEqual(bound)), false);

        assert_eq!(
            choice,
            StartChoice::EnqueuedTime(at(10, 0) + chrono::Duration::milliseconds(1))
        );
    }

    #[test]
    fn checkpoint_start_is_exclusive() {
        let position = to_start_position(&StartChoice::Checkpoint(41));

        assert_eq!(
            position,
            StartPosition {
                location: StartLocation::SequenceNumber(41),
                inclusive: false,
            }
        );
    }

    #[rstest]
    #[case::before_epoch(Utc.with_ymd_and_hms(1969, 12, 31, 23, 59, 59).unwrap())]
    #[case::year_one(Utc.with_ymd_and_hms(1, 1, 1, 0, 0, 0).unwrap())]
    fn pre_epoch_lower_bound_starts_at_earliest(#[case] bound: DateTime<Utc>) {
        let choice = choose_start(None, &filters(TimeFilter::NewerOrEqual(bound)), false);

        assert_eq!(choice, StartChoice::Earliest);
        assert_eq!(to_start_position(&choice).location, StartLocation::Earliest);
    }

    #[test]
    fn a_pre_epoch_enqueued_time_choice_still_maps_to_earliest() {
        let year_one = Utc.with_ymd_and_hms(1, 1, 1, 0, 0, 0).unwrap();

        let position = to_start_position(&StartChoice::EnqueuedTime(year_one));

        assert_eq!(position.location, StartLocation::Earliest);
    }

    #[test]
    fn a_far_future_enqueued_time_start_never_panics() {
        let far_future = Utc.with_ymd_and_hms(200_000, 1, 1, 0, 0, 0).unwrap();

        let position = to_start_position(&StartChoice::EnqueuedTime(far_future));

        assert!(position.inclusive || position.location == StartLocation::Earliest);
    }

    #[rstest]
    #[case::checkpoint(StartChoice::Checkpoint(41), Some(41))]
    #[case::enqueued_time(StartChoice::EnqueuedTime(at(10, 0)), None)]
    #[case::earliest(StartChoice::Earliest, None)]
    fn the_replay_guard_is_seeded_only_by_a_checkpoint_start(
        #[case] choice: StartChoice,
        #[case] expected: Option<i64>,
    ) {
        assert_eq!(initial_high_water(&choice), expected);
    }

    #[test]
    fn an_ignored_checkpoint_seeds_nothing() {
        let choice = choose_start(
            Some(&checkpoint(41, None)),
            &CaptureFilters::default(),
            true,
        );

        assert_eq!(initial_high_water(&choice), None);
    }

    #[rstest]
    #[case::ignored_run_keeps_its_checkpoint(true, false, 40, Some(40))]
    #[case::normal_run_has_no_floor(false, false, 40, None)]
    #[case::recreated_hub_may_move_back(true, false, 60, None)]
    #[case::recreated_empty_partition_may_move_back(true, true, 60, None)]
    #[case::emptied_by_retention_keeps_its_checkpoint(true, true, 40, Some(40))]
    fn checkpoint_floor_applies_only_to_ignored_runs_on_the_same_hub(
        #[case] ignore_checkpoint: bool,
        #[case] is_empty: bool,
        #[case] checkpointed: i64,
        #[case] expected: Option<i64>,
    ) {
        let stored = checkpoint(checkpointed, None);

        let floor = checkpoint_floor(Some(&stored), &snapshot(is_empty, 50), ignore_checkpoint);

        assert_eq!(floor.map(|floor| floor.sequence_number), expected);
    }

    #[test]
    fn closed_window_on_an_empty_partition_has_nothing_to_read() {
        let decision = decide(&StartChoice::Earliest, &snapshot(true, 0), Some(at(13, 0)));

        assert_eq!(decision, StartDecision::NothingInWindow);
    }

    #[rstest]
    #[case::checkpoint_at_tail(StartChoice::Checkpoint(50))]
    #[case::checkpoint_past_tail(StartChoice::Checkpoint(51))]
    #[case::start_after_last_event(StartChoice::EnqueuedTime(at(12, 30)))]
    fn closed_window_with_a_start_past_the_tail_has_nothing_to_read(#[case] choice: StartChoice) {
        let decision = decide(&choice, &snapshot(false, 50), Some(at(13, 0)));

        assert_eq!(decision, StartDecision::NothingInWindow);
    }

    #[test]
    fn closed_window_reads_until_the_tail() {
        let decision = decide(
            &StartChoice::Checkpoint(10),
            &snapshot(false, 50),
            Some(at(13, 0)),
        );

        assert_eq!(decision, StartDecision::Consume { tail: Some(50) });
    }

    #[test]
    fn broker_time_alone_can_close_the_window() {
        let snapshot = PartitionSnapshot {
            taken_at: at(9, 0),
            ..snapshot(false, 50)
        };

        let decision = decide(&StartChoice::Earliest, &snapshot, Some(at(11, 0)));

        assert_eq!(decision, StartDecision::Consume { tail: Some(50) });
    }

    #[rstest]
    #[case::empty(true, StartChoice::Earliest)]
    #[case::checkpoint_past_tail(false, StartChoice::Checkpoint(60))]
    #[case::mid_stream(false, StartChoice::Checkpoint(10))]
    fn a_window_that_extends_into_the_future_keeps_listening(
        #[case] is_empty: bool,
        #[case] choice: StartChoice,
    ) {
        let decision = decide(&choice, &snapshot(is_empty, 50), Some(at(15, 0)));

        assert_eq!(decision, StartDecision::Consume { tail: None });
    }

    #[test]
    fn no_upper_bound_keeps_listening() {
        let decision = decide(&StartChoice::Earliest, &snapshot(true, 0), None);

        assert_eq!(decision, StartDecision::Consume { tail: None });
    }

    #[rstest]
    #[case::ahead(60, false, true)]
    #[case::at_tail(50, false, false)]
    #[case::empty_partition_at_its_last_number(50, true, false)]
    fn checkpoint_ahead_of_partition_flags_a_recreated_hub(
        #[case] sequence_number: i64,
        #[case] is_empty: bool,
        #[case] expected: bool,
    ) {
        let flagged = checkpoint_ahead_of_partition(
            &checkpoint(sequence_number, None),
            &snapshot(is_empty, 50),
        );

        assert_eq!(flagged, expected);
    }

    #[test]
    fn a_recreated_empty_hub_is_flagged() {
        let flagged = checkpoint_ahead_of_partition(&checkpoint(24, None), &snapshot(true, -1));

        assert!(flagged);
    }

    #[test]
    fn resuming_past_a_recreated_hub_fails_naming_both_sequence_numbers() {
        let error = ensure_resumable("3", &checkpoint(44, None), &snapshot(true, -1)).unwrap_err();

        let text = error.to_string();
        assert!(text.contains("partition 3 (sequence number 44)"), "{text}");
        assert!(text.contains("last sequence number -1"), "{text}");
        assert!(text.contains("Use another data folder"), "{text}");
    }

    #[test]
    fn resuming_within_the_partition_is_allowed() {
        let resumable = ensure_resumable("0", &checkpoint(50, None), &snapshot(false, 80));

        assert!(resumable.is_ok());
    }

    #[rstest]
    #[case::window_ends_before(Some(at(11, 0)), Some(at(12, 0)), true)]
    #[case::window_reaches_it(Some(at(13, 0)), Some(at(12, 0)), false)]
    #[case::no_upper_bound(None, Some(at(12, 0)), false)]
    #[case::checkpoint_without_time(Some(at(11, 0)), None, false)]
    fn window_before_checkpoint_compares_with_the_checkpointed_event(
        #[case] upper: Option<DateTime<Utc>>,
        #[case] checkpointed: Option<DateTime<Utc>>,
        #[case] expected: bool,
    ) {
        assert_eq!(
            window_before_checkpoint(&checkpoint(5, checkpointed), upper),
            expected
        );
    }
}

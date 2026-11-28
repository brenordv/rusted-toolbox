use super::*;

fn minutes(count: i64) -> TimeDelta {
    TimeDelta::minutes(count)
}

fn blocked(since: DateTime<Utc>, until: DateTime<Utc>) -> Change {
    Change::Blocked { since, until }
}

fn limits_with(source: SourceName, block: Block) -> RateLimits {
    let mut limits = RateLimits::default();
    limits.sources.insert(source, block);
    limits
}

#[test]
fn blocked_until_hides_the_source_until_the_end_and_not_after() {
    let now = Utc::now();
    let until = now + minutes(5);
    let limits = limits_with(
        SourceName::GitHub,
        Block {
            since: now,
            until,
            strikes: 1,
        },
    );

    assert_eq!(
        limits.blocked_until(SourceName::GitHub, until - TimeDelta::seconds(1)),
        Some(until)
    );
    assert_eq!(limits.blocked_until(SourceName::GitHub, until), None);
    assert_eq!(limits.blocked_until(SourceName::Osv, now), None);
}

#[test]
fn block_until_uses_retry_after_when_given() {
    let at = Utc::now();

    assert_eq!(block_until(at, Some(120)), at + TimeDelta::seconds(120));
}

#[test]
fn block_until_without_retry_after_waits_a_minute() {
    let at = Utc::now();

    assert_eq!(block_until(at, None), at + DEFAULT_BLOCK);
}

#[test]
fn block_until_never_waits_past_an_hour() {
    let at = Utc::now();

    assert_eq!(block_until(at, Some(7200)), at + MAX_BLOCK);
    assert_eq!(block_until(at, Some(u64::MAX)), at + MAX_BLOCK);
}

fn meta(retry_after: Option<u64>, remaining: Option<u64>, reset: Option<i64>) -> ResponseMeta {
    ResponseMeta {
        retry_after_seconds: retry_after,
        ratelimit_remaining: remaining,
        ratelimit_reset: reset,
        link: None,
    }
}

/// `at` whole seconds, so a reset time in epoch seconds compares equal.
fn whole_second_now() -> DateTime<Utc> {
    DateTime::from_timestamp(Utc::now().timestamp(), 0).unwrap()
}

#[test]
fn github_retry_after_wins_on_a_refusal() {
    let at = whole_second_now();

    let until = github_block_until(at, 403, &meta(Some(30), Some(0), None), 5);

    assert_eq!(until, Some(at + TimeDelta::seconds(30)));
}

#[test]
fn github_no_requests_left_blocks_until_the_reset() {
    let at = whole_second_now();
    let reset = at + minutes(10);

    let until = github_block_until(at, 429, &meta(None, Some(0), Some(reset.timestamp())), 0);

    assert_eq!(until, Some(reset));
}

#[rstest::rstest]
#[case::no_strikes(0, 1)]
#[case::one_strike(1, 2)]
#[case::three_strikes(3, 8)]
#[case::capped(40, 60)]
fn github_a_bare_refusal_waits_a_minute_doubled_per_strike(
    #[case] strikes: u32,
    #[case] wait_minutes: i64,
) {
    let at = whole_second_now();

    let until = github_block_until(at, 403, &meta(None, None, None), strikes);

    assert_eq!(until, Some(at + minutes(wait_minutes)));
}

#[rstest::rstest]
#[case::refused_by_chrono(Some(i64::MAX))]
#[case::in_the_past(Some(0))]
fn github_a_reset_that_is_not_ahead_waits_the_doubled_time(#[case] reset: Option<i64>) {
    let at = whole_second_now();

    let until = github_block_until(at, 403, &meta(None, Some(0), reset), 1);

    assert_eq!(until, Some(at + minutes(2)));
}

#[test]
fn github_a_reset_two_hours_on_is_capped_at_an_hour() {
    let at = whole_second_now();
    let reset = at + minutes(120);

    let until = github_block_until(at, 403, &meta(None, Some(0), Some(reset.timestamp())), 0);

    assert_eq!(until, Some(at + MAX_BLOCK));
}

#[test]
fn github_a_huge_retry_after_is_an_hour() {
    let at = whole_second_now();

    let until = github_block_until(at, 429, &meta(Some(u64::MAX), None, None), 0);

    assert_eq!(until, Some(at + MAX_BLOCK));
}

#[rstest::rstest]
#[case::requests_left(Some(5), None)]
#[case::no_header(None, None)]
fn github_a_success_with_requests_left_needs_no_block(
    #[case] remaining: Option<u64>,
    #[case] expected: Option<DateTime<Utc>>,
) {
    let at = whole_second_now();

    assert_eq!(
        github_block_until(at, 200, &meta(None, remaining, None), 0),
        expected
    );
}

#[test]
fn github_a_success_with_no_requests_left_blocks_until_the_reset() {
    let at = whole_second_now();
    let reset = at + minutes(15);

    let until = github_block_until(at, 200, &meta(None, Some(0), Some(reset.timestamp())), 0);

    assert_eq!(until, Some(reset));
}

#[test]
fn apply_blocked_keeps_the_later_end_and_adds_a_strike() {
    let now = Utc::now();
    let mut limits = limits_with(
        SourceName::GitHub,
        Block {
            since: now - minutes(10),
            until: now + minutes(20),
            strikes: 2,
        },
    );

    limits.apply(SourceName::GitHub, &blocked(now, now + minutes(5)));

    assert_eq!(
        limits.entry(SourceName::GitHub),
        Some(Block {
            since: now,
            until: now + minutes(20),
            strikes: 3
        })
    );
}

#[test]
fn apply_blocked_saturates_the_strikes() {
    let now = Utc::now();
    let mut limits = limits_with(
        SourceName::GitHub,
        Block {
            since: now,
            until: now + minutes(1),
            strikes: u32::MAX,
        },
    );

    limits.apply(SourceName::GitHub, &blocked(now, now + minutes(1)));

    assert_eq!(limits.strikes(SourceName::GitHub), u32::MAX);
}

#[test]
fn apply_blocked_clamps_the_end_to_an_hour_after_the_start() {
    let now = Utc::now();
    let mut limits = RateLimits::default();

    limits.apply(SourceName::Osv, &blocked(now, now + minutes(90)));

    assert_eq!(
        limits.entry(SourceName::Osv),
        Some(Block {
            since: now,
            until: now + MAX_BLOCK,
            strikes: 1
        })
    );
}

#[test]
fn apply_answered_removes_the_entry_it_loaded() {
    let now = Utc::now();
    let loaded = Block {
        since: now,
        until: now + minutes(1),
        strikes: 1,
    };
    let mut limits = limits_with(SourceName::Osv, loaded);

    limits.apply(
        SourceName::Osv,
        &Change::Answered {
            loaded: Some(loaded),
        },
    );

    assert_eq!(limits.entry(SourceName::Osv), None);
}

#[test]
fn apply_answered_keeps_a_newer_block_from_another_run() {
    let now = Utc::now();
    let newer = Block {
        since: now,
        until: now + minutes(30),
        strikes: 2,
    };
    let mut limits = limits_with(SourceName::Osv, newer);

    limits.apply(
        SourceName::Osv,
        &Change::Answered {
            loaded: Some(Block {
                since: now - minutes(5),
                until: now + minutes(1),
                strikes: 1,
            }),
        },
    );

    assert_eq!(limits.entry(SourceName::Osv), Some(newer));
}

#[test]
fn apply_answered_with_nothing_loaded_keeps_a_block_another_run_saved() {
    let now = Utc::now();
    let other = Block {
        since: now,
        until: now + minutes(1),
        strikes: 1,
    };
    let mut limits = limits_with(SourceName::Osv, other);

    limits.apply(SourceName::Osv, &Change::Answered { loaded: None });

    assert_eq!(limits.entry(SourceName::Osv), Some(other));
}

#[test]
fn two_blocks_for_one_source_merge_to_the_later_end_and_more_strikes() {
    let now = Utc::now();
    let mut limits = RateLimits::default();

    limits.apply(SourceName::DepsDev, &blocked(now, now + minutes(3)));
    limits.apply(
        SourceName::DepsDev,
        &blocked(now + minutes(1), now + minutes(2)),
    );

    assert_eq!(
        limits.entry(SourceName::DepsDev),
        Some(Block {
            since: now + minutes(1),
            until: now + minutes(3),
            strikes: 2
        })
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_save_creates_missing_folders_owner_only() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("dep-check");
    let now = Utc::now();

    save_changes(
        Some(&folder.join("rate-limits.json")),
        &[(SourceName::Osv, blocked(now, now + minutes(1)))],
        now,
    )
    .await;

    let mode = std::fs::metadata(&folder).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o700);
}

#[tokio::test]
async fn save_then_load_round_trips_with_github_written_as_github() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dep-check").join("rate-limits.json");
    let now = Utc::now();

    save_changes(
        Some(&path),
        &[(SourceName::GitHub, blocked(now, now + minutes(30)))],
        now,
    )
    .await;
    let loaded = load(Some(&path), now).await;

    assert_eq!(
        loaded.entry(SourceName::GitHub),
        Some(Block {
            since: now,
            until: now + minutes(30),
            strikes: 1
        })
    );
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("\"github\""), "{text}");
}

#[tokio::test]
async fn load_pulls_a_far_end_back_and_drops_old_entries() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rate-limits.json");
    let now = Utc::now();
    let mut saved = RateLimits::default();
    saved.sources.insert(
        SourceName::GitHub,
        Block {
            since: now,
            until: now + minutes(120),
            strikes: 1,
        },
    );
    saved.sources.insert(
        SourceName::Osv,
        Block {
            since: now - minutes(70),
            until: now - minutes(61),
            strikes: 1,
        },
    );
    saved.sources.insert(
        SourceName::DepsDev,
        Block {
            since: now - minutes(60),
            until: now - minutes(59),
            strikes: 4,
        },
    );
    save_object_to_json_file(&path, &saved).unwrap();

    let loaded = load(Some(&path), now).await;

    assert_eq!(
        loaded.entry(SourceName::GitHub).map(|block| block.until),
        Some(now + MAX_BLOCK)
    );
    assert_eq!(loaded.entry(SourceName::Osv), None);
    assert_eq!(loaded.strikes(SourceName::DepsDev), 4);
    assert_eq!(loaded.blocked_until(SourceName::DepsDev, now), None);
}

#[tokio::test]
async fn load_of_a_missing_file_or_no_path_is_empty() {
    let dir = tempfile::tempdir().unwrap();
    let now = Utc::now();

    let missing = read(&dir.path().join("absent.json"), now).await;
    let no_path = load(None, now).await;

    assert_eq!(missing.unwrap(), RateLimits::default());
    assert_eq!(no_path, RateLimits::default());
}

#[tokio::test]
async fn a_file_under_a_file_reads_as_absent() {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("dep-check");
    std::fs::write(&blocker, b"a file").unwrap();

    let limits = read(&blocker.join("rate-limits.json"), Utc::now()).await;

    assert_eq!(limits.unwrap(), RateLimits::default());
}

#[tokio::test]
async fn a_file_that_does_not_parse_is_an_error_about_its_content() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rate-limits.json");
    std::fs::write(&path, b"{ not json").unwrap();

    let error = read(&path, Utc::now()).await.unwrap_err();

    assert!(is_unparseable(&error), "{error:#}");
    assert!(!is_absent(&error), "{error:#}");
}

#[tokio::test]
async fn save_with_no_path_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let now = Utc::now();

    save_changes(
        None,
        &[(SourceName::Osv, blocked(now, now + minutes(1)))],
        now,
    )
    .await;

    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[tokio::test]
async fn an_unparseable_file_loads_empty_and_the_next_save_replaces_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rate-limits.json");
    std::fs::write(&path, b"{ not json").unwrap();
    let now = Utc::now();

    let loaded = load(Some(&path), now).await;
    save_changes(
        Some(&path),
        &[(SourceName::Osv, blocked(now, now + minutes(2)))],
        now,
    )
    .await;

    assert_eq!(loaded, RateLimits::default());
    let reloaded = load(Some(&path), now).await;
    assert_eq!(
        reloaded.blocked_until(SourceName::Osv, now),
        Some(now + minutes(2))
    );
}

#[tokio::test]
async fn two_runs_saving_blocks_for_two_sources_keep_both() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rate-limits.json");
    let now = Utc::now();

    save_changes(
        Some(&path),
        &[(SourceName::Osv, blocked(now, now + minutes(1)))],
        now,
    )
    .await;
    save_changes(
        Some(&path),
        &[(SourceName::GitHub, blocked(now, now + minutes(2)))],
        now,
    )
    .await;

    let on_disk = load(Some(&path), now).await;
    assert_eq!(
        (
            on_disk.blocked_until(SourceName::Osv, now),
            on_disk.blocked_until(SourceName::GitHub, now)
        ),
        (Some(now + minutes(1)), Some(now + minutes(2)))
    );
}

#[tokio::test]
async fn an_answer_saved_after_another_runs_new_block_keeps_that_block() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rate-limits.json");
    let now = Utc::now();
    let loaded = load(Some(&path), now).await;
    save_changes(
        Some(&path),
        &[(SourceName::Osv, blocked(now, now + minutes(4)))],
        now,
    )
    .await;

    save_changes(
        Some(&path),
        &[(
            SourceName::Osv,
            Change::Answered {
                loaded: loaded.entry(SourceName::Osv),
            },
        )],
        now,
    )
    .await;

    let on_disk = load(Some(&path), now).await;
    assert_eq!(
        on_disk.blocked_until(SourceName::Osv, now),
        Some(now + minutes(4))
    );
}

#[tokio::test]
async fn a_save_that_changes_nothing_leaves_the_file_unwritten() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rate-limits.json");
    let now = Utc::now();
    let mut saved = RateLimits::default();
    saved.sources.insert(
        SourceName::Osv,
        Block {
            since: now,
            until: now + minutes(5),
            strikes: 1,
        },
    );
    let compact = serde_json::to_string(&saved).unwrap();
    std::fs::write(&path, &compact).unwrap();

    let result = save_changes(
        Some(&path),
        &[(SourceName::Osv, Change::Answered { loaded: None })],
        now,
    )
    .await;

    assert_eq!(result, Some(saved));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), compact);
}

#[tokio::test]
async fn a_save_returns_the_blocks_it_wrote() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rate-limits.json");
    let now = Utc::now();

    let result = save_changes(
        Some(&path),
        &[(SourceName::Osv, blocked(now, now + minutes(1)))],
        now,
    )
    .await;

    assert_eq!(
        result.map(|limits| limits.strikes(SourceName::Osv)),
        Some(1)
    );
}

#[tokio::test]
async fn a_file_that_cannot_be_read_again_is_not_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(format!("{}.json", "n".repeat(300)));
    let now = Utc::now();

    let result = save_changes(
        Some(&path),
        &[(SourceName::Osv, blocked(now, now + minutes(1)))],
        now,
    )
    .await;

    assert_eq!(result, None);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[tokio::test]
async fn a_save_under_a_file_returns_and_leaves_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("dep-check");
    std::fs::write(&blocker, b"a file").unwrap();
    let now = Utc::now();

    save_changes(
        Some(&blocker.join("rate-limits.json")),
        &[(SourceName::Osv, blocked(now, now + minutes(1)))],
        now,
    )
    .await;

    assert_eq!(std::fs::read(&blocker).unwrap(), b"a file");
}

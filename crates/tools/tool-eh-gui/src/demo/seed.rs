//! Seeded Read states: a connected session with a full list, reached through real inputs so
//! the screen shows a state the app can be in.

use super::Scenario;
use super::hub::{self, PARTITIONS};
use crate::bridge::{EngineSession, UiDelivery};
use crate::read::search::Rebuilt;
use crate::read::{ReadEffect, ReadInput};
use crate::state::{AppState, Effect, Input};
use crate::tasks::StoreSlot;
use anyhow::Context;
use chrono::{TimeDelta, Utc};
use shared_eventhub_core::engine::{EngineEvent, MessageSink};
use shared_eventhub_core::stats::StatsSnapshot;
use std::collections::BTreeMap;
use std::path::Path;
use tokio::runtime::Handle;
use tokio::task::JoinSet;
use tracing::debug;

/// The rows a seeded Read state holds.
const SEEDED_ROWS: usize = 5_000;
/// The messages `seeded-export`'s archive holds: past the Export list's cap.
const SEEDED_ARCHIVE: usize = 6_000;
/// Stores in flight at a time while seeding, so the archive's writer commits them in groups.
const SEED_BATCH: usize = 256;
/// The partitions `seeded-partitions` shows.
const MANY_PARTITIONS: usize = 32;
/// Each partition's count in `seeded-partitions`: eight digits.
const PARTITION_RECEIVED: u64 = 12_345_678;
/// The partition errors `seeded-partitions` logs.
const PARTITION_ERRORS: usize = 6;
/// The quick search `seeded-search` shows.
const SEEDED_SEARCH: &str = r#""amount":4\d\d"#;

/// Connects `state` to an idle session and fills the list, selecting a JSON row; with
/// `search`, also filters the list with a regex.
pub fn read(state: &mut AppState, rt: &Handle, search: bool) {
    let partitions: Vec<String> = PARTITIONS.iter().map(|id| (*id).to_owned()).collect();
    let stats = stats(SEEDED_ROWS as u64, &partitions);
    if !connect_and_fill(state, rt, partitions, Vec::new(), stats) {
        return;
    }
    if let Some(json) = state
        .read
        .rows
        .displayed()
        .iter()
        .rev()
        .find(|row| row.message.body.starts_with('{'))
        .map(|row| row.key_str.clone())
    {
        state.apply(Input::Read(ReadInput::Select(json)));
    }
    if search {
        filter(state);
    }
}

/// Connects `state` to an idle session over [`MANY_PARTITIONS`] partitions, each with an
/// eight-digit count, and logs [`PARTITION_ERRORS`] partition errors too long to show whole.
pub fn partitions(state: &mut AppState, rt: &Handle) {
    let partitions: Vec<String> = (0..MANY_PARTITIONS).map(|id| id.to_string()).collect();
    let errors = partitions
        .iter()
        .take(PARTITION_ERRORS)
        .map(|id| EngineEvent::Error {
            partition_id: Some(id.clone()),
            error: format!(
                "The receiver for partition {id} stopped: the service closed the link \
                 (amqp:link:detach-forced) after the namespace moved the partition to another \
                 node. The session reopens it from its last checkpoint; messages sent in the \
                 meantime are read then."
            ),
            fatal: false,
        })
        .collect();
    let stats = stats(PARTITION_RECEIVED * MANY_PARTITIONS as u64, &partitions);
    connect_and_fill(state, rt, partitions, errors, stats);
}

/// Connects `state` to an idle session over `partitions` and delivers the generated rows,
/// `extra` events after them, and `stats`, then flushes the list and ticks once. False when the
/// connect did not start.
fn connect_and_fill(
    state: &mut AppState,
    rt: &Handle,
    partitions: Vec<String>,
    extra: Vec<EngineEvent>,
    stats: StatsSnapshot,
) -> bool {
    let out = state.apply(Input::ConnectPressed);
    let Some(attempt) = out.effects.iter().find_map(|effect| {
        if let Effect::PrepareSession { attempt, .. } = effect {
            Some(*attempt)
        } else {
            None
        }
    }) else {
        debug!("the seeded connect did not start; the demo shows the form instead");
        return false;
    };
    state.apply(Input::SessionReady {
        attempt,
        result: Ok(EngineSession::idle(attempt, rt)),
    });
    let messages = hub::generated(Scenario::SeededRead, SEEDED_ROWS);
    let mut events = vec![EngineEvent::Started { partitions }];
    events.extend(
        messages
            .into_iter()
            .map(|message| EngineEvent::Message(Box::new(message))),
    );
    events.extend(extra);
    state.apply(Input::Delivery(UiDelivery {
        session: attempt,
        at: Utc::now(),
        events,
        stats: Some(stats),
    }));
    state.apply(Input::Read(ReadInput::ListFlush));
    state.apply(Input::Tick(Utc::now()));
    true
}

/// Runs the seeded search's rebuild on the spot (a seeded state runs no services).
fn filter(state: &mut AppState) {
    state.read.search_edit = rune::TextEdit::new(SEEDED_SEARCH);
    state.read.search_regex = true;
    let out = state.apply(Input::Read(ReadInput::SearchEdited));
    for effect in out.effects {
        if let Effect::Read(ReadEffect::RebuildSearch(request)) = effect {
            let rebuilt = Rebuilt {
                generation: request.current,
                matcher: request.matcher.clone(),
                result: request
                    .scan()
                    .ok_or_else(|| "the search moved on".to_owned()),
                upto: request.upto,
                follow_up: request.follow_up,
                scanned: request.rows.len(),
                elapsed_ms: 0,
            };
            state.apply(Input::Read(ReadInput::SearchRebuilt(rebuilt)));
        }
    }
}

/// Stores [`SEEDED_ARCHIVE`] generated messages into the demo archive of `root`, bound to the
/// demo hub, a batch of them in flight at a time.
///
/// # Errors
/// Fails when the archive cannot be opened, bound, or written.
pub async fn seed_archive(slot: &StoreSlot, root: &Path) -> anyhow::Result<()> {
    let store = slot.acquire(&super::data_paths(root)).await?;
    store
        .bind(&hub::identity())
        .await
        .context("Failed to bind the demo archive")?;
    let mut messages = hub::generated(Scenario::SeededExport, SEEDED_ARCHIVE);
    while !messages.is_empty() {
        let mut writes = JoinSet::new();
        for message in messages.drain(..SEED_BATCH.min(messages.len())) {
            let store = store.clone();
            writes.spawn(async move {
                store
                    .store(&message, &hub::checkpoint_of(&message), true)
                    .await
            });
        }
        while let Some(written) = writes.join_next().await {
            written.context("Storing a demo message stopped unexpectedly")??;
        }
    }
    debug!(messages = SEEDED_ARCHIVE, "seeded the demo archive");
    Ok(())
}

/// Statistics for a session that received `received` messages over two minutes, spread evenly
/// over `partitions`.
fn stats(received: u64, partitions: &[String]) -> StatsSnapshot {
    let now = Utc::now();
    let per_partition = received / partitions.len().max(1) as u64;
    StatsSnapshot {
        received,
        stored: received,
        duplicated: 12,
        filtered_out: 0,
        per_partition: partitions
            .iter()
            .map(|id| {
                (
                    id.clone(),
                    shared_eventhub_core::stats::PartitionStats {
                        received: per_partition,
                        last_received_at: Some(now),
                        last_enqueued_at: Some(now),
                    },
                )
            })
            .collect::<BTreeMap<_, _>>(),
        current_rate: 212.0,
        average_rate: received as f64 / 120.0,
        peak_rate: 404.0,
        last_message_at: Some(now - TimeDelta::seconds(2)),
        started_at: now - TimeDelta::minutes(2),
        elapsed: std::time::Duration::from_secs(120),
        ..StatsSnapshot::default()
    }
}

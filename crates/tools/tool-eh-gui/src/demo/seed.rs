//! Seeded Read states: a connected session with a full list, reached through real inputs so
//! the screen shows a state the app can be in.

use super::Scenario;
use super::hub::{self, PARTITIONS};
use crate::bridge::{EngineSession, UiDelivery};
use crate::read::search::Rebuilt;
use crate::read::{ReadEffect, ReadInput};
use crate::state::{AppState, Effect, Input};
use chrono::{TimeDelta, Utc};
use shared_eventhub_core::engine::EngineEvent;
use shared_eventhub_core::stats::StatsSnapshot;
use std::collections::BTreeMap;
use tokio::runtime::Handle;
use tracing::debug;

/// The rows a seeded Read state holds.
const SEEDED_ROWS: usize = 5_000;
/// The quick search `seeded-search` shows.
const SEEDED_SEARCH: &str = r#""amount":4\d\d"#;

/// Connects `state` to an idle session and fills the list, selecting a JSON row; with
/// `search`, also filters the list with a regex.
pub fn read(state: &mut AppState, rt: &Handle, search: bool) {
    let out = state.apply(Input::ConnectPressed);
    let Some(attempt) = out.effects.iter().find_map(|effect| {
        if let Effect::PrepareSession { attempt, .. } = effect {
            Some(*attempt)
        } else {
            None
        }
    }) else {
        debug!("the seeded connect did not start; the demo shows the form instead");
        return;
    };
    state.apply(Input::SessionReady {
        attempt,
        result: Ok(EngineSession::idle(attempt, rt)),
    });
    let messages = hub::generated(Scenario::SeededRead, SEEDED_ROWS);
    let stats = stats(SEEDED_ROWS as u64);
    let mut events = vec![EngineEvent::Started {
        partitions: PARTITIONS.iter().map(|id| (*id).to_owned()).collect(),
    }];
    events.extend(
        messages
            .into_iter()
            .map(|message| EngineEvent::Message(Box::new(message))),
    );
    state.apply(Input::Delivery(UiDelivery {
        session: attempt,
        at: Utc::now(),
        events,
        stats: Some(stats),
    }));
    state.apply(Input::Read(ReadInput::ListFlush));
    state.apply(Input::Tick(Utc::now()));
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

/// Statistics for a session that received `received` messages over two minutes.
fn stats(received: u64) -> StatsSnapshot {
    let now = Utc::now();
    let per_partition = received / PARTITIONS.len() as u64;
    StatsSnapshot {
        received,
        stored: received,
        duplicated: 12,
        filtered_out: 0,
        per_partition: PARTITIONS
            .iter()
            .map(|id| {
                (
                    (*id).to_owned(),
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

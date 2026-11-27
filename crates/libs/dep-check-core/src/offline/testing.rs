//! Shared helpers for the offline tests: an updater pointed at a local server, an install in
//! one call, and a raw connection to change a store the way another program could.

use crate::ecosystem::Ecosystem;
use crate::offline::store::StoreFiles;
use crate::offline::update::{ProgressFn, Trigger, UpdateLimits, UpdateOutcome, Updater};
use crate::test_support::{MockServer, Reply, fixed_clock, osv_zip};
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;
use url::Url;

pub(crate) const USER_AGENT: &str = "dep-check-test/1.0";

/// Small bounds, so a test reaches each one with a few bytes and gives up in under a second.
pub(crate) fn test_limits() -> UpdateLimits {
    UpdateLimits {
        connect_timeout: Duration::from_secs(2),
        read_timeout: Duration::from_millis(500),
        max_archive_bytes: 1024 * 1024,
        max_entry_bytes: 16 * 1024,
        auto_deadline: Duration::from_secs(600),
    }
}

/// The path the dump of `ecosystem` is fetched from.
pub(crate) fn archive_path(ecosystem: Ecosystem) -> String {
    format!("/{}/all.zip", ecosystem.osv_name())
}

pub(crate) fn updater_at(base: Url, dir: &Path, now: DateTime<Utc>) -> Updater {
    updater_with(base, dir, now, test_limits())
}

pub(crate) fn updater_with(
    base: Url,
    dir: &Path,
    now: DateTime<Utc>,
    limits: UpdateLimits,
) -> Updater {
    Updater::with_parts(
        USER_AGENT,
        base,
        dir.to_path_buf(),
        fixed_clock(now),
        limits,
    )
    .expect("an updater")
}

pub(crate) fn quiet() -> ProgressFn {
    Arc::new(|_| {})
}

pub(crate) fn not_cancelled() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

/// A manual update of `ecosystem` with no progress and no cancel.
pub(crate) async fn update_manually(updater: &Updater, ecosystem: Ecosystem) -> UpdateOutcome {
    updater
        .update(ecosystem, Trigger::Manual, quiet(), not_cancelled())
        .await
}

/// Installs `records` as `ecosystem`'s data in `dir`, current at `now`, from a local server
/// that sends the `ETag` `"first"`.
pub(crate) async fn install(
    dir: &Path,
    ecosystem: Ecosystem,
    records: &[Value],
    now: DateTime<Utc>,
) -> UpdateOutcome {
    let server = MockServer::start();
    server.route(
        "GET",
        &archive_path(ecosystem),
        Reply::new(200, osv_zip(records)).with_header("etag", "\"first\""),
    );
    update_manually(&updater_at(server.base(), dir, now), ecosystem).await
}

/// A plain connection to `ecosystem`'s store, as another program would open it.
pub(crate) fn raw(dir: &Path, ecosystem: Ecosystem) -> Connection {
    Connection::open(StoreFiles::of(dir, ecosystem).db).expect("a raw connection")
}

/// The ids of every stored record, sorted.
pub(crate) fn stored_ids(dir: &Path, ecosystem: Ecosystem) -> Vec<String> {
    let conn = raw(dir, ecosystem);
    let mut statement = conn
        .prepare("SELECT id FROM records ORDER BY id")
        .expect("a query");
    statement
        .query_map([], |row| row.get(0))
        .expect("the ids")
        .collect::<Result<_, _>>()
        .expect("every id")
}

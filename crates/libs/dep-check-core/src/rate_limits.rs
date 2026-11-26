//! Saved rate-limit blocks: when a source may be asked again, kept between runs in one file.
//!
//! The file holds blocks and nothing else, so reading and writing it is best effort: a file
//! that cannot be read starts the run with no saved blocks, and one that cannot be written
//! leaves the run's report as it is. Both are logged at warn.

use crate::advisory::SourceName;
use anyhow::Context;
use chrono::{DateTime, TimeDelta, Utc};
use common_file_utils::permissions::create_dir_all_owner_only;
use common_serialization_utils::{load_json_file_to_object, save_object_to_json_file};
use common_utils::file_system::toolbox_home;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// The longest block the tool keeps, counted from when it was seen.
pub const MAX_BLOCK: TimeDelta = TimeDelta::seconds(3600);
/// The block for a rate-limit answer that says nothing about how long to wait.
pub const DEFAULT_BLOCK: TimeDelta = TimeDelta::seconds(60);
/// How long after a block ends its entry is kept, so its strikes count toward the next block.
const KEEP_AFTER_END: TimeDelta = TimeDelta::seconds(3600);

/// The saved blocks, one per source at most.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RateLimits {
    #[serde(default)]
    sources: BTreeMap<SourceName, Block>,
}

/// One source's block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    /// When the block was seen.
    pub since: DateTime<Utc>,
    /// When the source may be asked again.
    pub until: DateTime<Utc>,
    /// Blocks in a row with no answer between them.
    pub strikes: u32,
}

/// What one run learned about one source.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// Blocked at `since` until `until`.
    Blocked {
        since: DateTime<Utc>,
        until: DateTime<Utc>,
    },
    /// Answered with no block; `loaded` is the entry this run loaded for the source.
    Answered { loaded: Option<Block> },
}

/// When a block seen at `at` ends: `retry_after_seconds` later when the answer said, else
/// [`DEFAULT_BLOCK`] later, and never more than [`MAX_BLOCK`] after `at`. A wait too large to
/// count in seconds is the longest block.
pub fn block_until(at: DateTime<Utc>, retry_after_seconds: Option<u64>) -> DateTime<Utc> {
    let wait = match retry_after_seconds {
        None => DEFAULT_BLOCK,
        Some(seconds) => i64::try_from(seconds)
            .ok()
            .and_then(TimeDelta::try_seconds)
            .map_or(MAX_BLOCK, |wait| wait.min(MAX_BLOCK)),
    };
    add_or_latest(at, wait)
}

/// `at + wait`, or the latest time chrono can hold when that is past it.
pub(crate) fn add_or_latest(at: DateTime<Utc>, wait: TimeDelta) -> DateTime<Utc> {
    at.checked_add_signed(wait)
        .unwrap_or(DateTime::<Utc>::MAX_UTC)
}

impl RateLimits {
    /// The end of `source`'s block while `now` is before it.
    pub fn blocked_until(&self, source: SourceName, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        self.sources
            .get(&source)
            .map(|block| block.until)
            .filter(|until| now < *until)
    }

    /// `source`'s blocks in a row, 0 when it has no entry.
    pub fn strikes(&self, source: SourceName) -> u32 {
        self.sources.get(&source).map_or(0, |block| block.strikes)
    }

    /// `source`'s entry, as a run hands it back in [`Change::Answered`].
    pub fn entry(&self, source: SourceName) -> Option<Block> {
        self.sources.get(&source).copied()
    }

    /// Records `change` for `source`.
    ///
    /// A block merges into an existing entry: the later end, one more strike, the newer
    /// start, and the end never more than [`MAX_BLOCK`] after the start. An answer removes the
    /// entry only when it is still the one the run loaded, so a block another run saved in the
    /// meantime survives.
    pub fn apply(&mut self, source: SourceName, change: &Change) {
        match change {
            Change::Blocked { since, until } => {
                let block = match self.sources.get(&source) {
                    Some(existing) => Block {
                        since: existing.since.max(*since),
                        until: existing.until.max(*until),
                        strikes: existing.strikes.saturating_add(1),
                    },
                    None => Block {
                        since: *since,
                        until: *until,
                        strikes: 1,
                    },
                };
                let latest = add_or_latest(block.since, MAX_BLOCK);
                self.sources.insert(
                    source,
                    Block {
                        until: block.until.min(latest),
                        ..block
                    },
                );
            }
            Change::Answered { loaded } => {
                if self.sources.get(&source) == loaded.as_ref() {
                    self.sources.remove(&source);
                }
            }
        }
    }

    /// Pulls an end later than `now + MAX_BLOCK` (a clock set back, a skewed reset) back to it,
    /// and drops entries that ended more than an hour before `now`. Entries in between stay,
    /// so their strikes survive runs that did not ask the source.
    fn housekeep(&mut self, now: DateTime<Utc>) {
        let latest = add_or_latest(now, MAX_BLOCK);
        self.sources
            .retain(|_, block| add_or_latest(block.until, KEEP_AFTER_END) >= now);
        for block in self.sources.values_mut() {
            block.until = block.until.min(latest);
        }
    }
}

/// The rate-limit file in the toolbox home: `~/.rusted-toolbox/dep-check/rate-limits.json`;
/// `None` when there is no home folder.
pub fn rate_limits_path() -> Option<PathBuf> {
    toolbox_home().map(|home| home.join("dep-check").join("rate-limits.json"))
}

/// The saved blocks at `path`, tidied for `now`. No path or no file gives none; a file that
/// cannot be read or parsed is logged at warn and gives none.
pub async fn load(path: Option<&Path>, now: DateTime<Utc>) -> RateLimits {
    let Some(path) = path else {
        return RateLimits::default();
    };
    match read(path, now).await {
        Ok(limits) => limits,
        Err(error) => {
            // Best effort: the file holds only blocks, so the run goes on without them and the
            // next save replaces the file.
            warn!(path = %path.display(), error = %format!("{error:#}"), "the rate-limit file could not be read");
            RateLimits::default()
        }
    }
}

/// Re-reads the file at `path`, applies `changes` in order, tidies the result for `now`, and
/// writes it back atomically when that changed anything. Re-reading keeps the blocks another
/// run saved since this run loaded the file; a file whose content does not parse is replaced.
///
/// Returns the blocks the file holds after the save, also when nothing needed writing, or
/// `None` when nothing was saved: no path, a file that could not be read for another reason
/// (it would be replaced without the blocks it may hold), or a failed write. The last two are
/// logged at warn.
pub async fn save_changes(
    path: Option<&Path>,
    changes: &[(SourceName, Change)],
    now: DateTime<Utc>,
) -> Option<RateLimits> {
    let path = path?;
    let (on_disk, replace) = match read(path, now).await {
        Ok(limits) => (limits, false),
        Err(error) if is_unparseable(&error) => {
            // Content that does not parse holds no block this save could keep.
            debug!(error = %format!("{error:#}"), "the rate-limit file is replaced");
            (RateLimits::default(), true)
        }
        Err(error) => {
            // Best effort, like the write below.
            warn!(path = %path.display(), error = %format!("{error:#}"), "the rate-limit file was not saved: it could not be read again");
            return None;
        }
    };
    let mut limits = on_disk.clone();
    for (source, change) in changes {
        limits.apply(*source, change);
    }
    limits.housekeep(now);
    if limits == on_disk && !replace {
        return Some(limits);
    }

    let target = path.to_path_buf();
    let to_write = limits.clone();
    let written = tokio::task::spawn_blocking(move || write_file(&target, &to_write)).await;
    let failure = match written {
        Ok(Ok(())) => return Some(limits),
        Ok(Err(error)) => format!("{error:#}"),
        Err(join_error) => format!("the write stopped: {join_error}"),
    };
    // Best effort: losing a block costs at most one request the service answers with another
    // block.
    warn!(path = %path.display(), error = %failure, "the rate-limit file could not be written");
    None
}

/// Writes `limits` to `path` atomically, creating the missing folders owner-only (0o700 on Linux
/// and macOS), as every tool's folder under the toolbox home is.
fn write_file(path: &Path, limits: &RateLimits) -> anyhow::Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        create_dir_all_owner_only(parent)
            .with_context(|| format!("failed to create folder {}", parent.display()))?;
    }
    save_object_to_json_file(path, limits)
}

/// The blocks at `path`, tidied for `now`; an absent file gives none.
async fn read(path: &Path, now: DateTime<Utc>) -> anyhow::Result<RateLimits> {
    match load_json_file_to_object::<RateLimits>(path).await {
        Ok(mut limits) => {
            limits.housekeep(now);
            Ok(limits)
        }
        Err(error) if is_absent(&error) => Ok(RateLimits::default()),
        Err(error) => Err(error),
    }
}

/// Whether `error` says the file is not there. A path through a regular file cannot exist
/// either, and only Windows reports that as `NotFound`; Linux and macOS say `NotADirectory`.
fn is_absent(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<std::io::Error>()
        .map(std::io::Error::kind)
        .is_some_and(|kind| matches!(kind, ErrorKind::NotFound | ErrorKind::NotADirectory))
}

/// Whether `error` is about the file's content: JSON that does not parse or fit, or bytes
/// that are not UTF-8 text.
fn is_unparseable(error: &anyhow::Error) -> bool {
    error.downcast_ref::<serde_json::Error>().is_some()
        || error
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == ErrorKind::InvalidData)
}

#[cfg(test)]
mod tests;

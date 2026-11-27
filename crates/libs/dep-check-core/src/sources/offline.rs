//! The offline source: OSV.dev's downloaded data, read from the ecosystem's store. A finding
//! always counts; a no-finding answer counts only while the data is fresh.

use crate::advisory::{SourceAdvisory, SourceName};
use crate::ecosystem::Ecosystem;
use crate::engine::{PackageAnswer, PackageKey, Resolved, SourceRun};
use crate::http::SourceFailure;
use crate::offline::cleaned;
use crate::offline::matching::{Verdict, evaluate};
use crate::offline::store::{self, OpenRead};
use crate::sources::osv::{OsvAffected, OsvRecord, advisory_from_record};
use chrono::{DateTime, TimeDelta, Utc};
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;
use tokio::task::JoinError;

/// The words for data a newer dep-check saved.
const NEWER_FORMAT: &str = "made by a newer dep-check";

/// Reads the offline stores in one folder.
#[derive(Debug, Clone)]
pub struct OfflineSource {
    /// `None` when there is no home folder: no store can exist.
    dir: Option<PathBuf>,
}

/// What the offline source found.
#[derive(Debug, Clone, PartialEq)]
pub enum OfflineRead {
    /// The ecosystem has no data.
    NoData,
    /// The data was asked; every package has its answer.
    Run(SourceRun),
}

impl OfflineSource {
    /// The stores in `dir`.
    pub fn new(dir: Option<PathBuf>) -> Self {
        Self { dir }
    }

    /// Answers every package from `ecosystem`'s store, inside one read transaction under the
    /// store's shared lock, so the data cannot change between packages. A package with no
    /// finding is answered only while the data was confirmed current within `max_age` of `now`
    /// (and not after `now`); otherwise it fails as stale. Blocking: it reads the disk.
    pub fn check(
        &self,
        ecosystem: Ecosystem,
        packages: &[Resolved],
        max_age: TimeDelta,
        now: DateTime<Utc>,
    ) -> OfflineRead {
        let Some(dir) = &self.dir else {
            return OfflineRead::NoData;
        };
        let reader = match store::open_read(dir, ecosystem) {
            OpenRead::Opened(reader) => reader,
            OpenRead::NotDownloaded => return OfflineRead::NoData,
            OpenRead::Locked => return every_package_fails(packages, &SourceFailure::OfflineBusy),
            OpenRead::Unavailable(reason) | OpenRead::Unreadable(reason) => {
                return every_package_fails(packages, &SourceFailure::OfflineUnreadable { reason });
            }
            OpenRead::NewerFormat => {
                return every_package_fails(
                    packages,
                    &SourceFailure::OfflineUnreadable {
                        reason: NEWER_FORMAT.to_owned(),
                    },
                );
            }
        };
        let as_of = reader.meta().current_at;
        let fresh = as_of <= now && now - as_of <= max_age;
        let mut run = SourceRun::empty(SourceName::Offline);
        run.data_as_of = Some(as_of);
        for package in packages {
            let answer = match reader.records_for(&ecosystem.normalize_name(&package.name)) {
                Ok(records) => answer(ecosystem, package, &records, fresh, as_of, &mut run),
                Err(error) => PackageAnswer::Failed {
                    failure: SourceFailure::OfflineUnreadable {
                        reason: cleaned(&error.to_string()),
                    },
                    advisories: Vec::new(),
                },
            };
            run.answers.insert(package.key, answer);
        }
        OfflineRead::Run(run)
    }
}

/// A read in which every package fails with `failure`.
fn every_package_fails(packages: &[Resolved], failure: &SourceFailure) -> OfflineRead {
    let keys: Vec<PackageKey> = packages.iter().map(|package| package.key).collect();
    OfflineRead::Run(failed_run(&keys, failure))
}

/// An offline run in which every package in `keys` fails with `failure`.
pub(crate) fn failed_run(keys: &[PackageKey], failure: &SourceFailure) -> SourceRun {
    let mut run = SourceRun::empty(SourceName::Offline);
    for key in keys {
        run.answers.insert(
            *key,
            PackageAnswer::Failed {
                failure: failure.clone(),
                advisories: Vec::new(),
            },
        );
    }
    run
}

/// The read's result from its blocking task: a task that ended without one (it panicked) fails
/// every package in `keys` as stopped.
pub(crate) fn joined(result: Result<OfflineRead, JoinError>, keys: &[PackageKey]) -> OfflineRead {
    match result {
        Ok(read) => read,
        Err(_) => OfflineRead::Run(failed_run(keys, &SourceFailure::OfflineStopped)),
    }
}

/// One package's answer from the records the index listed for it. Any record that affects the
/// version makes it answered with those advisories; else any record that could not be decided
/// fails it; else it is answered with none while the data is fresh, and fails as stale after.
fn answer(
    ecosystem: Ecosystem,
    package: &Resolved,
    records: &[Option<String>],
    fresh: bool,
    as_of: DateTime<Utc>,
    run: &mut SourceRun,
) -> PackageAnswer {
    let mut advisories: Vec<SourceAdvisory> = Vec::new();
    let mut undecided = false;
    for record in records {
        let Some(stored) = record.as_deref().and_then(Stored::parse) else {
            undecided = true;
            continue;
        };
        match evaluate(ecosystem, &package.name, &package.version, &stored.affected) {
            Verdict::Affected => {
                let (advisory, whole) = stored.advisory(ecosystem, &package.name);
                if !whole {
                    run.details_missing = run.details_missing.saturating_add(1);
                }
                advisories.push(advisory);
            }
            Verdict::NotAffected => {}
            Verdict::Undecided => undecided = true,
        }
    }
    if !advisories.is_empty() {
        return PackageAnswer::Answered { advisories };
    }
    let failure = if undecided {
        SourceFailure::OfflineUndecided
    } else if fresh {
        return PackageAnswer::Answered {
            advisories: Vec::new(),
        };
    } else {
        SourceFailure::OfflineStale { as_of }
    };
    PackageAnswer::Failed {
        failure,
        advisories: Vec::new(),
    }
}

/// A stored record: the JSON kept for the report, and its `affected` entries.
struct Stored {
    value: Value,
    id: String,
    affected: Vec<OsvAffected>,
}

impl Stored {
    /// The record in `text`; `None` when its id or its `affected` entries do not read, which the
    /// install already checked, so the store is not what this tool wrote.
    fn parse(text: &str) -> Option<Stored> {
        let value: Value = serde_json::from_str(text).ok()?;
        let id = value.get("id")?.as_str()?.to_owned();
        let affected = match value.get("affected") {
            None | Some(Value::Null) => Vec::new(),
            Some(entries) => Vec::<OsvAffected>::deserialize(entries).ok()?,
        };
        Some(Stored {
            value,
            id,
            affected,
        })
    }

    /// The record as an advisory for `name`, and whether its details read. Only `id` and
    /// `affected` were read strictly at install; a record whose other fields do not read (a
    /// severity without a score) is read again without its severity, and then as its id alone.
    fn advisory(&self, ecosystem: Ecosystem, name: &str) -> (SourceAdvisory, bool) {
        if let Ok(record) = OsvRecord::deserialize(&self.value) {
            return (
                advisory_from_record(SourceName::Offline, &record, ecosystem, name),
                true,
            );
        }
        let mut without_severity = self.value.clone();
        if let Some(object) = without_severity.as_object_mut() {
            object.remove("severity");
        }
        match OsvRecord::deserialize(&without_severity) {
            Ok(record) => (
                advisory_from_record(SourceName::Offline, &record, ecosystem, name),
                true,
            ),
            Err(_) => (
                SourceAdvisory::id_only(SourceName::Offline, &self.id),
                false,
            ),
        }
    }
}

#[cfg(test)]
mod tests;

//! Why an update failed, with each failure's word for the log and its sentence for people.

use super::OSV_DATA_HOST;
use crate::ecosystem::Ecosystem;
use crate::http::SourceFailure;
use crate::offline::cleaned;
use common_utils::string_utils::format_bytes_to_string;

/// Why an update failed. Every text it holds is already on one line and cut.
#[derive(Debug, Clone, PartialEq)]
pub enum UpdateFailure {
    /// The request failed: no connection, a timeout, a cut connection.
    Request(SourceFailure),
    /// A status other than 200, or a 304 for a request that sent no `If-None-Match`.
    Status(u16),
    /// No `Content-Length`, so the download's size is unknown.
    NoSize,
    /// A `Content-Length` above the most the updater accepts.
    TooLarge(u64),
    /// An automatic update ran past its deadline.
    Deadline,
    /// A zip entry's data does not match its checksum or its size.
    Damaged,
    /// The download is not a zip this tool can read.
    BadArchive(String),
    /// A record whose `id` or `affected` cannot be read; the cleaned id.
    BadRecord { id: String },
    /// A record over the largest the updater accepts; the cleaned id.
    EntryTooLarge { id: String },
    /// The download held no record for the ecosystem; installing it would answer every
    /// package clean.
    NoRecords,
    /// The store was saved by a newer dep-check.
    NewerFormat,
    /// The store could not be written.
    Store(String),
    /// A file could not be written or deleted.
    Disk(String),
    /// The update lock could not be taken for a reason other than another holder.
    Lock(String),
    /// A check or a removal holds the store.
    InUse,
}

impl UpdateFailure {
    /// The failure's kind as one word, as the log lines write it.
    pub fn kind(&self) -> &'static str {
        match self {
            UpdateFailure::Request(_) => "request",
            UpdateFailure::Status(_) => "status",
            UpdateFailure::NoSize => "no_size",
            UpdateFailure::TooLarge(_) => "too_large",
            UpdateFailure::Deadline => "deadline",
            UpdateFailure::Damaged => "damaged",
            UpdateFailure::BadArchive(_) => "bad_archive",
            UpdateFailure::BadRecord { .. } => "bad_record",
            UpdateFailure::EntryTooLarge { .. } => "entry_too_large",
            UpdateFailure::NoRecords => "no_records",
            UpdateFailure::NewerFormat => "newer_format",
            UpdateFailure::Store(_) => "store",
            UpdateFailure::Disk(_) => "disk",
            UpdateFailure::Lock(_) => "lock",
            UpdateFailure::InUse => "in_use",
        }
    }

    /// The failure as a sentence about `ecosystem`'s data: "the download was damaged; try
    /// again".
    pub fn describe(&self, ecosystem: Ecosystem) -> String {
        let name = ecosystem.name();
        match self {
            UpdateFailure::Request(failure) => format!("{OSV_DATA_HOST} {}", failure.describe()),
            UpdateFailure::Status(code) => format!("{OSV_DATA_HOST} answered {code}"),
            UpdateFailure::NoSize => "the download gave no size".to_owned(),
            UpdateFailure::TooLarge(bytes) => format!(
                "the download announced {}, more than this tool accepts",
                format_bytes_to_string(*bytes)
            ),
            UpdateFailure::Deadline => "the daily update took longer than 10 minutes".to_owned(),
            UpdateFailure::Damaged => "the download was damaged; try again".to_owned(),
            UpdateFailure::BadArchive(reason) => {
                format!("the download is not a readable zip: {reason}")
            }
            UpdateFailure::BadRecord { id } => format!("the record {id} could not be read"),
            UpdateFailure::EntryTooLarge { id } => {
                format!("the record {id} is larger than 8 MiB")
            }
            UpdateFailure::NoRecords => format!("the download held no {name} records"),
            UpdateFailure::NewerFormat => {
                format!("the {name} data was saved by a newer dep-check")
            }
            UpdateFailure::Store(reason) => format!("the {name} data could not be saved: {reason}"),
            UpdateFailure::Disk(reason) => {
                format!("the {name} download could not be written: {reason}")
            }
            UpdateFailure::Lock(reason) => format!("the update lock could not be taken: {reason}"),
            UpdateFailure::InUse => {
                format!("the {name} data is in use; try again when the check or removal ends")
            }
        }
    }

    pub(super) fn disk(error: &std::io::Error) -> Self {
        UpdateFailure::Disk(cleaned(&error.to_string()))
    }

    pub(super) fn store(error: &rusqlite::Error) -> Self {
        UpdateFailure::Store(cleaned(&error.to_string()))
    }
}

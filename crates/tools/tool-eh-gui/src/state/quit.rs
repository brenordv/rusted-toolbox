//! Quitting: the jobs a closed window waits for, the reopen when they run past
//! [`STILL_FINISHING_AFTER`], and Force quit.
//!
//! | State | Fields |
//! |---|---|
//! | Open | window open, `waited` none |
//! | ClosedWaiting | window closed, `closed_at` set, `waited` none |
//! | Reopened | window open, `waited` some |
//! | Quitting | `quit_requested` (final) |
//!
//! A close with nothing running quits; with a job running it waits, and quits when the last job
//! ends. A wait past the bound reopens the window with the jobs it waited for in `waited`. From
//! there Force quit quits, the end of every waited job returns to Open, and a second close
//! waits again from zero.

use super::{AppState, Effect, Outcome, STILL_FINISHING_AFTER, Section};
use crate::archive::bulk::CancelReason;
use chrono::{DateTime, Utc};
use tracing::{debug, info, warn};

/// Work a closed window waits for before the app quits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Job {
    /// A capture session, until its at-end pass ends.
    Session,
    /// A Save config or a Save profile.
    Save,
    /// The Read inspector's export of one message.
    ReadMessageExport,
    /// The Export inspector's export of one message.
    ArchiveMessageExport,
    BulkExport,
    Send,
}

impl Job {
    /// What the job is doing, as the words after "Still ".
    fn doing(self) -> &'static str {
        match self {
            Self::Session => "writing the session's message files",
            Self::Save => "saving the config",
            Self::ReadMessageExport => "exporting a message from Read",
            Self::ArchiveMessageExport => "exporting a message from Export",
            Self::BulkExport => "writing the export",
            Self::Send => "sending",
        }
    }

    /// What ending the app costs while the job runs.
    fn at_risk(self) -> &'static str {
        match self {
            Self::Session => {
                "captured messages are safe in the archive, and eh export or the Export section \
                 can write the missing message files"
            }
            Self::Save => "the save may be incomplete",
            Self::ReadMessageExport | Self::ArchiveMessageExport | Self::BulkExport => {
                "the file may be incomplete"
            }
            Self::Send => "the last batch may or may not have been stored",
        }
    }
}

/// The still-finishing notice for `waited`: what still runs, and what Force quit does and costs.
pub fn still_finishing_text(waited: &[Job]) -> String {
    let doing: Vec<&str> = waited.iter().map(|job| job.doing()).collect();
    let mut risks: Vec<&str> = Vec::new();
    for job in waited {
        if !risks.contains(&job.at_risk()) {
            risks.push(job.at_risk());
        }
    }
    format!(
        "Still {}. Force quit ends eh-gui now and everything it is running; {}.",
        doing.join(" and "),
        risks.join("; ")
    )
}

impl AppState {
    /// The jobs running now that a closed window waits for.
    pub fn running_jobs(&self) -> Vec<Job> {
        let mut jobs = Vec::new();
        if self.connection.session().is_some() {
            jobs.push(Job::Session);
        }
        if self.save_status.is_saving() {
            jobs.push(Job::Save);
        }
        if self.read.export_running() {
            jobs.push(Job::ReadMessageExport);
        }
        if self.archive.inspector.export.is_running() {
            jobs.push(Job::ArchiveMessageExport);
        }
        if self.archive.bulk.is_running() {
            jobs.push(Job::BulkExport);
        }
        if self.send.is_running() {
            jobs.push(Job::Send);
        }
        jobs
    }

    /// The jobs the window reopened for that still run; `None` unless it reopened because a
    /// close waited past [`STILL_FINISHING_AFTER`]. While it is some, the Read section shows
    /// the still-finishing notice and Force quit is accepted.
    pub fn waited(&self) -> Option<&[Job]> {
        self.waited.as_deref()
    }

    /// Reopens a window closed for longer than [`STILL_FINISHING_AFTER`] while a job still
    /// runs, on the Read section, which shows the notice and its Force quit.
    pub(super) fn reopen_when_overdue(&mut self, now: DateTime<Utc>, out: &mut Outcome) {
        let overdue = self.window_closed
            && self
                .closed_at
                .is_some_and(|closed| now - closed > STILL_FINISHING_AFTER);
        if !overdue {
            return;
        }
        let running = self.running_jobs();
        if running.is_empty() {
            return;
        }
        info!(still_running = ?running, "still finishing; reopening the window");
        self.reopen(out);
        self.waited = Some(running);
        self.section = Section::Read;
        out.changed = true;
    }

    /// Quits without waiting for anything that runs; accepted only while the window is
    /// reopened for jobs that ran past the bound.
    pub(super) fn force_quit(&mut self, out: &mut Outcome) {
        if self.waited.is_none() {
            debug!("Force quit ignored: nothing is still finishing");
            out.changed = false;
            return;
        }
        let still_running = self.running_jobs();
        self.send.request_cancel(CancelReason::WindowClosed);
        warn!(
            still_running = ?still_running,
            "force quit: stopped waiting for running work"
        );
        self.request_quit(out);
    }

    /// Drops the waited jobs that ended; with none left, the notice and Force quit go.
    pub(super) fn settle_waited(&mut self, out: &mut Outcome) {
        let Some(mut waited) = self.waited.take() else {
            return;
        };
        let before = waited.len();
        let running = self.running_jobs();
        waited.retain(|job| running.contains(job));
        out.changed |= waited.len() != before;
        if waited.is_empty() {
            debug!("every job the window reopened for ended");
        } else {
            self.waited = Some(waited);
        }
    }

    /// Whether the Read section shows the still-finishing notice in place of its own content.
    pub(super) fn still_finishing_shows(&self) -> bool {
        let Some(waited) = &self.waited else {
            return false;
        };
        let running = self.running_jobs();
        waited.iter().any(|job| running.contains(job))
    }

    /// Quits once the window is closed and no job runs. Asks for the quit once; the quit path
    /// then runs to the end on its own.
    pub(super) fn quit_rule(&mut self, out: &mut Outcome) {
        if self.quit_requested || !self.window_closed || !self.running_jobs().is_empty() {
            return;
        }
        info!("window closed and nothing runs; quitting");
        self.request_quit(out);
    }

    fn request_quit(&mut self, out: &mut Outcome) {
        self.quit_requested = true;
        out.effects.push(Effect::Quit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_notice_for_one_job_names_it_and_what_force_quit_costs() {
        assert_eq!(
            still_finishing_text(&[Job::BulkExport]),
            "Still writing the export. Force quit ends eh-gui now and everything it is running; \
             the file may be incomplete."
        );
    }

    #[test]
    fn the_notice_for_several_jobs_names_each_job_and_each_cost_once() {
        assert_eq!(
            still_finishing_text(&[Job::ReadMessageExport, Job::BulkExport, Job::Save]),
            "Still exporting a message from Read and writing the export and saving the config. \
             Force quit ends eh-gui now and everything it is running; the file may be \
             incomplete; the save may be incomplete."
        );
    }
}

use clap::ValueEnum;
use shared_head_tail::models::{delimiter_for, CountUnit, HeaderPolicy};
use std::time::Duration;

/// How `--follow` tracks a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum FollowMode {
    /// Follow the open file descriptor, even if the file is renamed.
    Descriptor,
    /// Reopen the file by name every polling cycle, picking up rotations.
    Name,
}

/// Runtime configuration for the tail engine.
#[derive(Debug, Clone)]
pub struct TailConfig {
    pub unit: CountUnit,
    pub count: u64,
    /// `true` for `-n +NUM`/`-c +NUM`: start at item NUM from the beginning.
    pub from_start: bool,
    pub follow: Option<FollowMode>,
    pub retry: bool,
    pub sleep_interval: Duration,
    /// Accepted for GNU compatibility; this port re-opens on every cycle in
    /// name mode, so the tuning point the flag adjusts does not exist.
    pub max_unchanged_stats: Option<u64>,
    pub headers: HeaderPolicy,
    pub zero_terminated: bool,
    pub files: Vec<String>,
}

impl TailConfig {
    pub fn delimiter(&self) -> u8 {
        delimiter_for(self.zero_terminated)
    }
}

use clap::ValueEnum;
use regex::bytes::Regex;
use std::path::PathBuf;

/// How extracted values are deduplicated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum RunMode {
    /// Print every extracted occurrence.
    All,
    /// Print each distinct value once per file (first occurrence wins).
    UniquePerFile,
    /// Print each distinct value once per run (first occurrence wins).
    UniquePerRun,
}

/// How the run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    /// Every target was scanned (some may have failed individually).
    Completed,
    /// A Ctrl+C shutdown was observed mid-scan; output is partial.
    Interrupted,
}

/// Counters behind the debug-level end-of-run summary.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RunStats {
    pub targets_processed: usize,
    pub values_emitted: usize,
    pub duplicates_suppressed: usize,
}

/// What the engine hands back to `main` for exit-code mapping.
#[derive(Debug)]
pub struct EngineResult {
    pub outcome: RunOutcome,
    pub all_ok: bool,
    pub stats: RunStats,
}

/// Runtime configuration for the extraction engine. The raw pattern string is
/// not carried here: the `--app-header` block prints it from the parsed
/// arguments before this config exists.
#[derive(Debug, Clone)]
pub struct RxgetConfig {
    pub pattern: Regex,
    pub mode: RunMode,
    pub with_filename: bool,
    pub targets: Vec<PathBuf>,
}

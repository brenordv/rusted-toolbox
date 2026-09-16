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

/// One resolved input: standard input (the `-` argument) or a file path.
/// Stdin is its own variant rather than a sentinel path so it can never
/// collide with a real file named `-` (reachable as `./-`) in the dedup pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Stdin,
    File(PathBuf),
}

impl Target {
    /// The name this target renders as wherever a path would: the `-H`
    /// prefix, warnings, and error contexts. Stdin uses the fleet's
    /// "standard input" wording (the head/tail header label).
    pub fn label(&self) -> String {
        match self {
            Target::Stdin => "standard input".to_string(),
            Target::File(path) => path.display().to_string(),
        }
    }
}

/// Runtime configuration for the extraction engine. The raw pattern string is
/// not carried here: the `--app-header` block prints it from the parsed
/// arguments before this config exists.
#[derive(Debug, Clone)]
pub struct RxgetConfig {
    pub pattern: Regex,
    pub mode: RunMode,
    pub with_filename: bool,
    pub targets: Vec<Target>,
}

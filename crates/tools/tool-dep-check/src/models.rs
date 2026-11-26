use dep_check_core::ecosystem::Ecosystem;
use dep_check_core::report::Outcome;

/// How the report is written to stdout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Format {
    /// Lines for a person to read.
    Text,
    /// The report as one JSON document.
    Json,
}

/// The parsed command line without the logging flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepCheckArgs {
    pub ecosystem: Ecosystem,
    pub format: Format,
    /// The packages given as arguments; empty when they come from stdin.
    pub packages: Vec<String>,
}

/// How dep-check ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitCode {
    /// Every package was checked and nothing was found.
    Clean,
    /// At least one package has a finding.
    Vulnerable,
    /// The command line or the packages could not be read.
    Usage,
    /// Nothing was found, but some package was not found or not checked.
    Incomplete,
    /// No source answered for any package, or the run itself failed.
    Failed,
}

impl ExitCode {
    /// The process exit code.
    pub fn code(self) -> i32 {
        match self {
            ExitCode::Clean => 0,
            ExitCode::Vulnerable => 1,
            ExitCode::Usage => 2,
            ExitCode::Incomplete => 3,
            ExitCode::Failed => 4,
        }
    }

    /// The code a report's outcome ends with.
    pub fn of(outcome: Outcome) -> Self {
        match outcome {
            Outcome::Clean => ExitCode::Clean,
            Outcome::Vulnerable => ExitCode::Vulnerable,
            Outcome::Incomplete => ExitCode::Incomplete,
            Outcome::Failed => ExitCode::Failed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::clean(Outcome::Clean, 0)]
    #[case::vulnerable(Outcome::Vulnerable, 1)]
    #[case::incomplete(Outcome::Incomplete, 3)]
    #[case::failed(Outcome::Failed, 4)]
    fn each_outcome_has_its_exit_code(#[case] outcome: Outcome, #[case] code: i32) {
        assert_eq!(ExitCode::of(outcome).code(), code);
    }

    #[test]
    fn a_usage_error_exits_2() {
        assert_eq!(ExitCode::Usage.code(), 2);
    }
}

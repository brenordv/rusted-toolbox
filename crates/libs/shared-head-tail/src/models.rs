/// Unit a `-n`/`-c` count applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountUnit {
    Lines,
    Bytes,
}

/// Sign prefix parsed off a `-n`/`-c` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountPrefix {
    /// No sign on a head count: the first NUM items.
    Plain,
    /// Leading `+` on a tail count: start at item NUM from the beginning.
    FromStart,
    /// Leading `-` (head: all but the last NUM; tail: explicit from-the-end),
    /// or tail's unsigned default.
    FromEnd,
}

/// A parsed count value with its sign prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Count {
    pub value: u64,
    pub prefix: CountPrefix,
}

/// When file name headers are emitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderPolicy {
    /// Headers only when more than one input operand is given.
    Auto,
    /// `-v`/`--verbose`: headers always.
    Always,
    /// `-q`: headers never.
    Never,
}

impl HeaderPolicy {
    /// Resolves `-q` and `-v`/`--verbose` into a policy. The flags are
    /// last-wins at parse time (clap override), so at most one arrives set.
    pub fn from_flags(quiet: bool, verbose: bool) -> Self {
        if quiet {
            HeaderPolicy::Never
        } else if verbose {
            HeaderPolicy::Always
        } else {
            HeaderPolicy::Auto
        }
    }

    /// Resolves the policy against the number of input operands.
    pub fn show(self, input_count: usize) -> bool {
        match self {
            HeaderPolicy::Always => true,
            HeaderPolicy::Never => false,
            HeaderPolicy::Auto => input_count > 1,
        }
    }
}

/// How a run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    /// All inputs were processed (some may have failed individually).
    Completed,
    /// A Ctrl+C shutdown was observed; output is partial.
    Interrupted,
}

/// Outcome plus the per-input success flag, mapped to an exit code by
/// [`io_shared::exit_from_result`](crate::io_shared::exit_from_result).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunResult {
    pub outcome: RunOutcome,
    pub all_ok: bool,
}

impl RunResult {
    pub fn completed(all_ok: bool) -> Self {
        RunResult {
            outcome: RunOutcome::Completed,
            all_ok,
        }
    }

    pub fn interrupted() -> Self {
        RunResult {
            outcome: RunOutcome::Interrupted,
            all_ok: false,
        }
    }
}

/// Resolves the `-n`/`-c` pair into a unit and count, falling back to the
/// GNU default of 10 lines with the tool's default prefix. Bytes win when
/// both are set; last-wins ordering is the caller's job (head and tail get
/// it from clap's mutual `overrides_with_all`, so at most one arrives here).
pub fn resolve_count(
    lines: Option<Count>,
    bytes: Option<Count>,
    default_prefix: CountPrefix,
) -> (CountUnit, Count) {
    match (lines, bytes) {
        (_, Some(count)) => (CountUnit::Bytes, count),
        (Some(count), None) => (CountUnit::Lines, count),
        (None, None) => (
            CountUnit::Lines,
            Count {
                value: 10,
                prefix: default_prefix,
            },
        ),
    }
}

/// The item delimiter for the `-z` flag: NUL when set, newline otherwise.
pub fn delimiter_for(zero_terminated: bool) -> u8 {
    if zero_terminated { 0 } else { b'\n' }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_policy_resolves_flags_and_input_counts() {
        assert_eq!(HeaderPolicy::from_flags(true, false), HeaderPolicy::Never);
        assert_eq!(HeaderPolicy::from_flags(false, true), HeaderPolicy::Always);
        assert_eq!(HeaderPolicy::from_flags(false, false), HeaderPolicy::Auto);

        assert!(HeaderPolicy::Always.show(1));
        assert!(!HeaderPolicy::Never.show(2));
        assert!(!HeaderPolicy::Auto.show(1));
        assert!(HeaderPolicy::Auto.show(2));
    }

    #[test]
    fn delimiter_follows_the_zero_terminated_flag() {
        assert_eq!(delimiter_for(false), b'\n');
        assert_eq!(delimiter_for(true), 0);
    }
}

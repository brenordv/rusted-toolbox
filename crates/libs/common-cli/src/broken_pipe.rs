//! Broken-pipe-as-clean-exit plumbing for streaming tools.
//!
//! A tool writing to stdout dies mid-stream when its consumer closes the pipe
//! (`tool big.txt | head`). The helpers here turn that write failure into a
//! typed [`BrokenPipe`] marker inside the `anyhow` chain, so the tool's run
//! loop can end quietly with a success exit instead of reporting an error.
//! Re-raising SIGPIPE like coreutils would is not portable to Windows.

use anyhow::{Context, Result};
use std::io::{ErrorKind, Write};

/// Marks an output write that failed because the reading side of the pipe
/// closed. Run loops detect it with `error.is::<BrokenPipe>()` and treat it
/// as a normal end of consumption rather than a failure.
#[derive(Debug)]
pub struct BrokenPipe;

impl std::fmt::Display for BrokenPipe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "output pipe closed by the consumer")
    }
}

impl std::error::Error for BrokenPipe {}

/// Writes all bytes, turning a closed pipe into the [`BrokenPipe`] marker.
///
/// # Errors
/// Fails with the [`BrokenPipe`] marker when the consumer closed the pipe,
/// or with the underlying I/O error for any other write failure.
pub fn write_out<W: Write>(out: &mut W, bytes: &[u8]) -> Result<()> {
    match out.write_all(bytes) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::BrokenPipe => Err(anyhow::Error::new(BrokenPipe)),
        Err(e) => Err(e).context("failed to write output"),
    }
}

/// Flushes the writer, turning a closed pipe into the [`BrokenPipe`] marker.
///
/// # Errors
/// Fails with the [`BrokenPipe`] marker when the consumer closed the pipe,
/// or with the underlying I/O error for any other flush failure.
pub fn flush_out<W: Write>(out: &mut W) -> Result<()> {
    match out.flush() {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::BrokenPipe => Err(anyhow::Error::new(BrokenPipe)),
        Err(e) => Err(e).context("failed to flush output"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_writers::{ClosedPipe, FailingDisk};

    #[test]
    fn closed_pipe_maps_to_the_marker_on_write_and_flush() {
        let err = write_out(&mut ClosedPipe, b"x").unwrap_err();
        assert!(err.is::<BrokenPipe>());

        let err = flush_out(&mut ClosedPipe).unwrap_err();
        assert!(err.is::<BrokenPipe>());
    }

    #[test]
    fn other_write_errors_stay_ordinary_errors() {
        let err = write_out(&mut FailingDisk, b"x").unwrap_err();
        assert!(!err.is::<BrokenPipe>());

        let err = flush_out(&mut FailingDisk).unwrap_err();
        assert!(!err.is::<BrokenPipe>());
    }

    #[test]
    fn successful_writes_pass_through() {
        let mut out: Vec<u8> = Vec::new();
        write_out(&mut out, b"data").unwrap();
        flush_out(&mut out).unwrap();
        assert_eq!(out, b"data");
    }
}

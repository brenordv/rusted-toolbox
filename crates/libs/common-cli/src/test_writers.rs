//! Failing [`Write`] doubles for exercising output error paths in tests.
//!
//! These are test doubles, not production types: they exist so the tools'
//! test suites can drive their write loops into the closed-pipe, full-disk,
//! and mid-stream failure branches without touching a real pipe or disk.

use std::io::{Error, ErrorKind, Result, Write};

/// A writer whose every write and flush fails with [`ErrorKind::BrokenPipe`],
/// like stdout after the consumer closed the pipe.
pub struct ClosedPipe;

impl Write for ClosedPipe {
    fn write(&mut self, _: &[u8]) -> Result<usize> {
        Err(Error::new(ErrorKind::BrokenPipe, "closed"))
    }

    fn flush(&mut self) -> Result<()> {
        Err(Error::new(ErrorKind::BrokenPipe, "closed"))
    }
}

/// A writer whose every write and flush fails with a non-pipe error
/// ([`ErrorKind::StorageFull`]), like a destination that ran out of space.
pub struct FailingDisk;

impl Write for FailingDisk {
    fn write(&mut self, _: &[u8]) -> Result<usize> {
        Err(Error::new(ErrorKind::StorageFull, "full"))
    }

    fn flush(&mut self) -> Result<()> {
        Err(Error::new(ErrorKind::StorageFull, "full"))
    }
}

/// A writer that accepts every write but fails to flush with a non-pipe error
/// ([`ErrorKind::StorageFull`]). Exercises the paths where buffered content is
/// only lost at the flush boundary.
pub struct FailingFlush;

impl Write for FailingFlush {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<()> {
        Err(Error::new(ErrorKind::StorageFull, "full"))
    }
}

/// A writer that accepts up to `budget` bytes and then fails with the
/// configured [`ErrorKind`]. A write that crosses the budget accepts the
/// remaining budget first (a partial write), and the next write fails, so it
/// exercises partial-write handling as well as the failure itself. Flush
/// always succeeds.
pub struct FailAfter {
    budget: usize,
    kind: ErrorKind,
}

impl FailAfter {
    /// A writer that fails with `kind` once `budget` bytes were accepted.
    pub fn new(budget: usize, kind: ErrorKind) -> Self {
        Self { budget, kind }
    }

    /// The closed-pipe flavor: accepts `budget` bytes, then
    /// [`ErrorKind::BrokenPipe`].
    pub fn broken_pipe(budget: usize) -> Self {
        Self::new(budget, ErrorKind::BrokenPipe)
    }

    /// The full-disk flavor: accepts `budget` bytes, then
    /// [`ErrorKind::StorageFull`].
    pub fn storage_full(budget: usize) -> Self {
        Self::new(budget, ErrorKind::StorageFull)
    }
}

impl Write for FailAfter {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        if self.budget == 0 {
            return Err(Error::new(self.kind, "budget exhausted"));
        }
        let accepted = self.budget.min(buf.len());
        self.budget -= accepted;
        Ok(accepted)
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_pipe_fails_write_and_flush_with_broken_pipe() {
        let err = ClosedPipe.write(b"x").unwrap_err();
        assert_eq!(err.kind(), ErrorKind::BrokenPipe);
        let err = ClosedPipe.flush().unwrap_err();
        assert_eq!(err.kind(), ErrorKind::BrokenPipe);
    }

    #[test]
    fn failing_disk_fails_write_and_flush_with_storage_full() {
        let err = FailingDisk.write(b"x").unwrap_err();
        assert_eq!(err.kind(), ErrorKind::StorageFull);
        let err = FailingDisk.flush().unwrap_err();
        assert_eq!(err.kind(), ErrorKind::StorageFull);
    }

    #[test]
    fn failing_flush_accepts_writes_and_fails_only_the_flush() {
        let mut writer = FailingFlush;
        assert_eq!(writer.write(b"data").unwrap(), 4);
        assert_eq!(writer.flush().unwrap_err().kind(), ErrorKind::StorageFull);
    }

    #[test]
    fn fail_after_accepts_the_budget_partially_then_fails() {
        let mut writer = FailAfter::broken_pipe(5);
        assert_eq!(writer.write(b"abc").unwrap(), 3);
        // Crossing the budget accepts the remaining budget first.
        assert_eq!(writer.write(b"defg").unwrap(), 2);
        let err = writer.write(b"h").unwrap_err();
        assert_eq!(err.kind(), ErrorKind::BrokenPipe);
        writer.flush().unwrap();
    }

    #[test]
    fn fail_after_zero_budget_fails_immediately_with_the_configured_kind() {
        let mut writer = FailAfter::storage_full(0);
        let err = writer.write(b"x").unwrap_err();
        assert_eq!(err.kind(), ErrorKind::StorageFull);
    }
}

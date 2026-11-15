//! Waiting on a cancel flag that another thread or task sets.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// How often [`flag_set`] looks at its flag.
pub const CANCEL_POLL: Duration = Duration::from_millis(100);

/// Resolves once `flag` is set, looking at it every [`CANCEL_POLL`]. A flag
/// that is already set resolves on the first poll without waiting.
///
/// Meant as one branch of a `select!`, so whatever it races ends at most one
/// poll interval after the flag flips.
pub async fn flag_set(flag: &AtomicBool) {
    while !flag.load(Ordering::Relaxed) {
        tokio::time::sleep(CANCEL_POLL).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::time::Instant;

    #[tokio::test(start_paused = true)]
    async fn a_set_flag_resolves_without_waiting() {
        let flag = AtomicBool::new(true);
        let started = Instant::now();

        flag_set(&flag).await;

        assert_eq!(started.elapsed(), Duration::ZERO);
    }

    #[tokio::test(start_paused = true)]
    async fn a_flag_set_later_resolves_within_one_poll() {
        let flag = Arc::new(AtomicBool::new(false));
        let setter = Arc::clone(&flag);
        let started = Instant::now();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(250)).await;
            setter.store(true, Ordering::Relaxed);
        });

        flag_set(&flag).await;

        let elapsed = started.elapsed();
        assert!(elapsed >= Duration::from_millis(250), "{elapsed:?}");
        assert!(
            elapsed <= Duration::from_millis(250) + CANCEL_POLL,
            "{elapsed:?}"
        );
    }
}

//! Watching a small app's one window from outside the UI thread: when it first shows, whether
//! it could be created at all, and the system appearance.

use crate::appearance::SystemAppearance;
use rune::{AppHandle, Tx};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

/// The graphics backend Rune draws with on each platform, for the text of a window failure.
pub const GPU_BACKENDS: &str = "DirectX 12 on Windows, Vulkan on Linux, Metal on macOS";

/// What `tool` needs when its window could not be created, for the log line that reports it.
pub fn gpu_hint(tool: &str) -> String {
    format!("{tool} needs a GPU that supports {GPU_BACKENDS}")
}

/// How long a window gets to appear before an error from Rune counts as a failure to create it.
pub const WINDOW_GRACE: Duration = Duration::from_secs(2);
/// How often the watch looks at the window.
const POLL: Duration = Duration::from_millis(250);
/// The appearance is read once in this many polls: once a second.
const POLLS_PER_APPEARANCE_READ: u32 = 4;

/// Why the window counts as not created, when it does: `error` once [`WINDOW_GRACE`] has
/// passed without the window having been seen on screen. Rune keeps running windowless when
/// it cannot create the window (no GPU adapter or device), so its error alone ends nothing.
pub fn window_failure(waited: Duration, shown: bool, error: Option<String>) -> Option<String> {
    if shown || waited < WINDOW_GRACE {
        return None;
    }
    error
}

/// What a [`WindowWatch`] applies on the UI thread. Each returns whether the view changed.
pub struct WatchHooks<S> {
    /// Runs once, when the window is first seen on screen.
    pub shown: fn(&mut S, &mut Tx) -> bool,
    /// Runs with the system appearance (true: dark) on its first read and on each change.
    pub appearance: fn(&mut S, bool) -> bool,
}

/// A running window watch. Dropping it stops the watch at its next look.
pub struct WindowWatch {
    failure: Arc<Mutex<Option<String>>>,
    stop: Arc<AtomicBool>,
}

impl WindowWatch {
    /// Why the window could not be created, when the watch ended the app for that.
    pub fn failure(&self) -> Option<String> {
        self.failure
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Drop for WindowWatch {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

/// Starts one thread that looks at `handle`'s window four times a second. It applies
/// `hooks.shown` once when the window is first on screen, applies `hooks.appearance` when the
/// system appearance is first read and when it changes (read once a second), and, when
/// [`window_failure`] gives a reason, keeps it for [`WindowWatch::failure`] and quits the app.
pub fn spawn_window_watch<S: 'static>(handle: AppHandle<S>, hooks: WatchHooks<S>) -> WindowWatch {
    let failure = Arc::new(Mutex::new(None));
    let stop = Arc::new(AtomicBool::new(false));
    let watch = WindowWatch {
        failure: Arc::clone(&failure),
        stop: Arc::clone(&stop),
    };
    thread::spawn(move || {
        let started = Instant::now();
        let mut appearance = SystemAppearance::default();
        let mut shown = false;
        let mut polls: u32 = 0;
        loop {
            thread::sleep(POLL);
            if stop.load(Ordering::Acquire) {
                return;
            }
            if !shown && handle.is_visible() {
                shown = true;
                handle.update_if(hooks.shown);
            }
            let error = if shown { None } else { handle.window_error() };
            if let Some(reason) = window_failure(started.elapsed(), shown, error) {
                *failure.lock().unwrap_or_else(PoisonError::into_inner) = Some(reason);
                handle.quit();
                return;
            }
            polls = polls.wrapping_add(1);
            if polls.is_multiple_of(POLLS_PER_APPEARANCE_READ)
                && let Some(dark) = appearance.poll()
            {
                let apply = hooks.appearance;
                handle.update_if(move |state, _| apply(state, dark));
            }
        }
    });
    watch
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    const REASON: &str = "no adapter";

    #[rstest]
    #[case::before_the_grace(Duration::from_millis(1_999), false, Some(REASON), None)]
    #[case::at_the_grace(WINDOW_GRACE, false, Some(REASON), Some(REASON))]
    #[case::long_after(Duration::from_secs(60), false, Some(REASON), Some(REASON))]
    #[case::no_error(Duration::from_secs(60), false, None, None)]
    #[case::seen_on_screen(Duration::from_secs(60), true, Some(REASON), None)]
    #[case::seen_early(Duration::ZERO, true, Some(REASON), None)]
    fn window_failure_needs_the_grace_an_error_and_no_window(
        #[case] waited: Duration,
        #[case] shown: bool,
        #[case] error: Option<&str>,
        #[case] expected: Option<&str>,
    ) {
        let failure = window_failure(waited, shown, error.map(str::to_owned));

        assert_eq!(failure.as_deref(), expected);
    }

    #[test]
    fn the_gpu_hint_names_the_tool_and_the_backends() {
        let hint = gpu_hint("alert");

        assert_eq!(
            hint,
            format!("alert needs a GPU that supports {GPU_BACKENDS}")
        );
    }
}

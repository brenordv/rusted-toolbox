use crossterm::cursor::MoveToColumn;
use crossterm::style::{Color, Print, ResetColor, SetForegroundColor};
use crossterm::terminal::{Clear, ClearType};
use crossterm::ExecutableCommand;
use std::io::{stdout, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// Tracks export progress with counters for exported, skipped, and duplicated messages.
///
/// # Fields
/// - `messages_exported`: Successfully exported message count
/// - `messages_skipped`: Skipped message count (filtered out)
/// - `messages_duplicated`: Duplicate message count (already exported)
/// - `start_time`: Export start timestamp for rate calculation
/// - `feedback_interval_secs`: Progress feedback interval in seconds
/// - `last_progress_time`: Thread-safe tracking of last progress display
pub struct ExportProgressTracker {
    pub messages_exported: AtomicU64,
    pub messages_skipped: AtomicU64,
    pub messages_duplicated: AtomicU64,
    pub start_time: Instant,
    pub feedback_interval_secs: f64,
    pub last_progress_time: std::sync::Mutex<Instant>,
}

impl ExportProgressTracker {
    /// Creates a new progress tracker with the specified feedback interval.
    ///
    /// # Arguments
    /// - `feedback_interval_secs`: Progress feedback interval in seconds
    ///
    /// # Returns
    /// - `Self`: Initialized progress tracker with zero counters
    pub fn new(feedback_interval_secs: f64) -> Self {
        Self {
            messages_exported: AtomicU64::new(0),
            messages_skipped: AtomicU64::new(0),
            messages_duplicated: AtomicU64::new(0),
            start_time: Instant::now(),
            feedback_interval_secs,
            last_progress_time: std::sync::Mutex::new(Instant::now()),
        }
    }

    /// Increments the exported message counter by 1.
    pub fn increment_exported(&self) {
        self.messages_exported.fetch_add(1, Ordering::Relaxed);
    }

    /// Increments the skipped message counter by 1.
    pub fn increment_skipped(&self) {
        self.messages_skipped.fetch_add(1, Ordering::Relaxed);
    }

    /// Increments the duplicated message counter by 1.
    pub fn increment_duplicated(&self) {
        self.messages_duplicated.fetch_add(1, Ordering::Relaxed);
    }

    /// Determines if progress should be displayed based on elapsed time since last display.
    ///
    /// # Returns
    /// - `true`: Feedback interval has elapsed, progress should be shown
    /// - `false`: Feedback interval not yet reached
    pub fn should_show_progress(&self) -> bool {
        let mut last_time = self.last_progress_time.lock().unwrap();
        let now = Instant::now();
        if now.duration_since(*last_time).as_secs_f64() >= self.feedback_interval_secs {
            *last_time = now;
            true
        } else {
            false
        }
    }

    /// Formats a progress line with export statistics and runtime information.
    ///
    /// # Returns
    /// - `String`: Formatted progress line with counts, rate, and runtime
    ///
    /// # Format
    /// - Exported/Skipped/Duplicated counts
    /// - Messages per second rate
    /// - Runtime in HH:MM:SS.mmmm format
    pub fn format_progress_line(&self) -> String {
        let exported = self.messages_exported.load(Ordering::Relaxed);
        let skipped = self.messages_skipped.load(Ordering::Relaxed);
        let duplicated = self.messages_duplicated.load(Ordering::Relaxed);
        let runtime = self.start_time.elapsed();

        let rate = if runtime.as_secs_f64() > 0.0 {
            exported as f64 / runtime.as_secs_f64()
        } else {
            0.0
        };

        let hours = runtime.as_secs() / 3600;
        let minutes = (runtime.as_secs() % 3600) / 60;
        let seconds = runtime.as_secs() % 60;
        let millis = runtime.subsec_millis();

        format!(
            "Exported: {} | Skipped: {} | Duplicated: {} | Rate: {:.2} msg/s | Runtime: {:02}:{:02}:{:02}.{:04}",
            exported, skipped, duplicated, rate, hours, minutes, seconds, millis
        )
    }

    /// Prints formatted progress to stdout with cyan color and line clearing.
    pub fn print_progress(&self) {
        let progress_line = self.format_progress_line();
        let mut stdout = stdout();
        let _ = stdout.execute(MoveToColumn(0));
        let _ = stdout.execute(Clear(ClearType::CurrentLine));
        let _ = stdout.execute(SetForegroundColor(Color::Cyan));
        let _ = stdout.execute(Print(&progress_line));
        let _ = stdout.execute(ResetColor);
        let _ = stdout.flush();
    }
}

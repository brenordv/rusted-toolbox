use crate::shared::eventhub::eventhub_models::InboundMessage;
use chrono::{DateTime, Local, Utc};
use crossterm::cursor::MoveToColumn;
use crossterm::style::{Color, Print, ResetColor, SetForegroundColor};
use crossterm::terminal::{Clear, ClearType};
use crossterm::ExecutableCommand;
use std::io::{stdout, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// Tracks EventHub message processing statistics and progress display.
///
/// Thread-safe counters for messages read, skipped, duplicated, and active operations.
/// Manages progress reporting intervals and maximum processing rates.
///
/// # Fields
///
/// * `messages_read` - A thread-safe counter (`AtomicU64`) representing the total
///   number of messages that have been successfully read and processed.
///
/// * `messages_skipped` - A thread-safe counter (`AtomicU64`) representing the total
///   number of messages that were skipped due to processing rules, errors, or other reasons.
///
/// * `messages_duplicated` - A thread-safe counter (`AtomicU64`) representing the
///   total number of duplicate messages encountered during processing.
///
/// * `active_operations` - A thread-safe counter (`AtomicU64`) tracking the number
///   of ongoing active operations in the system.
///
/// * `start_time` - An `Instant` value that captures the starting time when the
///   tracker was initialized. This is used to calculate elapsed time for the activity.
///
/// * `last_message_time` - A `Mutex`-wrapped `Option<DateTime<Utc>>` that stores
///   the timestamp of the most recent message processed. This is used to track
///   message processing frequency and determine periods of inactivity if applicable.
///
/// * `feedback_interval_secs` - A `u64` representing the interval (in seconds) at which
///   feedback or progress reporting occurs. This can be used to define a regular cadence
///   for any reporting mechanism.
///
/// * `last_progress_time` - A `Mutex`-wrapped `Instant` value representing the last
///   recorded time when progress was reported or significant activity was logged. This
///   helps in determining when to send updates or trigger events.
///
/// * `max_rate` - A `Mutex`-wrapped `f64` used to store the maximum processing rate
///   achieved by the system, expressed in some unit of operation (e.g., messages/sec).
///
/// # Thread Safety
///
/// The atomic and mutex-wrapped fields in this struct allow it to be shared safely
/// among multiple threads. However, care should be taken to avoid deadlocks when
/// acquiring multiple `Mutex` locks simultaneously.
pub struct ProgressTracker {
    pub messages_read: AtomicU64,
    pub messages_skipped: AtomicU64,
    pub messages_duplicated: AtomicU64,
    pub active_operations: AtomicU64,
    pub start_time: Instant,
    pub last_message_time: std::sync::Mutex<Option<DateTime<Utc>>>,
    pub feedback_interval_secs: u64,
    pub last_progress_time: std::sync::Mutex<Instant>,
    pub max_rate: std::sync::Mutex<f64>,
}

impl ProgressTracker {
    /// Creates new progress tracker with feedback interval.
    ///
    /// Initializes all counters to zero and sets current time as start time.
    pub fn new(feedback_interval_secs: u64) -> Self {
        let now = Instant::now();
        Self {
            messages_read: AtomicU64::new(0),
            messages_skipped: AtomicU64::new(0),
            messages_duplicated: AtomicU64::new(0),
            active_operations: AtomicU64::new(0),
            start_time: now,
            last_message_time: std::sync::Mutex::new(None),
            feedback_interval_secs,
            last_progress_time: std::sync::Mutex::new(now),
            max_rate: std::sync::Mutex::new(0.0),
        }
    }

    /// Increments read counter and updates last message timestamp.
    ///
    /// Thread-safe atomic increment with current UTC time recording.
    pub fn increment_read(&self) {
        self.messages_read.fetch_add(1, Ordering::Relaxed);
        *self.last_message_time.lock().unwrap() = Some(Utc::now());
    }

    /// Increments skipped message counter.
    ///
    /// Atomic increment for messages skipped due to filtering or processing rules.
    pub fn increment_skipped(&self) {
        self.messages_skipped.fetch_add(1, Ordering::Relaxed);
    }

    /// Increments duplicate message counter.
    ///
    /// Atomic increment for duplicate messages encountered during processing.
    pub fn increment_duplicated(&self) {
        self.messages_duplicated.fetch_add(1, Ordering::Relaxed);
    }

    /// Increments active operations counter.
    ///
    /// Tracks number of currently active processing operations.
    pub fn increment_active_operations(&self) {
        self.active_operations.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrements active operations counter.
    ///
    /// Reduces count when operations complete or are cancelled.
    pub fn decrement_active_operations(&self) {
        self.active_operations.fetch_sub(1, Ordering::Relaxed);
    }

    /// Returns current count of active operations.
    ///
    /// Thread-safe atomic read of active operations counter.
    pub fn get_active_operations(&self) -> u64 {
        self.active_operations.load(Ordering::Relaxed)
    }

    /// Determines if progress should be displayed based on feedback interval.
    ///
    /// Returns true if enough time elapsed since last update or first message processed.
    /// Updates last progress time when returning true.
    pub fn should_show_progress(&self) -> bool {
        let now = Instant::now();
        let mut last_progress = self.last_progress_time.lock().unwrap();

        // Show progress if enough time has passed OR if this is the first message
        let total_messages = self.messages_read.load(Ordering::Relaxed);
        if now.duration_since(*last_progress).as_secs() >= self.feedback_interval_secs
            || total_messages == 1
        {
            *last_progress = now;
            true
        } else {
            false
        }
    }

    /// Forces progress timestamp update to current time.
    ///
    /// Thread-safe update of last progress time to current instant.
    pub fn force_update_progress_control(&self) {
        // Force update the last progress time and return true
        let now = Instant::now();
        let mut last_progress = self.last_progress_time.lock().unwrap();
        *last_progress = now;
    }

    /// Forces progress display with timestamp update.
    ///
    /// Updates progress control then prints current progress status.
    pub fn print_progress_forced(&self) {
        self.force_update_progress_control();
        self.print_progress();
    }

    /// Updates maximum observed processing rate.
    ///
    /// Thread-safe update if current rate exceeds stored maximum.
    pub fn update_max_rate(&self, current_rate: f64) {
        let mut max_rate = self.max_rate.lock().unwrap();
        if current_rate > *max_rate {
            *max_rate = current_rate;
        }
    }

    /// Returns maximum processing rate achieved.
    ///
    /// Thread-safe read of stored maximum rate value.
    pub fn get_max_rate(&self) -> f64 {
        *self.max_rate.lock().unwrap()
    }

    /// Formats current progress statistics as display string.
    ///
    /// Returns formatted string with counters, processing rate, runtime duration,
    /// and last message timestamp for monitoring display.
    pub fn format_progress_line(&self) -> String {
        let messages_read = self.messages_read.load(Ordering::Relaxed);
        let messages_skipped = self.messages_skipped.load(Ordering::Relaxed);
        let messages_duplicated = self.messages_duplicated.load(Ordering::Relaxed);
        let elapsed = self.start_time.elapsed();
        let messages_per_second = if elapsed.as_secs_f64() > 0.0 {
            messages_read as f64 / elapsed.as_secs_f64()
        } else {
            0.0
        };

        // Update max rate
        self.update_max_rate(messages_per_second);

        let hours = elapsed.as_secs() / 3600;
        let minutes = (elapsed.as_secs() % 3600) / 60;
        let seconds = elapsed.as_secs() % 60;
        let millis = elapsed.subsec_millis();

        let last_msg_time = self
            .last_message_time
            .lock()
            .unwrap()
            .map(|t| t.with_timezone(&Local).format("%H:%M:%S%.3f").to_string())
            .unwrap_or_else(|| "Never".to_string());

        format!(
            "Read: {} | Skipped: {} | Duplicated: {} | Rate: {:.2} msg/s | Runtime: {:02}:{:02}:{:02}.{:04} | Last: {}",
            messages_read, messages_skipped, messages_duplicated, messages_per_second, hours, minutes, seconds, millis, last_msg_time
        )
    }

    /// Prints the progress line to the terminal, ensuring it fits within the terminal's width
    /// and is truncated if necessary.
    ///
    /// This method retrieves the formatted progress line by calling `format_progress_line`
    /// and determines the current terminal width using `crossterm::terminal::size()`. If the
    /// progress line exceeds the terminal width, it truncates the line and appends "..." to
    /// the end. The truncated or full line is then output to the terminal with a green
    /// foreground color.
    ///
    /// Steps performed in this method:
    /// 1. Calls `self.format_progress_line` to generate the progress content.
    /// 2. Fetches the current terminal width, defaulting to 80 columns if the size cannot be retrieved.
    /// 3. Truncates the progress line if it exceeds the terminal width, reserving space for an ellipsis.
    /// 4. Clears the current line on the terminal, moves the cursor to the beginning of the line,
    ///    sets the text color to green, prints the progress line, and resets the text color to default.
    /// 5. Flushes the output buffer to ensure an immediate display.
    ///
    /// # Errors
    /// The method performs various terminal operations via the `crossterm` library. If these operations
    /// fail, they are ignored, and the program continues execution.
    ///
    /// # Dependencies
    /// - This method requires the `crossterm` crate for terminal size retrieval and control of
    ///   terminal output, including clearing lines, moving the cursor, and changing text appearance.
    ///
    /// # Note
    /// This function uses `stdout` for terminal output. If called in rapid succession or
    /// with competing output to `stdout`, the behavior may result in an inconsistent display.
    pub fn print_progress(&self) {
        let progress_line = self.format_progress_line();
        let terminal_width = crossterm::terminal::size().unwrap_or((80, 24)).0 as usize;
        let truncated_line = if progress_line.len() > terminal_width {
            format!("{}...", &progress_line[..terminal_width.saturating_sub(3)])
        } else {
            progress_line
        };

        let mut stdout = stdout();
        let _ = stdout.execute(Clear(ClearType::CurrentLine));
        let _ = stdout.execute(MoveToColumn(0));
        let _ = stdout.execute(SetForegroundColor(Color::Green));
        let _ = stdout.execute(Print(&truncated_line));
        let _ = stdout.execute(ResetColor);
        let _ = stdout.flush();
    }

    /// Prints formatted information about an inbound message to the terminal.
    ///
    /// This function generates a concise summary of the input message, ensuring
    /// the output is human-readable and aligned within the terminal dimensions.
    ///
    /// # Parameters
    /// - `&self`: The reference to the struct instance.
    /// - `message: &InboundMessage`: A reference to an `InboundMessage` object
    ///   containing the message data to be processed.
    ///
    /// The printed information includes:
    /// 1. **Partition and Offset**: Displays the partition ID and event offset
    ///    (or "N/A" if undefined).
    /// 2. **Byte Count**: Specifies the size of the message in bytes.
    /// 3. **Message Preview**: Provides a sanitized, single-line preview of the
    ///    message content, excluding newlines, tabs, carriage returns, and other
    ///    control characters.
    ///
    /// The message preview is truncated if its length exceeds the terminal width,
    /// appending "..." to indicate truncation. Additionally, the output is embedded
    /// with ANSI color codes (blue text) to differentiate it in the terminal.
    ///
    /// # Behavior
    /// - Retrieves the terminal width to adjust the output accordingly.
    /// - Ensures non-printable control characters are removed from the message
    ///   preview.
    /// - Handles terminal-specific edge cases, such as avoiding output interleaving
    ///   through flushing of the stdout buffer.
    ///
    /// The output might look like:
    /// ```plaintext
    /// Partition/Offset: 1/45 | Bytes: 43 | Preview: Hello, World! This is a test message.
    /// ```
    ///
    /// Note: The message preview will be colored blue in a supported terminal.
    ///
    /// # Errors
    /// - If the terminal dimensions cannot be determined, a fallback width of `80` is used.
    /// - Any errors occurring during terminal writing or flushing (e.g., `stdout.execute`
    ///   or `stdout.flush`) are ignored.
    pub fn print_message_info(&self, message: &InboundMessage) {
        // Create a single-line preview of the message content
        let preview = message
            .msg_data
            .replace('\n', " ") // Replace newlines with spaces
            .replace('\r', "") // Remove carriage returns
            .replace('\t', " ") // Replace tabs with spaces
            .chars()
            .filter(|c| !c.is_control()) // Remove other control characters
            .collect::<String>();

        let info = format!(
            "Partition/Offset: {}/{} | Bytes: {} | Preview: {}",
            message.partition_id,
            message.event_offset.as_deref().unwrap_or("N/A"),
            message.msg_data.len(),
            preview
        );

        let terminal_width = crossterm::terminal::size().unwrap_or((80, 24)).0 as usize;
        let max_width = terminal_width.saturating_sub(1); // Leave room for the cursor
        let truncated_info = if info.len() > max_width {
            if max_width > 3 {
                format!("{}...", &info[..max_width.saturating_sub(3)])
            } else {
                "...".to_string()
            }
        } else {
            info
        };

        // Build the entire output as a single string with embedded ANSI codes
        // to prevent interleaving of output from multiple tasks
        let output = format!("\n\x1b[34m{}\x1b[0m", truncated_info);

        let mut stdout = stdout();
        let _ = stdout.execute(Print(&output));
        let _ = stdout.flush();
    }
}

// RAII guard to automatically decrement active operations counter
pub struct OperationGuard<'a> {
    progress: &'a ProgressTracker,
}

impl<'a> OperationGuard<'a> {
    pub(crate) fn new(progress: &'a ProgressTracker) -> Self {
        progress.increment_active_operations();
        Self { progress }
    }
}

impl<'a> Drop for OperationGuard<'a> {
    fn drop(&mut self) {
        self.progress.decrement_active_operations();
    }
}

use crate::progress_tracker::{OperationGuard, ProgressTracker};
use anyhow::{anyhow, Context, Result};
use azeventhubs::consumer::{EventPosition, ReadEventOptions};
use azeventhubs::ReceivedEventData;
use chrono::{DateTime, Local, Utc};
use futures_util::StreamExt;
use shared_eventhub::eventhub_models::{
    EventHubCheckpoint, EventHubConfig, InboundMessage, MessageStatus,
};
use shared_eventhub::utils::get_eventhub_database_path::get_eventhub_database_path;
use shared::system::resolve_path_with_base::resolve_path_with_base;
use shared::utils::message_matches_filter::message_matches_filter;
use sled::Db;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info, warn};

/// EventHub message consumer with progress tracking and graceful shutdown.
///
/// Manages EventHub connections, message processing, checkpoint persistence,
/// and coordinated shutdown across multiple partitions.
pub struct EventHubReader {
    config: EventHubConfig,
    db: Db,
    progress: Arc<ProgressTracker>,
    shutdown: Arc<AtomicBool>,
}

impl EventHubReader {
    /// Creates new EventHub reader instance.
    ///
    /// Initializes database connection, progress tracker, and shutdown signal
    /// based on provided configuration.
    ///
    /// # Errors
    /// Returns error if database path resolution or opening fails.
    pub async fn new(config: EventHubConfig) -> Result<Self> {
        let db_path = get_eventhub_database_path(
            &config.connection_string,
            &config.inbound_config.base_data_folder,
            &config.inbound_config.database_path,
        )
        .context("Failed to resolve database path")?;

        let db = sled::open(&db_path).context("Failed to open database")?;

        let progress = Arc::new(ProgressTracker::new(
            config.inbound_config.feedback_interval,
        ));

        let shutdown = Arc::new(AtomicBool::new(false));

        Ok(Self {
            config,
            db,
            progress,
            shutdown,
        })
    }

    /// Links external shutdown signal to internal shutdown handler.
    ///
    /// Spawns monitoring task that polls external signal every 100ms
    /// and propagates shutdown to internal components.
    pub fn set_shutdown_signal(&self, external_shutdown: Arc<AtomicBool>) {
        // Create a monitoring task that will set our internal shutdown flag
        // when the external shutdown signal is triggered
        let internal_shutdown = Arc::clone(&self.shutdown);

        tokio::spawn({
            let external_shutdown = Arc::clone(&external_shutdown);
            async move {
                // Poll the external shutdown signal every 100ms - Originally 50 ms.
                while !external_shutdown.load(Ordering::Relaxed) {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }

                // When external shutdown is triggered, set internal shutdown
                internal_shutdown.store(true, Ordering::Relaxed);

                // Log the shutdown propagation for debugging
                info!("External shutdown signal received and propagated to EventHubReader");
            }
        });
    }

    /// Generates checkpoint key for partition.
    ///
    /// Format: `checkpoint:<entity_path>:<partition_id>`
    fn get_checkpoint_key(&self, partition_id: &str) -> String {
        format!("checkpoint:{}:{}", self.config.entity_path, partition_id)
    }

    /// Saves message checkpoint to database.
    ///
    /// Creates checkpoint with sequence number, offset, partition ID,
    /// and current timestamp, then stores as JSON.
    ///
    /// # Errors
    /// Returns error if checkpoint serialization or database insert fails.
    fn save_checkpoint(
        &self,
        partition_id: &str,
        sequence_number: i64,
        offset: &str,
    ) -> Result<()> {
        let checkpoint = EventHubCheckpoint {
            sequence_number,
            offset: offset.to_string(),
            partition_id: partition_id.to_string(),
            updated_at: Utc::now(),
        };

        let checkpoint_key = self.get_checkpoint_key(partition_id);

        self.db
            .insert(
                &checkpoint_key,
                serde_json::to_vec(&checkpoint).context("Failed to serialize checkpoint")?,
            )
            .context("Failed to save checkpoint")?;

        Ok(())
    }

    /// Loads checkpoint from database for partition.
    ///
    /// # Returns
    /// - `Some(checkpoint)` if found and deserialized successfully
    /// - `None` if no checkpoint exists for partition
    ///
    /// # Errors
    /// Returns error if database access or deserialization fails.
    fn load_checkpoint(&self, partition_id: &str) -> Result<Option<EventHubCheckpoint>> {
        let checkpoint_key = self.get_checkpoint_key(partition_id);

        if let Some(checkpoint_data) = self
            .db
            .get(&checkpoint_key)
            .context("Failed to load checkpoint")?
        {
            let checkpoint: EventHubCheckpoint = serde_json::from_slice(&checkpoint_data)
                .context("Failed to deserialize checkpoint")?;

            return Ok(Some(checkpoint));
        }

        Ok(None)
    }

    /// Starts EventHub message reading process.
    ///
    /// Performs setup: database verification, EventHub connectivity check,
    /// export folder preparation, then begins message consumption.
    ///
    /// # Errors
    /// Returns error if setup verification or message reading fails.
    pub async fn start_reading(&mut self) -> Result<()> {
        // Step 1: Print some nice headers.
        Self::print_info_start_reading();

        // Step 2: Initialize and verify the local database
        println!("Checking local database access...");
        self.process_local_database_verification().await?;

        // Step 3: Check if we can reach EH
        println!("Checking if we can reach Azure EventHub...");
        self.verify_eventhub_connectivity().await?;

        println!("Preparing export config...");
        self.prepare_export_folders().await?;

        // Step 5: Give the user some feedback...
        println!("Listening for messages...");

        println!();

        // Step 6: Show the progress bar
        self.progress.print_progress();

        // Step 7: Start reading messages
        self.read_messages().await
    }

    /// Routes message reading to single partition or all partitions.
    ///
    /// Uses partition_id configuration: -1 for all partitions,
    /// specific ID for single partition reading.
    ///
    /// # Errors
    /// Returns error if partition reading fails.
    async fn read_messages(&mut self) -> Result<()> {
        if self.config.inbound_config.partition_id == -1 {
            Ok(self
                .read_all_partitions()
                .await
                .context("Failed to read messages from all partitions")?)
        } else {
            Ok(self
                .read_single_partition(&self.config.inbound_config.partition_id.to_string())
                .await
                .context("Failed to read messages from single partition")?)
        }
    }

    /// Prepares export directories if file output is enabled.
    ///
    /// Creates necessary folders for message export when read_to_file
    /// configuration is enabled.
    ///
    /// # Errors
    /// Returns error if directory setup fails.
    async fn prepare_export_folders(&mut self) -> Result<()> {
        if self.config.inbound_config.read_to_file {
            println!("Messages will be saved to file...");
            match self.setup_export_directories().await {
                Ok(_) => println!("[OK]  Export directories ready!"),
                Err(e) => {
                    println!("[FAIL] Export setup failed: {}", e);
                    anyhow::bail!("Failed to setup export directories: {}", e);
                }
            }
        } else {
            println!("Messages will be only saved to the database...");
        }
        Ok(())
    }

    /// Verifies EventHub connection and partition access.
    ///
    /// Tests connection by creating consumer client and retrieving
    /// partition information with timeout handling.
    ///
    /// # Errors
    /// Returns error if connection validation fails.
    async fn verify_eventhub_connectivity(&mut self) -> Result<()> {
        self.validate_eventhub_connection().await?;
        Ok(())
    }

    /// Verifies local database connectivity with 10-second timeout.
    ///
    /// Performs health check on Sled database to ensure read/write operations
    /// work before starting message processing.
    ///
    /// # Errors
    /// Returns error if database verification fails or times out.
    async fn process_local_database_verification(&mut self) -> Result<()> {
        let _ = tokio::time::timeout(Duration::from_secs(10), self.verify_database_async())
            .await
            .context("Failed to verify connection to local database")?;
        Ok(())
    }

    /// Prints startup information and user instructions.
    ///
    /// Logs process start time and keyboard shortcut for stopping.
    fn print_info_start_reading() {
        info!("Starting to read messages from EventHub...");
        println!("- Process started at: {}", Local::now());
        println!("Press Ctrl+C to stop.");
        println!();
    }

    /// Tests database health with insert/read/delete operations.
    ///
    /// Executes blocking database operations on separate thread
    /// to verify functionality without blocking async runtime.
    ///
    /// # Errors
    /// Returns error if database operations fail.
    async fn verify_database_async(&self) -> anyhow::Result<()> {
        // Test database operations with the async wrapper
        tokio::task::spawn_blocking({
            let db = self.db.clone();
            move || {
                let test_key = b"__health_check__";
                let test_value = b"ok";

                // Try to write and read a test value
                db.insert(test_key, test_value)?;
                match db.get(test_key)? {
                    Some(value) if value.as_ref() == test_value => {
                        // Clean up test data
                        let _ = db.remove(test_key);
                        Ok(())
                    }
                    _ => Err(anyhow!("Database read/write test failed")),
                }
            }
        })
        .await
        .map_err(|e| anyhow!("Database operation failed: {}", e))?
    }

    /// Validates EventHub connection string format and entity path.
    ///
    /// Checks for required 'Endpoint=' segment and non-empty entity path.
    ///
    /// # Errors
    /// Returns error if connection string format is invalid or entity path is empty.
    async fn validate_connection_string(&self) -> anyhow::Result<()> {
        if self.config.connection_string.is_empty() {
            return Err(anyhow!("Connection string is empty"));
        }

        if !self.config.connection_string.contains("Endpoint=") {
            return Err(anyhow!("Invalid connection string format"));
        }

        if self.config.entity_path.is_empty() {
            return Err(anyhow!("Entity path is empty"));
        }

        Ok(())
    }

    /// Validates EventHub connection and retrieves partition count.
    ///
    /// Creates test consumer client, fetches partition IDs with 15-second timeout,
    /// and validates partition properties with 10-second timeout.
    ///
    /// # Returns
    /// Number of available partitions on success.
    ///
    /// # Errors
    /// Returns error if connection fails, times out, or no partitions found.
    async fn validate_eventhub_connection(&self) -> anyhow::Result<usize> {
        // First, validate the connection string format
        self.validate_connection_string().await?;

        // Create a test consumer client to validate actual connectivity
        let mut consumer_client = self
            .config
            .create_consumer_client()
            .await
            .map_err(|e| anyhow!("Failed to create EventHub consumer client: {}", e))?;

        // Test connection by retrieving partition information with timeout
        let partition_ids = tokio::time::timeout(
            Duration::from_secs(15), // 15-second timeout for connection test
            consumer_client.get_partition_ids(),
        )
        .await
        .map_err(|_| {
            anyhow!("Connection timeout: Unable to connect to EventHub within 15 seconds")
        })?
        .map_err(|e| {
            anyhow!(
                "Failed to retrieve partition information from EventHub: {}",
                e
            )
        })?;

        if partition_ids.is_empty() {
            return Err(anyhow!(
                "EventHub '{}' has no partitions available",
                self.config.entity_path
            ));
        }

        // Validate that we can get partition properties for at least one partition
        let test_partition = &partition_ids[0];
        let _partition_props = tokio::time::timeout(
            Duration::from_secs(10),
            consumer_client.get_partition_properties(test_partition),
        )
        .await
        .map_err(|_| anyhow!("Timeout getting partition properties for validation"))?
        .map_err(|e| anyhow!("Failed to get partition properties for validation: {}", e))?;

        info!(
            "EventHub connection validated successfully. Entity: '{}', Partitions: {}",
            self.config.entity_path,
            partition_ids.len()
        );

        Ok(partition_ids.len())
    }

    /// Ensures the necessary export directories exist and are writable by creating
    /// the directory structure and testing write permissions.
    ///
    /// This function performs the following actions:
    /// 1. Resolves the path for the inbound export directory based on the configured
    ///    base data folder and received message path.
    /// 2. Creates the specified directory structure using the `tokio::fs::create_dir_all` API.
    /// 3. Writes a temporary test file to verify write permissions in the directory.
    /// 4. Removes the temporary test file upon successful completion of the writing test.
    ///
    /// # Errors
    /// - Returns an error if the directory cannot be created.
    /// - Returns an error if write permissions cannot be verified by creating the test file.
    /// - Returns an error if any filesystem operation fails during this process.
    ///
    /// # Returns
    /// - Returns `Ok(())` if the directories are successfully created and writable.
    /// - Otherwise, returns an `anyhow::Result` describing the failure.
    ///
    /// # Important Notes
    /// - This function is asynchronous and must be awaited in an async context.
    /// - Ensure that the application has enough permissions to perform file operations
    ///   in the configured directory.
    async fn setup_export_directories(&self) -> Result<()> {
        let inbound_path = resolve_path_with_base(
            &self.config.inbound_config.base_data_folder,
            &self.config.inbound_config.received_msg_path,
        );

        // Create the base export directory
        tokio::fs::create_dir_all(&inbound_path)
            .await
            .context(format!(
                "Failed to create export directory: [{}]",
                inbound_path.display()
            ))?;

        // Test write permissions by creating a test file
        let test_file = inbound_path.join(".write_test");

        tokio::fs::write(&test_file, "test").await.context(format!(
            "No write permission to export directory: [{}]",
            inbound_path.display()
        ))?;

        // Clean up a test file
        tokio::fs::remove_file(&test_file).await.context(format!(
            "Failed to clean up test file: [{}]",
            test_file.display()
        ))?;

        Ok(())
    }

    /// Reads and processes events from a single partition asynchronously.
    ///
    /// This function is responsible for creating a consumer client for a specified partition,
    /// determining the starting position from which to read events, and processing events
    /// as they are received. It uses `tokio::select!` for managing concurrent tasks
    /// effectively, including handling shutdown signals and periodic progress updates.
    ///
    /// # Arguments
    ///
    /// * `partition_id` - A string slice that holds the identifier of the partition to read from.
    ///
    /// # Workflow
    ///
    /// 1. **Initialize Consumer Client**:
    ///    - Creates a new consumer client for the given partition ID using the configuration provided in `self.config`.
    /// 2. **Determine Starting Position**:
    ///    - Retrieves the starting position for reading events using the `get_starting_position_reading_single_partition` method.
    /// 3. **Create Event Stream**:
    ///    - Starts an event stream for the given partition using the consumer client.
    /// 4. **Process Events**:
    ///    - Continuously listens for events from the partition and processes them as they are received.
    ///    - Periodically checks for shutdown signals to gracefully terminate operations.
    ///    - Shows progress updates based on configured intervals.
    /// 5. **Handle Edge Cases**:
    ///    - Handles timeouts, errors during event processing, and unexpected stream terminations.
    ///    - Closes the event stream gracefully with a timeout to avoid hanging.
    ///
    /// # Behavior
    ///
    /// - **Event Timeout Handling**: If no event is received within 5 seconds, the function continues to monitor for new events.
    /// - **Shutdown Support**: The loop checks for a shutdown signal every 100 milliseconds and terminates if the signal is set.
    /// - **Error Handling**:
    ///   - Logs errors encountered during event receiving or processing.
    ///   - Implements retries for transient issues, such as errors fetching events.
    /// - **Stream Closure**: Ensures the event stream is closed properly with a timeout to avoid resource leaks.
    ///
    /// # Logs
    ///
    /// - Logs the start of the partition reader.
    /// - Logs errors during event consumption or stream closure.
    /// - Logs when progress updates are shown.
    /// - Logs when the stream ends, either unexpectedly or after receiving a shutdown signal.
    ///
    /// # Returns
    ///
    /// * `Ok(())` on successful completion or graceful shutdown.
    async fn read_single_partition(&self, partition_id: &str) -> Result<()> {
        info!("Reading from partition: {}", partition_id);

        // Create a new consumer client for this partition
        let mut consumer_client = self
            .config
            .create_consumer_client()
            .await
            .context("Failed to create consumer client")?;

        // Determine the starting position based on checkpoint configuration
        let starting_position = self
            .get_starting_position_reading_single_partition(partition_id)
            .context("Failed to determine starting position")?;

        // Create options for reading events
        let read_options = ReadEventOptions::default();

        // Create an event stream for this partition
        let mut event_stream = consumer_client
            .read_events_from_partition(partition_id, starting_position, read_options)
            .await
            .context("Failed to create event stream")?;

        info!("Started receiving events from partition: {}", partition_id);

        // Process events as they arrive with better cancellation support
        loop {
            // Use tokio::select! for proper cancellation handling
            tokio::select! {
                // Check the shutdown signal and show progress every 100 ms
                _ = sleep(Duration::from_millis(100)) => {
                    if self.shutdown.load(Ordering::Relaxed) {
                        info!("Shutdown signal received, stopping partition {} reader", partition_id);
                        break;
                    }

                    // Show progress periodically even if no messages are being processed
                    if self.progress.should_show_progress() {
                        self.progress.print_progress();
                    }
                }

                // Try to get the next event with timeout
                event_result = tokio::time::timeout(Duration::from_secs(5), event_stream.next()) => {
                    match event_result {
                        Ok(Some(Ok(received_event))) => {
                            if self.shutdown.load(Ordering::Relaxed) {
                                break;
                            }

                            self.process_received_event(received_event, partition_id)
                            .await
                            .context("Failed to process received event")?;

                            // Always show progress after processing a message (forced update)
                            self.progress.print_progress_forced();
                        }
                        Ok(Some(Err(e))) => {
                            error!("Error receiving events from partition {}: {}", partition_id, e);
                            // Check shutdown before waiting
                            if self.shutdown.load(Ordering::Relaxed) {
                                break;
                            }

                            // Brief wait with shutdown check
                            for _ in 0..10 {
                                if self.shutdown.load(Ordering::Relaxed) {
                                    break;
                                }
                                sleep(Duration::from_millis(100)).await;
                            }
                        }
                        Ok(None) => {
                            // Stream ended
                            warn!("Event stream for partition {} ended unexpectedly", partition_id);
                            break;
                        }
                        Err(_) => {
                            // Timeout - continue listening, shutdown check will happen in the next iteration
                            continue;
                        }
                    }
                }
            }
        }

        // Close the stream with timeout to prevent hanging
        info!("Closing event stream for partition {}", partition_id);
        match tokio::time::timeout(Duration::from_secs(5), event_stream.close()).await {
            Ok(Ok(_)) => {
                info!(
                    "Event stream for partition {} closed successfully",
                    partition_id
                );
            }
            Ok(Err(e)) => {
                error!(
                    "Error closing event stream for partition {}: {}",
                    partition_id, e
                );
            }
            Err(_) => {
                warn!(
                    "Timeout closing event stream for partition {}, forcing close",
                    partition_id
                );
            }
        }

        Ok(())
    }

    /// Retrieves the starting position for reading events from a single partition in an Event Hub.
    ///
    /// This function determines the starting position of event consumption in a specified partition
    /// based on the existence of a checkpoint. Checkpoints store the latest consumed event details
    /// (such as sequence and offset) and are used to resume event processing without duplication or data loss.
    /// If checkpointing is disabled or a checkpoint doesn't exist, it starts reading from the earliest position.
    ///
    /// # Parameters
    /// - `partition_id`: A string slice that represents the identifier of the partition to read from.
    ///
    /// # Returns
    /// - `Result<EventPosition, anyhow::Error>`:
    ///   - `Ok(EventPosition)`: Returns an `EventPosition` indicating where to start event consumption.
    ///   - `Err(anyhow::Error)`: If there is an error loading the checkpoint.
    ///
    /// # Behavior
    /// - When `self.config.inbound_config.ignore_checkpoint` is `false` and a valid checkpoint exists:
    ///     - Resumes event consumption from the offset or sequence number stored in the checkpoint.
    ///     - Logs the partition, sequence number, and offset from which resumption occurs.
    /// - When `self.config.inbound_config.ignore_checkpoint` is `false` and no checkpoint exists:
    ///     - Starts reading events from the earliest position in the partition.
    ///     - Logs that no checkpoint was found.
    /// - When `self.config.inbound_config.ignore_checkpoint` is `true`:
    ///     - Ignores any existing checkpoints and starts reading events from the earliest position.
    ///     - Logs that the checkpoint is being ignored.
    ///
    /// # Logging
    /// - Logs detailed information indicating whether a checkpoint is being used (if available)
    ///   or ignored, and where the starting position for the partition is.
    ///
    /// # Errors
    /// - Returns an error if there is an issue loading the checkpoint for the specified partition.
    fn get_starting_position_reading_single_partition(
        &self,
        partition_id: &str,
    ) -> Result<EventPosition> {
        let starting_position = if !self.config.inbound_config.ignore_checkpoint {
            match self
                .load_checkpoint(partition_id)
                .context("Failed to load checkpoint")?
            {
                Some(checkpoint) => {
                    info!(
                        "Resuming from checkpoint - Partition: {}, Sequence: {}, Offset: {}",
                        partition_id, checkpoint.sequence_number, checkpoint.offset
                    );
                    // Parse offset as i64 for EventPosition
                    let offset_num: i64 = checkpoint
                        .offset
                        .parse()
                        .unwrap_or(checkpoint.sequence_number);
                    EventPosition::from_offset(offset_num, false)
                }
                None => {
                    info!(
                        "No checkpoint found for partition: {}, starting from beginning",
                        partition_id
                    );
                    EventPosition::earliest()
                }
            }
        } else {
            info!(
                "Ignoring checkpoint for partition: {}, starting from beginning",
                partition_id
            );
            EventPosition::earliest()
        };
        Ok(starting_position)
    }

    //TODO: Add summaries
    async fn read_all_partitions(&self) -> Result<()> {
        info!("Reading from all partitions...");

        let partition_ids = self
            .get_runtime_partition_ids()
            .await
            .context("Failed to get partition IDs")?;

        info!("Found {} partitions", partition_ids.len());

        // Use JoinSet for better task management
        let mut join_set = tokio::task::JoinSet::new();

        for partition_id in partition_ids {
            let reader = self.clone();

            join_set.spawn(async move {
                let result = reader
                    .read_single_partition(&partition_id)
                    .await
                    .context(format!("Failed to read partition: [{}]", &partition_id));
                (partition_id, result)
            });
        }

        // Wait for all tasks to complete with a timeout and abort mechanism
        info!("Waiting for all partition readers to complete...");
        let mut completed_count = 0;

        let mut error_count = 0;

        let start_time = std::time::Instant::now();

        // Maximum time to wait for tasks
        let max_wait_time = Duration::from_secs(30);

        while !join_set.is_empty() {
            // Check if we should force abort due to shut down timeout
            if self.shutdown.load(Ordering::Relaxed) && start_time.elapsed() > max_wait_time {
                warn!(
                    "Force aborting {} remaining partition reader tasks due to timeout",
                    join_set.len()
                );
                join_set.abort_all();
                break;
            }

            // Wait for the next task with timeout
            match tokio::time::timeout(Duration::from_secs(5), join_set.join_next()).await {
                Ok(Some(Ok((partition_id, Ok(_))))) => {
                    completed_count += 1;
                    info!("Partition {} reader completed successfully", partition_id);
                }
                Ok(Some(Ok((partition_id, Err(e))))) => {
                    error_count += 1;
                    error!("Partition {} reader failed: {}", partition_id, e);
                }
                Ok(Some(Err(join_error))) => {
                    error_count += 1;
                    if join_error.is_cancelled() {
                        info!("Partition reader task was cancelled");
                    } else {
                        error!("Partition reader task failed to join: {}", join_error);
                    }
                }
                Ok(None) => {
                    // No more tasks
                    break;
                }
                Err(_) => {
                    // Timeout waiting for task completion
                    if self.shutdown.load(Ordering::Relaxed) {
                        warn!(
                            "Timeout waiting for partition readers, {} tasks still running",
                            join_set.len()
                        );
                        // Give a bit more time for graceful completion
                        continue;
                    }
                }
            }
        }

        // Ensure database operations are fully completed
        info!("Ensuring database operations are completed...");
        if let Err(e) = self.db.flush() {
            error!("Failed to flush database: {}", e);
        }

        info!(
            "All partition readers completed: {} successful, {} errors",
            completed_count, error_count
        );

        Ok(())
    }

    /// Retrieves the runtime partition IDs from EventHub.
    ///
    /// This asynchronous function fetches the partition IDs available for the configured EventHub
    /// by creating a consumer client and querying it for the partition IDs.
    ///
    /// # Returns
    ///
    /// A `Result` containing a vector of partition ID strings if successful, or an `anyhow::Error` if an error occurs.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The consumer client cannot be created.
    /// - There's a failure in fetching partition IDs from the consumer client.
    async fn get_runtime_partition_ids(&self) -> Result<Vec<String>> {
        Ok({
            // Production mode: get partition IDs from EventHub
            let mut consumer_client = self
                .config
                .create_consumer_client()
                .await
                .context("Failed to create consumer client")?;

            consumer_client
                .get_partition_ids()
                .await
                .context("Failed to get partition IDs")?
        })
    }

    /// Processes a received event, applying filters, saving it to the database, and handling checkpoints.
    ///
    /// # Arguments
    ///
    /// * `received_event` - The event data that needs to be processed, represented by `ReceivedEventData`.
    /// * `partition_id` - A string slice representing the partition identifier where the event originated from.
    ///
    /// # Returns
    ///
    /// This function returns a `Result<()>`, which will:
    /// - Return `Ok(())` if the event is successfully processed.
    /// - Return an `anyhow::Error` if there is an error during processing.
    ///
    /// # Behavior
    ///
    /// 1. **Operation Tracking**:
    ///    - Uses an `OperationGuard` to track active operations for monitoring and manage progress states.
    ///
    /// 2. **Duplicate Check**:
    ///    - Verifies if the event has already been processed using its unique `message_key`.
    ///    - If the message exists in the database and checkpoints are not ignored, it increments the duplicate counter and exits early.
    ///
    /// 3. **Message Filtering**:
    ///    - Applies dump filters when configured. If the message does not match the filter conditions, it skips processing and increments the skipped counter.
    ///
    /// 4. **Message Processing**:
    ///    - Constructs an `InboundMessage` object using the received event's metadata and message data.
    ///    - Converts the event body (binary format) to a UTF-8 string before processing.
    ///
    /// 5. **Database Storage**:
    ///    - Serializes the `InboundMessage` object and saves it into the database using its unique `message_key`.
    ///
    /// 6. **Optional File Export**:
    ///    - If the `read_to_file` configuration is enabled, exports the processed message to an external file asynchronously.
    ///
    /// 7. **Checkpointing**:
    ///    - Saves a checkpoint for the received event after successful processing unless checkpoint saving is ignored.
    ///    - Logs an error in case of checkpoint saving issues but does not stop the processing flow.
    ///
    /// 8. **Progress Tracking**:
    ///    - Updates progress statistics such as the increment of read, skipped, and duplicated counters.
    ///    - If verbose mode is enabled, prints detailed message information to aid debugging.
    ///
    /// # Errors
    ///
    /// - Returns an `anyhow::Error` if:
    ///   - There are issues parsing or processing the event body.
    ///   - Database operations (insertion or key lookups) fail.
    ///   - Serialization of the `InboundMessage` object fails.
    ///   - File export encounters errors when enabled.
    ///
    /// # Notes
    ///
    /// - This function is heavily reliant on the configuration provided in `self.config`.
    /// - Errors during checkpoint saving are logged but do not interrupt the processing flow.
    /// - Filtering behavior is conditional on the presence and content of the dump filters.
    ///
    /// # Dependencies
    ///
    /// - External libraries for JSON serialization (`serde_json`) and date/time handling (`DateTime`, `Utc`).
    /// - Custom types like `ReceivedEventData`, `OperationGuard`, `InboundMessage`, and `MessageStatus`.
    async fn process_received_event(
        &self,
        received_event: ReceivedEventData,
        partition_id: &str,
    ) -> Result<()> {
        // Use a guard to ensure we properly track active operations
        let _guard = OperationGuard::new(&self.progress);

        let sequence_number = received_event.sequence_number();
        let event_id = sequence_number.to_string();
        let message_key = format!(
            "msg:{}:{}:{}",
            self.config.entity_path, partition_id, event_id
        );

        // Check if a message already processed (unless ignoring checkpoints)
        if !self.config.inbound_config.ignore_checkpoint && self.db.contains_key(&message_key)? {
            self.progress.increment_duplicated();
            return Ok(());
        }

        let message_data = String::from_utf8_lossy(received_event.body()?).to_string();

        // Apply dump filter if configured and not empty
        if let Some(filters) = &self.config.inbound_config.dump_filter {
            if !filters.is_empty() && !message_matches_filter(&message_data, filters) {
                self.progress.increment_skipped();
                return Ok(());
            }
        }

        let message = InboundMessage {
            id: event_id.clone(),
            event_id: event_id.clone(),
            partition_key: received_event.partition_key().map(|s| s.to_string()),
            partition_id: partition_id.to_string(),
            queued_time: DateTime::from_timestamp(
                received_event.enqueued_time().unix_timestamp(),
                0,
            )
            .unwrap_or_else(Utc::now),
            event_seq_number: Some(sequence_number),
            event_offset: received_event.offset().map(|o| o.to_string()),
            suggested_filename: None,
            processed_at: Utc::now(),
            msg_data: message_data,
            status: MessageStatus::Read,
        };

        // Store in the database
        let serialized = serde_json::to_vec(&message).context("Failed to serialize message")?;
        self.db
            .insert(&message_key, serialized)
            .context("Failed to insert message")?;

        // Export to file if configured
        if self.config.inbound_config.read_to_file {
            self.export_message_to_file(&message)
                .await
                .context("Failed to export message to file")?;
        }

        // Save checkpoint after successful processing (unless ignoring checkpoints)
        if !self.config.inbound_config.ignore_checkpoint {
            let offset_str = received_event
                .offset()
                .map(|o| o.to_string())
                .unwrap_or_else(|| sequence_number.to_string());

            if let Err(e) = self.save_checkpoint(partition_id, sequence_number, &offset_str) {
                // Logging the error and moving on. Don't want to fail the whole run here.
                error!(
                    "Failed to save checkpoint for partition {}: {}",
                    partition_id, e
                );
            }
        }

        self.progress.increment_read();

        // Only print message info if verbose mode is enabled
        if self.config.verbose {
            self.progress.print_message_info(&message);
        }

        Ok(())
    }

    async fn export_message_to_file(&self, message: &InboundMessage) -> Result<()> {
        info!("Exporting message {} to file...", message.event_id);

        // Ensure an inbound folder exists
        let inbound_path = resolve_path_with_base(
            &self.config.inbound_config.base_data_folder,
            &self.config.inbound_config.received_msg_path,
        );

        tokio::fs::create_dir_all(&inbound_path)
            .await
            .context("Failed to create inbound folder")?;

        // Create a time-based subfolder
        let date_folder = message.processed_at.format("%Y-%m-%d").to_string();

        let date_path = inbound_path.join(&date_folder);

        tokio::fs::create_dir_all(&date_path)
            .await
            .context("Failed to create date folder")?;

        // Generate filename
        let timestamp = message.processed_at.format("%Y-%m-%dT%H-%M-%S%.3f");

        let filename = format!(
            "{}--{}.txt",
            timestamp,
            message.event_offset.as_deref().unwrap_or("unknown")
        );

        let file_path = date_path.join(&filename);

        // Write content
        let content = if self.config.inbound_config.dump_content_only {
            message.msg_data.clone()
        } else {
            self.format_full_message(message)
        };

        tokio::fs::write(&file_path, content)
            .await
            .context("Failed to write message to file")?;

        info!("Exported message to: {:?}", file_path);

        Ok(())
    }

    fn format_full_message(&self, message: &InboundMessage) -> String {
        format!(
            r#"---| DETAILS      |----------------------------------------------------------
id: {}
partition key: {}
added to queue at: {}
partition id: {}
event sequence number: {}
event offset: {}
Message processed at: {}
Filename: {}

---| MESSAGE BODY |----------------------------------------------------------
{}
---|          EOF |----------------------------------------------------------
"#,
            message.event_id,
            message.partition_key.as_deref().unwrap_or(""),
            message.queued_time.format("%Y-%m-%dT%H:%M:%S%.9fZ"),
            message.partition_id,
            message.event_seq_number.unwrap_or(0),
            message.event_offset.as_deref().unwrap_or(""),
            message.processed_at.format("%Y-%m-%dT%H:%M:%S%.9fZ"),
            message.suggested_filename.as_deref().unwrap_or(""),
            message.msg_data
        )
    }

    pub fn shutdown(&self) {
        if self.shutdown.load(Ordering::Relaxed) {
            return;
        }

        self.shutdown.store(true, Ordering::Relaxed);

        println!();

        println!("🔄 Shutdown signal received, initiating cleanup...");

        // Force a quick database flush to ensure data persistence
        print!("💾 Quick-saving current progress... ");

        match self.db.flush() {
            Ok(_) => println!("[OK]  Done"),
            Err(_) => println!("  - Warning: Could not flush database"),
        }
    }

    pub async fn graceful_shutdown(&self) {
        println!();

        println!("Graceful shutdown initiated...");

        self.shutdown.store(true, Ordering::Relaxed);

        // Phase 1: Signal all tasks to stop
        println!("📡 Sending shutdown signal to all EventHub readers...");

        // Wait for shutdown to be acknowledged with a shorter timeout
        let shutdown_timeout = Duration::from_secs(5); // Reduced timeout

        let start_time = std::time::Instant::now();

        let mut last_logged_ops = u64::MAX;

        let mut progress_dots = 0;

        println!("Waiting for active operations to complete...");
        loop {
            // Check if we've exceeded the timeout
            if start_time.elapsed() > shutdown_timeout {
                println!(
                    "  - Shutdown timeout reached after {:?} seconds, forcing completion",
                    shutdown_timeout.as_secs()
                );
                // After timeout, we'll do one final flush and exit
                break;
            }

            // Check active operations count
            let active_ops = self.progress.get_active_operations();

            if active_ops == 0 {
                println!("[OK] All active operations completed successfully");
                break;
            }

            // Show progress with dots and operation count
            if active_ops != last_logged_ops {
                println!(
                    "   - {} operations still running, waiting for completion...",
                    active_ops
                );
                last_logged_ops = active_ops;
                progress_dots = 0;
            } else {
                // Show progress dots every second
                if progress_dots % 10 == 0 && progress_dots > 0 {
                    print!("   - Still waiting");
                    for _ in 0..(progress_dots / 10).min(3) {
                        print!(".");
                    }
                    println!(" ({} ops)", active_ops);
                }
                progress_dots += 1;
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        // Phase 2: Flush database to ensure all writings are persisted
        println!("Ensuring all data is saved to disk...");
        let flush_start = std::time::Instant::now();

        match tokio::task::spawn_blocking({
            let db = self.db.clone();
            move || db.flush()
        })
        .await
        {
            Ok(Ok(_)) => {
                let flush_time = flush_start.elapsed();
                println!(
                    "[OK] Database flushed successfully ({:.1}s)",
                    flush_time.as_secs_f32()
                );
            }
            Ok(Err(e)) => {
                println!("[FAIL] Failed to flush database: {}", e);
            }
            Err(_) => {
                println!("  - Database flush operation was interrupted");
            }
        }

        // Phase 3: Statistics
        let total_messages = self.progress.messages_read.load(Ordering::Relaxed);

        let total_skipped = self.progress.messages_skipped.load(Ordering::Relaxed);

        let total_duplicated = self.progress.messages_duplicated.load(Ordering::Relaxed);

        let total_runtime = self.progress.start_time.elapsed();

        println!();

        println!("Runtime Statistics:");

        println!("   - Messages processed: {}", total_messages);

        if total_skipped > 0 {
            println!("   - Messages skipped: {}", total_skipped);
        }

        if total_duplicated > 0 {
            println!("   - Duplicate messages: {}", total_duplicated);
        }

        println!("   - Total runtime: {:.1}s", total_runtime.as_secs_f32());

        if total_messages > 0 {
            let avg_rate = total_messages as f64 / total_runtime.as_secs_f64();
            let max_rate = self.progress.get_max_rate();
            println!("   - Average rate: {:.2} messages/second", avg_rate);
            println!("   - Peak rate: {:.2} messages/second", max_rate);
        }

        println!();

        println!("Graceful shutdown completed successfully!");

        println!();
    }
}

impl Clone for EventHubReader {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            db: self.db.clone(),
            progress: Arc::clone(&self.progress),
            shutdown: Arc::clone(&self.shutdown),
        }
    }
}
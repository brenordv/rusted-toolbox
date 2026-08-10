# EventHub Reader (eh_read)

## What it does

The EventHub Reader (`eh_read`) is an Azure EventHub consumer that connects to EventHub services, reads messages from
partitions, and stores them locally with checkpoint/resume functionality. 
It provides real-time progress tracking, graceful shutdown handling, and flexible message export options.

**Key Features:**
- Reads messages from single partition or all partitions concurrently
- Checkpoint/resume support for fault tolerance  
- Local Sled database for message and state persistence
- Optional file export with full message or content-only modes
- Real-time progress tracking with processing statistics
- Graceful shutdown with Ctrl+C handling
- Message filtering capabilities
- Connection validation and health checks
- Configurable feedback intervals and timeouts

## Command-Line Options
- `--config`: Path to JSON configuration file
- `--connection-string`: Azure EventHub connection string (required)
- `--entity-path`: EventHub entity/topic name (required)
- `--consumer-group`: Consumer group name (default: "$Default")
- `--partition-id`: Specific partition ID to read from (-1 for all partitions)
- `--base-data-folder`: Base folder for data storage
- `--database-path`: Local database file path
- `--received-msg-path`: Path for exported message files
- `--read-to-file`: Enable message export to files
- `--dump-content-only`: Export only message content (not metadata)
- `--ignore-checkpoint`: Start from beginning, ignoring saved checkpoints
- `--dump-filter`: Message content filters (repeatable)
- `--feedback-interval`: Progress update interval in seconds
- `--verbose`: Enable verbose logging

## Examples

### Basic Usage - Read All Partitions
**Command:**
```bash
eh_read --connection-string "Endpoint=sb://myhub.servicebus.windows.net/;SharedAccessKeyName=RootManageSharedAccessKey;SharedAccessKey=..." --entity-path "my-eventhub"
```

**Output:**
```
🚀 EventHub Reader v1.0.0
-------------------------------
🎯 Entity Path: my-eventhub
👥 Consumer Group: $Default
📊 Partition: All partitions
💾 Database: /data/eventhub.db
📁 Base Data Folder: /data
🔊 Verbose: false
📄 Read to file: false
⚡ Feedback: Every 30 second(s)

⌚ Process started at: 2024-01-15 10:30:45
Press Ctrl+C to stop.

Read: 1,250 | Skipped: 0 | Duplicated: 5 | Rate: 42.50 msg/s | Runtime: 00:00:29.456 | Last: 10:31:14
```

### Single Partition with File Export
**Command:**
```bash
eh_read --connection-string "..." --entity-path "events" --partition-id 0 --read-to-file --received-msg-path "./exports"
```

**Output:**
```
🚀 EventHub Reader v1.0.0
-------------------------------
🎯 Entity Path: events
👥 Consumer Group: $Default
📊 Partition: 0
💾 Database: /data/eventhub.db
📁 Base Data Folder: /data
🔊 Verbose: false
📄 Read to file: true
📁 Export: ./exports
📝 Content Only: false
⚡ Feedback: Every 30 second(s)

📄 Messages will be saved to file...
✅ Export directories ready!
👂 Listening for messages...

Read: 500 | Skipped: 0 | Duplicated: 0 | Rate: 15.32 msg/s | Runtime: 00:00:32.654 | Last: 10:32:18
```

### Content-Only Export with Filtering
**Command:**
```bash
eh_read --connection-string "..." --entity-path "logs" --read-to-file --dump-content-only --dump-filter "ERROR" --dump-filter "CRITICAL"
```

**Input messages:**
```text
{"level": "INFO", "message": "User logged in", "timestamp": "2024-01-15T10:30:00Z"}
{"level": "ERROR", "message": "Database connection failed", "timestamp": "2024-01-15T10:31:00Z"}
{"level": "DEBUG", "message": "Query executed", "timestamp": "2024-01-15T10:32:00Z"}
```

**Output file content:**
```
Database connection failed
```

### Resume from Checkpoint
**Command:**
```bash
eh_read --connection-string "..." --entity-path "events"
```

**Output:**
```
🔃 Checking local database access...
✅ Database connection verified!

🔃 Checking if we can reach Azure EventHub...
✅ EventHub connection validated successfully. Entity: 'events', Partitions: 4

📤 Preparing export config...
📄 Messages will be only saved to the database...

👂 Listening for messages...

INFO: Resuming from checkpoint - Partition: 0, Sequence: 15432, Offset: 1567890
INFO: Resuming from checkpoint - Partition: 1, Sequence: 12876, Offset: 1234567
...
```

### Ignore Checkpoints and Start Fresh
**Command:**
```bash
eh_read --connection-string "..." --entity-path "events" --ignore-checkpoint
```

**Output:**
```
⚠️ Ignore Checkpoint: true

INFO: Ignoring checkpoint for partition: 0, starting from beginning
INFO: Ignoring checkpoint for partition: 1, starting from beginning
...
``` 
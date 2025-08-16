# EventHub Export Tool (eh-export)

## What it does

The EventHub Export Tool (`eh-export`) reads messages previously stored by the `eh-read` tool from a local Sled database
and exports them to various file formats (TXT, CSV, JSON). It provides flexible filtering, progress tracking, checkpoint
management, and graceful shutdown capabilities for efficient bulk message processing.

**Key Features:**
- Exports messages to TXT, CSV, or JSON formats
- Optional metadata inclusion (entity path, partition ID, event ID, timestamp)
- Message filtering based on content patterns
- Checkpoint tracking to avoid duplicate exports
- Condensed output (single file) or individual files per message
- Real-time progress feedback with export statistics
- Graceful shutdown with data preservation
- Configurable via JSON file and/or command-line arguments

## Command-Line Options
- `-c, --config`: JSON configuration file path
- `--connection-string`: Azure EventHub connection string (required for database location)
- `--entity-path`: EventHub entity path (required)
- `--export-format`: Output format - txt, csv, or json (default: txt)
- `--export-folder`: Export destination folder
- `--export-base-data-folder`: Base directory for exports
- `--include-metadata`: Include message metadata in exports
- `--condense-output`: Write all messages to single file instead of individual files
- `--ignore-checkpoint`: Re-export previously exported messages
- `--dump-filter`: Content filter patterns (repeatable)
- `--feedback-interval`: Progress update interval in seconds
- `--use-local-time`: Use local time instead of UTC for timestamps

## Examples

### Basic Export - All Messages to TXT
**Command:**
```bash
eh-export --config export-config.json --export-format txt
```

**Configuration (export-config.json):**
```json
{
  "eventhubConnString": "Endpoint=sb://myhub.servicebus.windows.net/;SharedAccessKeyName=...",
  "entityPath": "my-event-hub",
  "inboundConfig": {
    "baseDiskPath": "./data",
    "databasePath": "eventhub-db"
  },
  "exportConfig": {
    "baseDiskPath": "./exports",
    "exportFolder": "messages",
    "exportFormat": "txt",
    "includeMetadata": false,
    "condenseOutput": false
  }
}
```

**Output Structure:**
```
exports/messages/
├── 2024-01/
│   ├── 01/
│   │   ├── 20240101_120000_001-evt123.txt
│   │   ├── 20240101_120001_002-evt124.txt
│   │   └── ...
│   └── 02/
└── 2024-02/
```

**Sample Output File Content:**
```
{"temperature": 23.5, "humidity": 60.2, "sensor_id": "TH001"}
```

### Export with Metadata to CSV
**Command:**
```bash
eh-export --config export-config.json --export-format csv --include-metadata --condense-output
```

**Output (messages-2024-01.csv):**
```csv
entity_path,partition_id,event_id,timestamp,message_content
my-event-hub,0,evt123,2024-01-01T12:00:00.000000000Z,"{""temperature"": 23.5, ""humidity"": 60.2}"
my-event-hub,1,evt124,2024-01-01T12:00:01.000000000Z,"{""pressure"": 1013.25, ""location"": ""warehouse""}"
```

### Filtered Export with JSON Format
**Command:**
```bash
eh-export --config export-config.json --export-format json --dump-filter "temperature" --dump-filter "sensor" --condense-output
```

**Output (messages-2024-01.json):**
```json
[
  {
    "message_content": "{\"temperature\": 23.5, \"humidity\": 60.2, \"sensor_id\": \"TH001\"}"
  },
  {
    "message_content": "{\"temperature\": 22.1, \"sensor_id\": \"TH002\"}"
  }
]
```

### Resume Export from Checkpoint
**Command:**
```bash
eh-export --config export-config.json --export-format txt --feedback-interval 5
```
*Skips previously exported messages using checkpoint database*

**Progress Output:**
```
🚀 Starting export process...
Exported: 1250 | Skipped: 0 | Duplicated: 340 | Rate: 125.50 msg/s | Runtime: 00:00:12.6540
✅ Export completed successfully!
```
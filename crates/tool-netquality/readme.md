# NetQuality

A cross-platform CLI that monitors connectivity and speed, stores activity in SQLite, and sends notifications via Telegram (and optionally OpenTelemetry). It runs a simple 1-second loop that schedules connectivity and speed checks using configurable delays and backoff rules.

## What It Does
- Runs periodic connectivity checks against a rotating URL list
- Runs speed tests only when connectivity is up
- Stores connectivity/speed activity in SQLite with session linking
- Notifies on outage recovery and speed threshold changes

## Command-Line Options
- `-c, --config <FILE>`: Path to `config.json` (optional)
- `--url <URL>`: Connectivity check URL (repeatable)
- `--replace-urls`: Replace default URL list instead of merging
- `--expected-download <MBPS>`: Expected download speed in Mbps (required if not in config)
- `--expected-upload <MBPS>`: Expected upload speed in Mbps (optional)
- `--download-thresholds <V,S,M,MF>`: Download thresholds as percentages (e.g. `30,50,65,85`)
- `--upload-thresholds <V,S,M,MF>`: Upload thresholds as percentages (e.g. `30,50,65,85`)
- `--connectivity-delay <SECS>`: Connectivity check delay in seconds
- `--speed-delay <SECS>`: Speed check delay in seconds
- `--connectivity-timeout <SECS>`: Connectivity request timeout in seconds
- `--outage-backoff <SECS>`: Outage backoff delay in seconds
- `--outage-backoff-max <SECS>`: Maximum outage backoff delay in seconds
- `--db-path <FILE>`: SQLite database path
- `--telegram-token <TOKEN>`: Telegram bot token
- `--telegram-chat-id <CHAT>`: Telegram chat ID
- `--otel-endpoint <URL>`: OpenTelemetry OTLP endpoint
- `-v, --verbose`: Enable verbose logs

## Configuration Loading
NetQuality loads configuration in this order:
1. `config.json` in the current working directory
2. `config.json` next to the executable
3. Command-line overrides

If `--config` is provided, that file is used instead of the default lookup.
If `db_path` is not provided, the database defaults to `netquality.db` next to the executable.

## Example `config.json`
```json
{
  "storage": {
    "db_path": "netquality.db"
  },
  "connectivity": {
    "delay_secs": 10,
    "timeout_secs": 1,
    "outage_backoff_secs": 10,
    "outage_backoff_max_secs": 3600,
    "url_mode": "merge",
    "urls": [
      "https://example.com/health",
      "https://1.1.1.1"
    ]
  },
  "speed": {
    "expected_download_mbps": 100.0,
    "expected_upload_mbps": 20.0,
    "delay_secs": 14400,
    "download_thresholds": {
      "very_slow": 30.0,
      "slow": 50.0,
      "medium": 65.0,
      "medium_fast": 85.0
    },
    "upload_thresholds": {
      "very_slow": 30.0,
      "slow": 50.0,
      "medium": 65.0,
      "medium_fast": 85.0
    }
  },
  "notifications": {
    "telegram": {
      "bot_token": "YOUR_BOT_TOKEN",
      "chat_id": "YOUR_CHAT_ID"
    },
    "otel_endpoint": "http://localhost:4318"
  }
}
```

## Examples
### Run with defaults + CLI overrides
```bash
netquality --expected-download 100 --expected-upload 20 --telegram-token TOKEN --telegram-chat-id 123
```

### Use a custom config file
```bash
netquality --config ./config.json
```

### Replace the URL list
```bash
netquality --expected-download 200 --replace-urls --url https://example.com/health --url https://1.1.1.1
```

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
- `--min-download-threshold <THRESHOLD>`: Minimum download threshold to trigger notifications (`very_slow|slow|medium|medium_fast|expected`)
- `--min-upload-threshold <THRESHOLD>`: Minimum upload threshold to trigger notifications (`very_slow|slow|medium|medium_fast|expected`)
- `--connectivity-delay <SECS>`: Connectivity check delay in seconds
- `--speed-delay <SECS>`: Speed check delay in seconds
- `--connectivity-timeout <SECS>`: Connectivity request timeout in seconds
- `--outage-backoff <SECS>`: Outage backoff delay in seconds
- `--outage-backoff-max <SECS>`: Maximum outage backoff delay in seconds
- `--db-path <FILE>`: SQLite database path
- `--speedtest-cli-path <FILE>`: Path to Ookla `speedtest` CLI binary
- `--telegram-token <TOKEN>`: Telegram bot token
- `--telegram-chat-id <CHAT>`: Telegram chat ID
- `--otel-endpoint <URL>`: OpenTelemetry OTLP endpoint
- `-v, --verbose`: Enable verbose logs

## Configuration Loading
NetQuality loads configuration in this order:
1. `config.json` next to the executable
2. `config.json` in the current working directory
3. `--config` (if provided)
4. Command-line overrides
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
    "speedtest_cli_path": "C:\\Program Files\\Speedtest\\speedtest.exe",
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
    "otel_endpoint": "http://localhost:4318",
    "min_download_threshold": "medium_fast",
    "min_upload_threshold": "slow"
  }
}
```

## Ookla Speedtest CLI (optional)
If `speedtest_cli_path` is set (or `--speedtest-cli-path` is provided), NetQuality will use the Ookla CLI.
If the binary fails, NetQuality logs the error and falls back to the built-in Cloudflare test.

### Install (macOS)
```bash
brew tap teamookla/speedtest
brew update
brew install speedtest --force
```

### Install (Ubuntu/Debian)
```bash
sudo apt-get install curl
curl -s https://packagecloud.io/install/repositories/ookla/speedtest-cli/script.deb.sh | sudo bash
sudo apt-get install speedtest
```

### Install (Fedora/CentOS/RHEL)
```bash
curl -s https://packagecloud.io/install/repositories/ookla/speedtest-cli/script.rpm.sh | sudo bash
sudo yum install speedtest
```

### Install (Windows)
Download the official zip and extract `speedtest.exe`:
`https://install.speedtest.net/app/cli/ookla-speedtest-1.2.0-win64.zip`

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

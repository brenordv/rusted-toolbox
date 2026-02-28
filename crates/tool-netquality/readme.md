# NetQuality
A cross-platform CLI that monitors connectivity and speed, stores activity in SQLite, and sends notifications via 
Telegram, and optionally OpenTelemetry. 

It runs a simple 1-second loop that schedules connectivity and speed checks using configurable delays and backoff rules.

## What It Does
- Runs periodic connectivity checks against a rotating URL list
- Runs speed tests (only when connectivity is up)
- Stores connectivity/speed activity in SQLite, organized by session (connectivity check + speed test executed inside the same loop)
- Cleans up activity older than 1 year (configurable interval)
- Notifies on outage recovery and speed threshold changes

## Command-Line Options
- `-c, --config <FILE>`: Path to `config.json` (optional)
- `--url <URL>`: Connectivity check URL (repeatable)
- `--replace-urls`: Replace a default URL list instead of merging
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

## Configuration Loading order and overrides
To make this tool simpler to use, NetQuality loads configuration in this order:
1. `config.json` in the same folder as the executable, then...
2. `config.json` in the current working directory, and then...
3. `--config` file (if provided), and lastly...
4. Command-line overrides.

I designed it this way so you can keep things like notification tokens and chat IDs in a single file next to the 
 executable and then override just what you need.

## Configuration defaults
Here are the built-in defaults NetQuality uses when a setting is not provided:
- `storage.db_path`: `netquality.db` next to the executable
- `storage.cleanup_enabled`: `true`
- `storage.cleanup_interval_days`: `365` days
- `connectivity.delay_secs`: `60`
- `connectivity.timeout_secs`: `1`
- `connectivity.outage_backoff_secs`: `10`
- `connectivity.outage_backoff_max_secs`: `3600`
- `connectivity.url_mode`: `merge`
- `connectivity.urls`: Google, Cloudflare, and public DNS endpoints:
  - `https://www.google.com/generate_204`
  - `https://www.cloudflare.com/cdn-cgi/trace`
  - `https://1.1.1.1`
  - `https://8.8.8.8`
- `speed.expected_download_mbps`: no default (required)
- `speed.expected_upload_mbps`: not set (upload checks disabled)
- `speed.delay_secs`: `14400` (4 hours)
- `speed.download_thresholds` / `speed.upload_thresholds`:
  - `very_slow`: `30`
  - `slow`: `50`
  - `medium`: `65`
  - `medium_fast`: `85`
- `speed.speedtest_cli_path`: not set (uses the embedded Cloudflare test)
- `notifications.telegram`: not set
- `notifications.otel_endpoint`: not set
- `notifications.min_download_threshold`: `medium`
- `notifications.min_upload_threshold`: `slow`

## Example `config.json`
```json
{
  "storage": {
    "db_path": "netquality.db",
    "cleanup_enabled": true,
    "cleanup_interval_days": 365
  },
  "connectivity": {
    "delay_secs": 60,
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
# About the Connectivity Check
This is a simple HTTP GET request to a predefined list of URLs. If all of them fail, NetQuality considers the network 
down.

For this, we don't care about the actual response body or status code. If it is 2xx, 3xx, 4xx, or 5xx, this means 
someone replied. So the internet is working.

# About the Speed Test
The embedded speed test is performed using the Cloudflare speed test server, which is not guaranteed to be reliable on 
fast connections That's why I added the option to use an external speed test tool from a trusted third party source.

If you have fast internet connections (1 gbps or more), consider using an external speed test tool for more accurate 
results.

We'll use the Cloudflare speed test server on two ocasions:
- When the third party speed test tool is not informed in the configuration.
- When the third party speed test tool fails for any reason..

## Ookla Speedtest CLI (optional)
If `speedtest_cli_path` is set (or `--speedtest-cli-path` is provided), NetQuality will use the Ookla CLI.
If the binary fails, NetQuality logs the error and falls back to the built-in Cloudflare test.

### Install
To install Ookla's CLI, you can follow the instructions on their [official page](https://www.speedtest.net/apps/cli),
or the guides below.

#### Install (macOS)
```bash
brew tap teamookla/speedtest
brew update
brew install speedtest --force
```

#### Install (Ubuntu/Debian)
```bash
sudo apt-get install curl
curl -s https://packagecloud.io/install/repositories/ookla/speedtest-cli/script.deb.sh | sudo bash
sudo apt-get install speedtest
```

#### Install (Fedora/CentOS/RHEL)
```bash
curl -s https://packagecloud.io/install/repositories/ookla/speedtest-cli/script.rpm.sh | sudo bash
sudo yum install speedtest
```

#### Install (Windows)
Download the official zip and extract `speedtest.exe`:
`https://install.speedtest.net/app/cli/ookla-speedtest-1.2.0-win64.zip`

# Database Schema
Every activity is stored in the SQLite database. We use the following tables:
- `sessions`: stores which checks were performed in the current session.
- `activity_speed`: stores speed test results.
- `activity_connectivity`: stores connectivity check results.

## `sessions` table
Tracks which activities ran in the same loop tick (one session per loop).
```sql
CREATE TABLE sessions (
  session_id INTEGER PRIMARY KEY AUTOINCREMENT,
  parent_session_id INTEGER,
  connectivity_id INTEGER,
  speed_id INTEGER,
  FOREIGN KEY(connectivity_id) REFERENCES activity_connectivity(activity_id),
  FOREIGN KEY(speed_id) REFERENCES activity_speed(activity_id),
  FOREIGN KEY(parent_session_id) REFERENCES sessions(session_id)
);
```
- `session_id`: unique ID for the loop iteration.
- `parent_session_id`: optional link to a parent session (reserved for grouping checks done during an outage).
- `connectivity_id`: optional link to the connectivity check row.
- `speed_id`: optional link to the speed test row.

## `activity_speed` table
Stores the results of speed tests (one row per run).
```sql
CREATE TABLE activity_speed (
  activity_id INTEGER PRIMARY KEY AUTOINCREMENT,
  timestamp TEXT NOT NULL,
  download_speed REAL NOT NULL,
  upload_speed REAL,
  download_threshold TEXT NOT NULL,
  upload_threshold TEXT,
  success INTEGER NOT NULL,
  elapsed_time INTEGER NOT NULL
);
```
- `timestamp`: RFC3339 timestamp when the check ran.
- `download_speed` / `upload_speed`: measured Mbps.
- `download_threshold` / `upload_threshold`: label selected from thresholds.
- `success`: `1` for success, `0` for failure.
- `elapsed_time`: duration in milliseconds.

## `activity_connectivity` table
Stores connectivity checks (one row per URL attempt group).
```sql
CREATE TABLE activity_connectivity (
  activity_id INTEGER PRIMARY KEY AUTOINCREMENT,
  timestamp TEXT NOT NULL,
  url TEXT NOT NULL,
  result TEXT NOT NULL,
  elapsed_time INTEGER NOT NULL,
  success INTEGER NOT NULL
);
```
- `timestamp`: RFC3339 timestamp when the check ran.
- `url`: URL that was selected for the successful response (or last attempted).
- `result`: HTTP status code or error label.
- `elapsed_time`: duration in milliseconds.
- `success`: `1` for success, `0` for failure.

## Viewing summarized results
I have created a simple view (`session_activity_view`) that can be queried to see a summary of the results.

Example output:

| session_id | parent_session_id | connectivity_timestamp              | connectivity_url                         | connectivity_result | connectivity_elapsed_time | connectivity_success | speed_timestamp                     | download_speed | upload_speed | download_threshold | upload_threshold | speed_success | speed_elapsed_time |
|------------|-------------------|-------------------------------------|------------------------------------------|---------------------|---------------------------|----------------------|-------------------------------------|----------------|--------------|--------------------|------------------|---------------|--------------------|
| 1          |                   | 2026-01-23T18:09:02.075641300+00:00 | https://www.google.com/generate_204      | 204                 | 138                       | 1                    | 2026-01-23T18:09:16.152238600+00:00 | 947.367824     | 815.693904   | Expected           | Medium Fast      | 1             | 14073              |
| 2          |                   | 2026-01-23T18:10:02.633350300+00:00 | https://www.cloudflare.com/cdn-cgi/trace | 200                 | 63                        | 1                    |                                     |                |              |                    |                  |               |                    |
| 3          |                   | 2026-01-23T18:11:03.232561400+00:00 | https://1.1.1.1                          | 200                 | 137                       | 1                    |                                     |                |              |                    |                  |               |                    |
| 4          |                   | 2026-01-23T18:12:03.815837200+00:00 | https://8.8.8.8                          | 200                 | 95                        | 1                    |                                     |                |              |                    |                  |               |                    |
| 5          |                   | 2026-01-23T18:13:04.378344300+00:00 | https://www.google.com/generate_204      | 204                 | 71                        | 1                    |                                     |                |              |                    |                  |               |                    |
| 6          |                   | 2026-01-23T18:14:04.816723900+00:00 | https://www.cloudflare.com/cdn-cgi/trace | 200                 | 14                        | 1                    |                                     |                |              |                    |                  |               |                    |
| 7          |                   | 2026-01-23T18:15:05.400251500+00:00 | https://1.1.1.1                          | 200                 | 85                        | 1                    |                                     |                |              |                    |                  |               |                    |



# Usage Examples
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

# Creating alerts
If you are using the open telemetry instrumentation, you can create alerts, like:
1. Alert for `Internet is Down`: No spans for service netquality for N minutes (dead-man switch)
2. Alert for `Internet speed degraded`: Span name `netquality.notification`, and `notification.message` contains "Speed change detected"
3. Internet is back up: Span name `netquality.notification`, and `notification.message` contains "Outage ended"
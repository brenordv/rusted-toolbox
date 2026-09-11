# MQTT CLI tool
## What it does

The MQTT CLI tool is a fast and intuitive command-line utility for publishing messages to and subscribing to MQTT 
topics. It provides a simple interface for interacting with MQTT brokers, making it very convenient for IoT testing, 
debugging message flows, or quickly sending/receiving messages during development.

**Key features:**
- **Publish & subscribe**: Send messages to topics or listen for incoming messages
- **Authentication support**: Connect anonymously or with username/password credentials
- **Real-time messaging**: Async implementation for high-performance message handling

## Command-line options
- `command`: Operation to perform - `read`/`reads` (subscribe) or `post`/`send` (publish)
- `-o, --host`: MQTT broker host to connect to (required)
- `-p, --port`: MQTT broker port (default: 1883)
- `-t, --topic`: MQTT topic to publish to or subscribe from (required)
- `-m, --message`: Message content to publish (required for post command)
- `-u, --username`: Username for authenticated connections (optional)
- `-a, --password`: Password for authenticated connections (optional)

**Note**: When using authentication, both username and password must be provided together.

### Shared runtime options
- `--app-header`: Print the tool name, version, and runtime configuration
- `--log-level <LEVEL>`: Log level: trace | debug | info | warn | error | disabled (case insensitive; default: warn)
- `--log-to-console`: Log to stdout instead of the default stderr
- `--log-to-file`: Also write logs to a file
- `--rotate-log-file-by-day`: Rotate the log file by day

## Examples
### Subscribe to a topic (anonymous)
**Command:**
```bash
mqtt read --host broker.hivemq.com --topic sensors/temperature
```
**Behavior:** Continuously listens for messages on the `sensors/temperature` topic and prints each payload to
stdout as it arrives, one line per message, with control characters escaped. Logs stay on stderr, so stdout can be
piped: a single-line JSON payload works with `mqtt read ... | jq .` as is, while a multi-line payload arrives
escaped onto one line. Avoid `--log-to-console` when piping: it routes logs to stdout and they interleave
with the payloads.

### Subscribe with the runtime header (authenticated)
**Command:**
```bash
mqtt read --host my-broker.com --port 8883 --topic private/data --username myuser --password mypass --app-header
```
**Output:**
```
mqtt (2.3.0)
---------------------------------------------------
- Basic Runtime Config
  - Verbose mode: <unused>
  - Log level: Warning
  - Log to stdout: false
  - Log to file: false
  - Rotate log file by day: false
- Tool Runtime Config
  - Host: my-broker.com:8883
  - Connection type: Authenticated
  - Topic: private/data
  - Command: Read
```
The header block is printed only when `--app-header` is passed.

### Publish a message (anonymous)
**Command:**
```bash
mqtt post --host broker.hivemq.com --topic sensors/temperature --message "22.5"
```
**Behavior:** Publishes the message "22.5" to the `sensors/temperature` topic and waits for the broker's
acknowledgment before exiting.

### Publish to a custom port with the send alias
**Command:**
```bash
mqtt send --host localhost --port 1884 --topic test/message --message "Hello MQTT!"
```
**Behavior:** Same as `post`; `send` is a visible alias. For a post command, the `--app-header` block also shows
the message under the command line:
```
  - Command: Post
    - Message: Hello MQTT!
```

### Authenticated message publishing
**Command:**
```bash
mqtt post --host secure-broker.com --topic alerts/system --message "System online" --username admin --password secret123
```
**Behavior:** Connects with the given credentials and publishes the message.

### Real-time message monitoring
**Command:**
```bash
mqtt reads --host test.mosquitto.org --topic home/+/temperature
```
**Input:** Multiple devices publishing to topics like `home/kitchen/temperature`, `home/bedroom/temperature`
**Output:** One line per message on stdout, payload only:
```
21.3
19.8
23.1
...
```

## Technical details
### MQTT protocol support
- **Protocol version**: MQTT 3.1.1 via rumqttc library
- **Transport**: TCP connections to MQTT brokers
- **QoS levels**: 
  - AtMostOnce (QoS 0) for subscription
  - AtLeastOnce (QoS 1) for publishing with acknowledgment
- **Keep-alive**: 5-second interval for connection maintenance

## Command aliases
The tool supports multiple command aliases for convenience:
- **Read/Subscribe**: `read`, `reads`
- **Publish**: `post`, `send`

## Known issues
1. **Message size**: No explicit message size limits, but very large messages may impact performance
2. **Topic wildcards**: Wildcard subscriptions (`+`, `#`) are supported by the broker but the tool treats them as literal topic names in validation
3. **SSL/TLS**: Currently only supports unencrypted TCP connections; secure connections are not implemented
4. **Persistent sessions**: Does not support persistent MQTT sessions; each connection is clean session
5. **Binary messages**: Binary payloads are lossy-converted to UTF-8 for display (invalid bytes become the U+FFFD replacement character, with a warning) and printed with control characters escaped; the raw bytes are not shown

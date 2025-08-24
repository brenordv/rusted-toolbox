# MQTT CLI Tool
## What It Does

The MQTT CLI tool is a fast and intuitive command-line utility for publishing messages to and subscribing to MQTT 
topics. It provides a simple interface for interacting with MQTT brokers, making it very convenient for IoT testing, 
debugging message flows, or quickly sending/receiving messages during development.

**Key Features:**
- **Publish & Subscribe**: Send messages to topics or listen for incoming messages
- **Authentication Support**: Connect anonymously or with username/password credentials
- **Real-time Messaging**: Async implementation for high-performance message handling

## Command-Line Options
- `command`: Operation to perform - `read`/`reads` (subscribe) or `post`/`send` (publish)
- `-o, --host`: MQTT broker host to connect to (required)
- `-p, --port`: MQTT broker port (default: 1883)
- `-t, --topic`: MQTT topic to publish to or subscribe from (required)
- `-m, --message`: Message content to publish (required for post command)
- `-u, --username`: Username for authenticated connections (optional)
- `-a, --password`: Password for authenticated connections (optional)

**Note**: When using authentication, both username and password must be provided together.

## Examples
### Subscribe to Topic (Anonymous)
**Command:**
```bash
mqtt read --host broker.hivemq.com --topic sensors/temperature
```
**Output:**
```
✉️ MQTT v1.0.0
---------------------------
Host: broker.hivemq.com:1883
Connection type: Anonymous
Topic: sensors/temperature
Command: Read
```
**Behavior:** Continuously listens for messages on the `sensors/temperature` topic and displays them as they arrive.

### Subscribe to Topic (Authenticated)
**Command:**
```bash
mqtt read --host my-broker.com --port 8883 --topic private/data --username myuser --password mypass
```
**Output:**
```
✉️ MQTT v1.0.0
---------------------------
Host: my-broker.com:8883
Connection type: Authenticated
Topic: private/data
Command: Read
```

### Publish Message (Anonymous)
**Command:**
```bash
mqtt post --host broker.hivemq.com --topic sensors/temperature --message "22.5"
```
**Output:**
```
✉️ MQTT v1.0.0
---------------------------
Host: broker.hivemq.com:1883
Connection type: Anonymous
Topic: sensors/temperature
Command: Post
Message: 22.5
```
**Behavior:** Publishes the message "22.5" to the `sensors/temperature` topic and waits for acknowledgment.

### Publish to Custom Port
**Command:**
```bash
mqtt send --host localhost --port 1884 --topic test/message --message "Hello MQTT!"
```
**Output:**
```
✉️ MQTT v1.0.0
---------------------------
Host: localhost:1884
Connection type: Anonymous
Topic: test/message
Command: Post
Message: Hello MQTT!
```

### Authenticated Message Publishing
**Command:**
```bash
mqtt post --host secure-broker.com --topic alerts/system --message "System online" --username admin --password secret123
```
**Output:**
```
✉️ MQTT v1.0.0
---------------------------
Host: secure-broker.com:1883
Connection type: Authenticated
Topic: alerts/system
Command: Post
Message: System online
```

### Real-time Message Monitoring
**Command:**
```bash
mqtt read --host test.mosquitto.org --topic home/+/temperature
```
**Input:** Multiple devices publishing to topics like `home/kitchen/temperature`, `home/bedroom/temperature`
**Output:** 
```
✉️ MQTT v1.0.0
---------------------------
Host: test.mosquitto.org:1883
Connection type: Anonymous
Topic: home/+/temperature
Command: Read

Message received: "21.3"
Message received: "19.8"
Message received: "23.1"
...
```

## Technical Details
### MQTT Protocol Support
- **Protocol Version**: MQTT 3.1.1 via rumqttc library
- **Transport**: TCP connections to MQTT brokers
- **QoS Levels**: 
  - AtMostOnce (QoS 0) for subscription
  - AtLeastOnce (QoS 1) for publishing with acknowledgment
- **Keep-Alive**: 5-second interval for connection maintenance

## Command Aliases
The tool supports multiple command aliases for convenience:
- **Read/Subscribe**: `read`, `reads`
- **Publish**: `post`, `send`

## Known Issues
1. **Message Size**: No explicit message size limits, but very large messages may impact performance
2. **Topic Wildcards**: Wildcard subscriptions (`+`, `#`) are supported by the broker but the tool treats them as literal topic names in validation
3. **SSL/TLS**: Currently only supports unencrypted TCP connections; secure connections are not implemented
4. **Persistent Sessions**: Does not support persistent MQTT sessions; each connection is clean session
5. **Binary Messages**: Binary payloads are converted to UTF-8 strings, which may not display correctly for non-text data
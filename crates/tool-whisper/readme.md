# Whisper
## What it does
Whisper is a bare-bones, secure, and private peer-to-peer chat application that enables encrypted real-time 
communication between two parties over TCP.

It creates a secure channel using RSA 4096-bit encryption with automatic key exchange, ensuring that all messages are
encrypted end-to-end without relying on external servers or third-party services.

**Key Features:**
- End-to-end RSA 4096-bit encryption with automatic key exchange
- Direct peer-to-peer communication (no central server required)
- Terminal-based user interface with real-time messaging
- Host/client architecture with flexible connection options
- Message integrity and privacy protection
- Cross-platform networking support
- No logging or message history
- Fully private/anonymous

## Command-Line Options
- `-w, --wait [PORT]`: Host mode - Listen for connections on specified port (default: 2428)
- `-c, --connect <HOST:PORT>`: Client mode - Connect to specified host and port
- `-b, --bind-to-all-interfaces`: Bind to all network interfaces instead of localhost only

**Note**: You must specify either `--wait` or `--connect`. The default port 2428 corresponds to "CHAT" in T9 keypad notation.

## Examples

### Host Mode - Wait for Connection
**Command:**
```bash
whisper --wait
```

**Input:** Start server on default port 2428, listening on localhost
**Output:** 
```
Generating Keypair...
Generated keypair with fingerprint: A1B2C3D4E5F6
Initializing listener on: 127.0.0.1:2428
Waiting for someone to talk to...
```

### Host Mode - Custom Port and All Interfaces
**Command:**
```bash
whisper --wait 3000 --bind-to-all-interfaces
```

**Input:** Listen on port 3000, accessible from any network interface
**Output:**
```
Generating Keypair...
Generated keypair with fingerprint: A1B2C3D4E5F6
Initializing listener on: 0.0.0.0:3000
Waiting for someone to talk to...
```

### Client Mode - Connect to Host
**Command:**
```bash
whisper --connect 192.168.1.100:2428
```

**Input:** Connect to a Whisper host running on 192.168.1.100:2428
**Output:**
```
Generating Keypair...
Generated keypair with fingerprint: F6E5D4C3B2A1
Connecting to: 192.168.1.100:2428
Connected! Creating connection manager...
Starting handshake...
Handshake completed!
```

### Complete Chat Session Example
**Host Side:**
```bash
$ whisper --wait
Generating Keypair...
Generated keypair with fingerprint: HOST123456
Initializing listener on: 127.0.0.1:2428
Waiting for someone to talk to...
Client connected! Client address: 127.0.0.1:54321
Starting handshake...
Handshake completed!

┌───────────────────────────────────────────┐
│ whisper | v1.0.0                          │
├───────────────────────────────────────────┤
│ > Hello! This is the host speaking        │
│ < Hi! Client here, connection works great │
│ > Great! How's the encryption?            │
│ < Perfect - all messages are secure       │
├───────────────────────────────────────────┤
│ What's on your mind?                      │
│ [cursor here]                             │
├───────────────────────────────────────────┤
│ Press Esc to stop editing, Enter to send │
└───────────────────────────────────────────┘
```

**Client Side:**
```bash
$ whisper --connect 127.0.0.1:2428
Generating Keypair...
Generated keypair with fingerprint: CLIENT789ABC
Connecting to: 127.0.0.1:2428
Connected! Creating connection manager...
Starting handshake...
Handshake completed!

┌───────────────────────────────────────────┐
│ whisper | v1.0.0                          │
├───────────────────────────────────────────┤
│ < Hello! This is the host speaking        │
│ > Hi! Client here, connection works great │
│ < Great! How's the encryption?            │
│ > Perfect - all messages are secure       │
├───────────────────────────────────────────┤
│ What's on your mind?                      │
│ [cursor here]                             │
├───────────────────────────────────────────┤
│ Press Esc to stop editing, Enter to send │
└───────────────────────────────────────────┘
```

## User Interface Controls

**In Normal Mode:**
- `e`: Enter editing mode to type messages
- `q`: Quit the application

**In Editing Mode:**
- `Enter`: Send the typed message
- `Esc`: Return to normal mode
- `←/→`: Move cursor left/right within message
- `Backspace`: Delete character before cursor
- Type normally to enter text

## Technical Details

### Encryption
- **Algorithm**: RSA with PKCS#1 v1.5 padding
- **Key Size**: 4096 bits for maximum security
- **Key Exchange**: Automatic public key exchange during handshake
- **Fingerprint**: SHA-256 hash of a public key (first 12 characters displayed)

### Network Protocol
- **Transport**: TCP for reliable message delivery
- **Message Format**: Length-prefixed binary protocol
- **Header**: 4-byte big-endian message length
- **Payload**: Base64-encoded encrypted message content
- **Connection**: Direct peer-to-peer, no intermediary servers

### Security Considerations
- Each session generates a new RSA keypair
- All messages are encrypted with the peer's public key
- Private keys never leave the local machine
- No message history is stored after the session ends

## Known Issues

1. **Message Size Limitation**: RSA encryption limits message size to approximately 446 bytes for 4096-bit keys. Longer messages will fail to encrypt. I'll probably improve this later.
2. **No File Transfer**: Only text messages are supported; no file sharing capabilities.
3. **Network Dependency**: Requires direct network connectivity between peers; doesn't work through NAT without port forwarding.
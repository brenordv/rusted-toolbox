# PingX

A cross-platform ping-style CLI with extra features. 
Like classic ping, but with convenient output modes (CSV/JSON/custom templates), periodic stats, timestamping, 
and IPv4/IPv6 controls.

## What It Does
PingX sends ICMP echo requests to a target host/IP at a configurable interval, reports per-packet round-trip times,
and prints summary statistics. It supports human-friendly output, machine-readable CSV/JSON, and fully customizable
line templates.

## Features
- IPv4/IPv6 auto-detection or explicit selection
- Adjustable packet count, interval, payload size, TTL, per-reply timeout, and overall deadline
- Output modes: default (human), CSV, JSON (aggregated), or a custom template with tags
- Optional timestamp prefix on every line
- Periodic statistics printing (e.g., every N seconds)
- Quiet mode (summary only), compact header, and "no header" mode
- Optional beep on packet loss
- Reverse DNS lookup (disable with --numeric)

## Usage
```bash
pingx <target> [OPTIONS]
```

## Command-Line Options
- target                         Hostname or IP address to ping (required)
- -c, --count <N>                Number of packets to send (-1 for infinite; default: -1 when omitted)
- -i, --interval <SECS>          Interval between packets in seconds, fractional value allowed (default: 1.0)
- -s, --size <BYTES>             ICMP payload size (default: 56)
- -w, --timeout <SECS>           Per reply timeout (default: 2.0)
- -W, --deadline <SECS>          Stop after total elapsed seconds (optional)
- -T, --continuous               Ping until interrupted (Ctrl+C)
- -4, --ipv4                     Force IPv4
- -6, --ipv6                     Force IPv6
- -D, --timestamp                Prefix each reply with an RFC3339 timestamp
- -q, --quiet                    Quiet mode: suppress packet lines, print only summary
- -v, --verbose                  Verbose output (extra diagnostics)
- -n, --numeric                  Do not perform reverse DNS lookup
- -o, --output <MODE|TEMPLATE>   Output mode: default | csv | json | or a custom template string
- -e, --stats-every <SECS>       Print stats every N seconds
- -b, --beep                     Beep on packet loss
- -m, --compact-header           Print a compact header (one-line, ping-like)
- -p, --no-header                Do not print the initial header

Notes:
- --ipv4 and --ipv6 are mutually exclusive.
- --count must be -1 (for infinite, although might be better to use `--continuous` in this case) or >= 1.

## Output Modes
- default: Human readable per-packet lines and final summary
- csv: Header line then one CSV line per packet; final CSV stats line
  - Header columns (without --timestamp): `host,ip,reverse_dns,size,icmp_seq,time`
  - With --timestamp: `timestamp,host,ip,reverse_dns,size,icmp_seq,time`
  - Periodic/final stats line format: `stats,<sent>,<received>,<loss%>`
- json: Emits a single JSON object with metadata and an array of packets at the end (no per-packet prints)
- template: Use a custom template string with the tags listed below

### Template Tags
Use any of these (case-insensitive) in a custom template string passed to --output. At least one tag is required.
- %host%
- %ip%
- %reverse_dns%
- %size%               (payload + IP/ICMP headers)
- %size_no_headers%    (payload only)
- %icmp_seq%
- %time%               (ms, with two decimals)
- %timestamp%          (RFC3339)
- %error%              (error message if a packet failed)

Tip: If your template contains %timestamp%, PingX will avoid adding an extra timestamp prefix even if --timestamp is set.

## Examples
### Basic ping
```bash
pingx google.com --count 3
```
Sample output:
```
XPing v1.0.0
---------------------------
- Host: google.com
- IP: 142.250.72.14
- Reverse DNS: lhr25s10-in-f14.1e100.net
- Packet Size: 56 (with headers: 84)
- Count: 3

64 bytes from lhr25s10-in-f14.1e100.net (142.250.72.14): icmp_seq=1 time=12.34 ms
64 bytes from lhr25s10-in-f14.1e100.net (142.250.72.14): icmp_seq=2 time=12.29 ms
64 bytes from lhr25s10-in-f14.1e100.net (142.250.72.14): icmp_seq=3 time=12.20 ms

--- statistics ---
3 packets transmitted, 3 received, 0.0% packet loss
```

### Force IPv4 and set interval
```bash
pingx -4 example.com -c 2 -i 0.5
```

### CSV output with timestamps
```bash
pingx example.com -c 2 -D -o csv
```
Sample output:
```
timestamp,host,ip,reverse_dns,size,icmp_seq,time
2025-10-29T20:07:00Z,example.com,93.184.216.34, ,64,1,23.15
2025-10-29T20:07:01Z,example.com,93.184.216.34, ,64,2,22.98
stats,2,2,0.0
```

### JSON output (aggregated)
```bash
pingx 1.1.1.1 -c 2 -o json
```
Sample output (shape):
```json
{
  "host": "1.1.1.1",
  "ip": "1.1.1.1",
  "reverse_dns": null,
  "size": 64,
  "sent": 2,
  "received": 2,
  "loss_percent": 0.0,
  "packets": [
    { "icmp_seq": 1, "time": 9.87 },
    { "icmp_seq": 2, "time": 10.02 }
  ]
}
```

### Custom template
```bash
pingx cloudflare.com -c 2 -o "%timestamp% %host% (%ip%) seq=%icmp_seq% t=%time%ms"
```
Sample output:
```
2025-10-29T20:07:00Z cloudflare.com (104.16.133.229) seq=1 t=10.22ms
2025-10-29T20:07:01Z cloudflare.com (104.16.133.229) seq=2 t=10.09ms
```

### Quiet summary only
```bash
pingx example.com -c 2 -q
```
Sample output:
```
--- statistics ---
2 packets transmitted, 2 received, 0.0% packet loss
```

### Compact header
```bash
pingx example.com -c 1 -m
```
Sample header format:
```
PING example.com (93.184.216.34) 56(84) bytes of data.
```

### Periodic statistics
```bash
pingx example.com -T -e 5
```
- Prints stats every 5 seconds when running continuously.
- With -o csv, the periodic stats line is machine-readable: `stats,<sent>,<received>,<loss%>`

## Known Issues / Limitations
1. JSON mode prints only a final aggregated object (no per-packet lines)
2. Template mode suppresses periodic/final stats lines to avoid mixing formats
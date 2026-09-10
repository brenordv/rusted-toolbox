# Timestamp Converter (ts)

## What it does

The Timestamp Converter (`ts`) is a bidirectional timestamp utility that converts between Unix timestamps and
human-readable datetime formats. It automatically detects the input type and performs the appropriate conversion,
supporting a wide variety of datetime formats commonly used across different systems and locales.

**Key Features:**
- Bidirectional conversion: Unix timestamp to datetime string and back
- Shows current timestamp when no input provided
- Supports multiple datetime formats (ISO 8601, space/slash separated)
- Displays both UTC and local timezone when converting from Unix timestamp
- Automatic format detection and parsing
- Handles both full datetime and date-only inputs (assumes midnight)

## Command-line options

- `input`: optional positional input, no quotes needed for spaces:
  - Unix timestamp (integer; values longer than 10 digits are read as milliseconds)
  - Datetime string in one of the supported formats
  - Empty for the current timestamp

Shared flags from the common CLI: `--app-header` (print the tool header block),
`--log-level <level>` (long form only), `--log-to-console`, `--log-to-file`,
`--rotate-log-file-by-day`.

## Examples

Default output has no header; the header block appears only with `--app-header`.

### Current timestamp
```bash
ts
```
```
Unix timestamp: 1703764800
UTC Time: 2023-12-28T12:00:00Z
Local Time: 2023-12-28T13:00:00+0100
```

### Unix timestamp to datetime
```bash
ts 1703764800
```
```
UTC Time: 2023-12-28T12:00:00Z
Local Time: 2023-12-28T13:00:00+0100
```

### ISO 8601 datetime to Unix timestamp
```bash
ts 2023-12-28T12:00:00
```
```
Unix Timestamp: 1703764800
```

### Date-only input (assumes midnight)
```bash
ts 2023-12-28
```
```
Unix Timestamp: 1703721600
```

### European date format
```bash
ts 28-12-2023 14:30:45
```
```
Unix Timestamp: 1703773845
```

### With the header block
```bash
ts --app-header 1703764800
```
```
ts (2.0.1)
---------------------------------------------------
- Basic Runtime Config
  - Verbose mode: <unused>
  - Log level: Warning
  - Log to stdout: false
  - Log to file: false
  - Rotate log file by day: false
- Tool Runtime Config
  - Input: 1703764800

UTC Time: 2023-12-28T12:00:00Z
Local Time: 2023-12-28T13:00:00+0100
```

### Unparseable input
```bash
ts "12/28/2023 2:30 PM"
```
The tool reports the parse failure on stderr and exits with code 1.

## Known issues

1. **Limited US date format support**: formats like "MM/DD/YYYY h:mm AM/PM" are not supported
   and exit with an error.
2. **Timezone handling**: datetime parsing assumes the local timezone; there is no option to
   parse in a different timezone.
3. **Signed milliseconds heuristic**: the milliseconds branch keys on token length, so a
   sign prefix (`+` or `-`) makes an 11-character seconds value like `-1000000000` read as
   milliseconds.

## Comparison with Unix tools

The `ts` tool provides functionality similar to parts of the Unix `date` command, but with a different focus:

**Similarities:**
- Both can display the current timestamp
- Both can convert between different time representations

**Key differences:**

| Feature            | `ts` Tool                   | Unix `date`                         |
|--------------------|-----------------------------|-------------------------------------|
| Current time       | `ts`                        | `date +%s`                          |
| Unix to datetime   | `ts 1703764800`             | `date -d @1703764800`               |
| Datetime to Unix   | `ts "2023-12-28 12:00:00"`  | `date -d "2023-12-28 12:00:00" +%s` |
| Format flexibility | Multiple auto-detected      | Requires format specification       |
| Timezone display   | Always shows both UTC/local | Single timezone (customizable)      |
| Output format      | Fixed, user-friendly        | Highly customizable                 |

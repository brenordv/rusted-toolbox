# Timestamp Converter (ts)

## What it does

The Timestamp Converter (`ts`) is a bidirectional timestamp utility that converts between Unix timestamps and 
human-readable datetime formats. It automatically detects the input type and performs the appropriate conversion, 
supporting a wide variety of datetime formats commonly used across different systems and locales.

**Key Features:**
- Bidirectional conversion: Unix timestamp ↔ datetime string
- Shows current timestamp when no input provided
- Supports multiple datetime formats (ISO 8601, space/slash separated)
- Displays both UTC and local timezone when converting from Unix timestamp
- Automatic format detection and parsing
- Handles both full datetime and date-only inputs (assumes midnight)

## Command-Line Options
- `input`: Optional input that can be:
  - Unix timestamp (integer)
  - Datetime string in various formats
  - Empty for current timestamp

## Examples

### Current Timestamp
**Command:**
```bash
ts
```

**Output:**
```
🚀 Timestamp Converter v1.0.0
================================================
🔢 Input: (Current time)

Unix timestamp: 1703764800
UTC Time: 2023-12-28T12:00:00Z
Local Time: 2023-12-28T13:00:00+0100
```

### Unix Timestamp to Datetime
**Command:**
```bash
ts 1703764800
```

**Output:**
```
🚀 Timestamp Converter v1.0.0
================================================
🔢 Input: 1703764800

UTC Time: 2023-12-28T12:00:00Z
Local Time: 2023-12-28T13:00:00+0100
```

### ISO 8601 DateTime to Unix Timestamp
**Command:**
```bash
ts 2023-12-28T12:00:00
```

**Output:**
```
🚀 Timestamp Converter v1.0.0
================================================
🔢 Input: 2023-12-28T12:00:00

Unix Timestamp: 1703764800
```

### Date-Only Input (Assumes Midnight)
**Command:**
```bash
ts 2023-12-28
```

**Output:**
```
🚀 Timestamp Converter v1.0.0
================================================
🔢 Input: 2023-12-28

Unix Timestamp: 1703721600
```

### European Date Format
**Command:**
```bash
ts "28-12-2023 14:30:45"
```

**Output:**
```
🚀 Timestamp Converter v1.0.0
================================================
🔢 Input: 28-12-2023 14:30:45

Unix Timestamp: 1703773845
```

### US Date Format with Slashes
**Command:**
```bash
ts "12/28/2023 2:30 PM"
```

**Output:**
```
🚀 Timestamp Converter v1.0.0
================================================
🔢 Input: 12/28/2023 2:30 PM

Invalid date-time format. Unable to parse the input.
```

## Known Issues

1. **Limited US Date Format Support**: Common US formats like "MM/DD/YYYY h:mm AM/PM" are not supported, leading to parsing failures for widely-used datetime representations.

2. **Timezone Handling Inconsistency**: All datetime parsing assumes local timezone, but there's no option to specify a different timezone for input parsing.

## Comparison with Unix Tools

The `ts` tool provides functionality similar to parts of the Unix `date` command, but with a different focus:

### Unix `date` Command Comparison

**Similarities:**
- Both can display the current timestamp
- Both can convert between different time representations

**Key Differences:**

| Feature            | `ts` Tool                   | Unix `date`                         |
|--------------------|-----------------------------|-------------------------------------|
| Current time       | `ts`                        | `date +%s`                          |
| Unix to datetime   | `ts 1703764800`             | `date -d @1703764800`               |
| Datetime to Unix   | `ts "2023-12-28 12:00:00"`  | `date -d "2023-12-28 12:00:00" +%s` |
| Format flexibility | Multiple auto-detected      | Requires format specification       |
| Timezone display   | Always shows both UTC/local | Single timezone (customizable)      |
| Output format      | Fixed, user-friendly        | Highly customizable                 |

# Changelog
## 1.1.0
- Now able to process numeric timestamps with milliseconds alongside the traditional unix timestamp.
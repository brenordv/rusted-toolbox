# GUID Generator Tool

## What this app does

The GUID Generator is a command-line utility that creates UUID version 4 (Universally Unique Identifiers) in various
modes. It supports single GUID generation, empty GUID creation, continuous generation at specified intervals, and
clipboard integration. The tool is designed for developers and system administrators who need reliable GUID generation
in scripts, testing, or development workflows.

## Command Line Examples

### Generate a single GUID
```bash
$ guid
550e8400-e29b-41d4-a716-446655440000
```

### Generate an empty GUID
```bash
$ guid --empty
00000000-0000-0000-0000-000000000000
```

### Generate GUID and copy to clipboard
```bash
$ guid --copy-to-clipboard
550e8400-e29b-41d4-a716-446655440000
```

### Generate GUID silently (no runtime info)
```bash
$ guid --silent
550e8400-e29b-41d4-a716-446655440000
```

### Continuously generate GUIDs every 2 seconds
```bash
$ guid --continuous-generation 2.0
🚦 Press Ctrl+C to stop...
550e8400-e29b-41d4-a716-446655440000
a1b2c3d4-e5f6-7890-abcd-ef1234567890
...
```

### Continuous generation in silent mode
```bash
$ guid --continuous-generation 1.5 --silent
550e8400-e29b-41d4-a716-446655440000
a1b2c3d4-e5f6-7890-abcd-ef1234567890
...
```

## Comparison with Unix Tools

This tool doesn't directly mimic existing Unix utilities, but it's similar to `uuidgen` on many Unix systems:

### Behavioral Comparison with `uuidgen`:

| Feature                   | `guid`                        | `uuidgen`        |
|---------------------------|-------------------------------|------------------|
| **Basic UUID Generation** | ✅                             | ✅                |
| **Empty/Zero UUID**       | ✅ (`--empty`)                 | ❌                |
| **Continuous Generation** | ✅ (`--continuous-generation`) | ❌                |
| **Clipboard Integration** | ✅ (`--copy-to-clipboard`)     | ❌                |
| **Silent Mode**           | ✅ (`--silent`)                | ❌                |
| **Multiple UUIDs**        | ❌                             | ✅ (`-n <count>`) |
| **Different UUID Types**  | ❌ (only v4)                   | ✅ (`-t`, `-r`)   |

### Key Discrepancies:

1. **Multiple UUID Generation**: `uuidgen` supports generating multiple UUIDs with `-n <count>`, while `guid` only supports continuous generation with intervals.

2. **UUID Types**: `uuidgen` supports different UUID versions (v1 time-based, v4 random), while `guid` only generates v4.

This tool serves a different niche with its continuous generation and clipboard features, making it complementary rather than competitive to `uuidgen`. 
# Touch
## What it does

This is a port of the Unix `touch` command to Rust.
The Touch utility updates file access and modification timestamps, creating files if they don't exist. 
It mimics the Unix `touch` command. You can set timestamps to the current time or pass custom timestamps through
various options.

**Key Features:**
- Creates empty files if they don't exist (unless `-c` flag is used)
- Updates access and/or modification timestamps
- Supports multiple time input formats (date strings, formatted timestamps)
- Can copy timestamps from reference files
- Handles symbolic links appropriately
- Processes multiple files in a single command
- Special handling for stdout (`-`) as no-op

## Command-Line Options
- `-a`: Change access time only
- `-c, --no-create`: Don't create files that don't exist  
- `-d, --date <STRING>`: Parse date string and use as timestamp. Accepted forms: POSIX/ISO
  `YYYY-MM-DDThh:mm:SS[.frac][Z]` (`T` or a space; frac takes `.` or `,`; `Z` means UTC, otherwise
  local), the ISO offset form `YYYY-MM-DDThh:mm:ss[.frac]+hh[:]mm` (`2024-01-15T10:30:45+0900`),
  `YYYY-MM-DD [hh:mm[:ss]]`, US month-first `MM/DD/YYYY [hh:mm[:ss]]` (the GNU convention),
  `DD Mon YYYY [hh:mm[:ss]]`, RFC-2822 style with offset (`Mon, 15 Jan 2024 10:30:45 +0000`), and
  `now`. Date-only values resolve to local midnight. GNU relative items (`yesterday`, `2 days ago`)
  are not supported. tool-timestamp parses day-first; the difference is deliberate, touch follows
  the original.
- `-f`: Accepted and ignored (compatibility flag, as in GNU touch)
- `-m`: Change modification time only
- `-n, --no-dereference`: Update symlink timestamps instead of target file
- `-r, --reference <FILE>`: Copy timestamps from reference file
- `-t <TIME>`: Use formatted timestamp `[[CC]YY]MMDDhhmm[.ss]`
- `--time <WORD>`: Specify which time to change (`access`, `atime`, `use`, `modify`, `mtime`)
- `<FILES>`: One or more files to touch

Shared flags (available in every tool of the toolbox):
- `--app-header`: Show the header with tool name, version, and runtime options
- `--log-level <LEVEL>`: Set the log level (long-only flag, case insensitive; defaults to `warn`)
- `--log-to-console`: Log to stdout instead of the default stderr
- `--log-to-file`: Also write logs to a file
- `--rotate-log-file-by-day`: Rotate the log file daily

## Examples
### Basic Usage - Update to Current Time
**Command:**
```bash
touch file1.txt file2.txt
```

**Input:** Two existing or non-existing files
**Output:** Files created if missing, timestamps set to current time. Nothing is printed on success.

### Update Access Time Only
**Command:**
```bash
touch -a existing_file.txt
```

**Input:** File with current modify time: 2024-01-10 08:00:00
**Output:** Access time updated, modification time preserved

### Set Specific Date and Time
**Command:**
```bash
touch -d "2024-12-25 15:30:00" holiday_file.txt
```

**Input:** Non-existing file
**Output:** File created with both timestamps set to 2024-12-25 15:30:00

### Copy Timestamps from Reference File
**Command:**
```bash
touch -r reference.txt target1.txt target2.txt
```

**Input:** 
- reference.txt (access: 2024-01-01 12:00:00, modify: 2024-01-01 11:30:00)
- target files (existing or not)

**Output:** Target files get reference file's timestamps

### Don't Create Missing Files
**Command:**
```bash
touch -c nonexistent.txt existing.txt
```

**Input:** One missing file, one existing file
**Output:** Only existing file timestamps updated; the missing file is not created and causes no error

### Using Formatted Time Specification
**Command:**
```bash
touch -t 202412251530.45 new_year_prep.txt
```

**Input:** Non-existing file
**Output:** File created with both timestamps set to 2024-12-25 15:30:45

## Known Issues

1. **Non-Standard Flag Usage**: Uses `-n` for `--no-dereference` instead of the more common `-h` flag used by standard Unix `touch`. It was intentional to avoid conflict with the `-h` that is automatically added by `clap`. This might create incompatibility with scripts.

2. **Exit Codes**: This tool uses standard 0 (success) and 1 (error), while the Unix implementation has different exit codes for different errors. Existing scripts might not run properly.

## Unix Touch Command Comparison

The current implementation closely follows Unix `touch` behavior with the following discrepancies:

### Compatible Behaviors:
- ✅ Creates files by default, respects `-c` flag
- ✅ Updates both timestamps by default
- ✅ Supports `-a` and `-m` flags for selective updates
- ✅ Handles reference files with `-r`
- ✅ Supports date parsing with `-d`
- ✅ Implements time specification with `-t`
- ✅ Treats `-` as stdout (no-op)
# Touch File Timestamps Utility

## What it does

The Touch utility updates file access and modification timestamps, creating files if they don't exist. 
It mimics the behavior of the Unix `touch` command, allowing users to set timestamps to the current time or specify
custom timestamps through various options.

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
- `-d, --date <STRING>`: Parse date string and use as timestamp
- `-m`: Change modification time only
- `-n, --no-dereference`: Update symlink timestamps instead of target file
- `-r, --reference <FILE>`: Copy timestamps from reference file
- `-t <TIME>`: Use formatted timestamp `[[CC]YY]MMDDhhmm[.ss]`
- `--time <WORD>`: Specify which time to change (`access`, `atime`, `use`, `modify`, `mtime`)
- `<FILES>`: One or more files to touch

## Examples

### Basic Usage - Update to Current Time
**Command:**
```bash
touch file1.txt file2.txt
```

**Input:** Two existing or non-existing files
**Output:** Files created if missing, timestamps set to current time

**Result:**
```
file1.txt - access time: 2024-01-15 10:30:45, modify time: 2024-01-15 10:30:45
file2.txt - access time: 2024-01-15 10:30:45, modify time: 2024-01-15 10:30:45
```

### Update Access Time Only
**Command:**
```bash
touch -a existing_file.txt
```

**Input:** File with current modify time: 2024-01-10 08:00:00
**Output:** Access time updated, modification time preserved

**Result:**
```
existing_file.txt - access time: 2024-01-15 10:30:45, modify time: 2024-01-10 08:00:00
```

### Set Specific Date and Time
**Command:**
```bash
touch -d "2024-12-25 15:30:00" holiday_file.txt
```

**Input:** Non-existing file
**Output:** File created with specified timestamp

**Result:**
```
holiday_file.txt - access time: 2024-12-25 15:30:00, modify time: 2024-12-25 15:30:00
```

### Copy Timestamps from Reference File
**Command:**
```bash
touch -r reference.txt target1.txt target2.txt
```

**Input:** 
- reference.txt (access: 2024-01-01 12:00:00, modify: 2024-01-01 11:30:00)
- target files (existing or not)

**Output:** Target files get reference file's timestamps

**Result:**
```
target1.txt - access time: 2024-01-01 12:00:00, modify time: 2024-01-01 11:30:00
target2.txt - access time: 2024-01-01 12:00:00, modify time: 2024-01-01 11:30:00
```

### Don't Create Missing Files
**Command:**
```bash
touch -c nonexistent.txt existing.txt
```

**Input:** One missing file, one existing file
**Output:** Only existing file timestamps updated, no error for missing file

**Result:**
```
nonexistent.txt - not created, no error
existing.txt - timestamps updated to current time
```

### Using Formatted Time Specification
**Command:**
```bash
touch -t 202412251530.45 new_year_prep.txt
```

**Input:** Non-existing file
**Output:** File created with specified timestamp (2024-12-25 15:30:45)

**Result:**
```
new_year_prep.txt - access time: 2024-12-25 15:30:45, modify time: 2024-12-25 15:30:45
```

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
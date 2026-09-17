# CSV-Split Tool

## Overview

The csv-split tool divides large CSV or text files into smaller files based on line count. It preserves CSV headers
across all output files when CSV mode is enabled, provides real-time progress feedback, and supports graceful shutdown.

The binary was called `split` before 3.0.0; it was renamed because it shadowed the coreutils
tool of that name while doing a different, CSV-focused job.

Notes: 
1. The input must be UTF-8. A byte sequence that is not valid UTF-8 stops the run with an error
   (exit code 1); output written up to that point is left on disk.
2. When splitting files in CSV mode, we are not validating the file or if there's any malformed data.

## Command-line options

| Flag                          | Description                                                                   |
|-------------------------------|-------------------------------------------------------------------------------|
| `-f, --file <FILE>`           | Path to the input file (required).                                            |
| `-o, --output-dir <DIR>`      | Output directory. Defaults to the input file's directory. Created if missing. |
| `-l, --lines-per-file <N>`    | Lines per output file (default 100, minimum 1).                               |
| `-p, --file-prefix <PREFIX>`  | Prefix for output file names (default `split`).                               |
| `-i, --feedback-interval <N>` | Lines between progress updates (default 100, minimum 1).                      |
| `-c, --csv-mode`              | Repeat the first line (header) in every output file.                          |

Shared flags from the common CLI: `--app-header`, `--log-level <level>` (long form only),
`--log-to-console`, `--log-to-file`, `--rotate-log-file-by-day`.

Exit codes: 0 on success, 1 on failure (including non-UTF-8 input and a failed part
flush), 130 when interrupted with
Ctrl+C. On interruption the line already read is written before stopping, so no consumed data is
lost, and the partial output stays on disk.

## Command Line Usage

### Basic Usage
```bash
# Split a file into chunks of 100 lines (default)
csv-split --file large_file.txt

# Split with custom line count and prefix
csv-split --file data.csv --lines-per-file 1000 --file-prefix chunk

# CSV mode with header preservation
csv-split --file data.csv --csv-mode --lines-per-file 500

# Custom output directory
csv-split --file input.txt --output-dir ./output --file-prefix part
```

### Examples with Sample Input/Output

#### Example 1: Basic Text File Splitting
**Input:** `sample.txt` (300 lines)
```
csv-split --file sample.txt --lines-per-file 100
```

**Output:** Creates 3 files:
- `split_sample_1.txt` (lines 1-100)
- `split_sample_2.txt` (lines 101-200)  
- `split_sample_3.txt` (lines 201-300)

#### Example 2: CSV File with Header Preservation
**Input:** `employees.csv`
```csv
id,name,department,salary
1,John Doe,Engineering,75000
2,Jane Smith,Marketing,65000
3,Bob Johnson,Sales,55000
4,Alice Brown,Engineering,80000
```

**Command:**
```bash
csv-split --file employees.csv --csv-mode --lines-per-file 2
```

**Output:** Creates 2 files:

`split_employees_1.csv`:
```csv
id,name,department,salary
1,John Doe,Engineering,75000
2,Jane Smith,Marketing,65000
```

`split_employees_2.csv`:
```csv
id,name,department,salary
3,Bob Johnson,Sales,55000
4,Alice Brown,Engineering,80000
```

## Comparison with Unix `split` Command

The tool covers the same ground as the Unix `split` command but has some differences:

### Similarities
- Splits files into smaller chunks
- Supports custom output naming
- Handles large files efficiently

### Key Differences

| Feature               | Unix `split`               | This Tool                                           |
|-----------------------|----------------------------|-----------------------------------------------------|
| **Default splitting** | By bytes (1000 lines)      | By lines (100 lines)                                |
| **Output naming**     | `xaa`, `xab`, `xac`...     | `prefix_filename_1.txt`, `prefix_filename_2.txt`... |
| **CSV support**       | None                       | Headers preserved in CSV mode                       |
| **Progress feedback** | None                       | Real-time progress display                          |
| **Graceful shutdown** | Basic signal handling      | Preserves partial progress, exits 130               |
| **File extensions**   | Preserves original or none | Uses `.txt` or `.csv` based on mode                 |

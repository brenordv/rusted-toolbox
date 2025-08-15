# Split Tool

## Overview

The Split tool divides large text or CSV files into smaller files based on line count. It preserves CSV headers across
all output files when CSV mode is enabled, provides real-time progress feedback, and supports graceful shutdown.

Notes: 
1. We always assume an UTf-8 file, so other encodings might not be processed correctly.
2. When splitting files in CSV mode, we are not validating the file or if there's any malformed data.

## Command Line Usage

### Basic Usage
```bash
# Split a file into chunks of 100 lines (default)
split --file large_file.txt

# Split with custom line count and prefix
split --file data.csv --lines-per-file 1000 --file-prefix chunk

# CSV mode with header preservation
split --file data.csv --csv-mode --lines-per-file 500

# Custom output directory
split --file input.txt --output-dir ./output --file-prefix part
```

### Examples with Sample Input/Output

#### Example 1: Basic Text File Splitting
**Input:** `sample.txt` (300 lines)
```
split --file sample.txt --lines-per-file 100
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
split --file employees.csv --csv-mode --lines-per-file 2
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

The tool mimics the Unix `split` command but has some differences:

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
| **Graceful shutdown** | Basic signal handling      | Preserves partial progress                          |
| **File extensions**   | Preserves original or none | Uses `.txt` or `.csv` based on mode                 |
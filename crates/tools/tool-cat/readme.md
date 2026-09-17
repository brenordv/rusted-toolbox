# Cat
## What It Is
This is a port of the Unix `cat` command to Rust.
A high-performance implementation of the Unix `cat` command that concatenates files and displays them on standard 
output. Supports all standard Unix cat options including line numbering, character visualization, and formatting 
features.

## What It Does
The cat tool reads files sequentially and writes them to standard output. When no files are specified, it reads from 
standard input. It offers two processing modes:

- **Raw mode**: Direct file-to-stdout copy for maximum performance when no formatting is needed
- **Formatted mode**: Line-by-line processing with various formatting options

## Examples
### Basic file concatenation
```bash
# Display single file
cat file.txt
```
**Input**: `file.txt` contains "Hello\nWorld"  
**Output**: 
```
Hello
World
```

### Line numbering
```bash
# Number all lines
cat -n file.txt
```
**Input**: `file.txt` contains "Hello\n\nWorld"  
**Output**: 
```
     1	Hello
     2	
     3	World
```

### Number only non-blank lines
```bash
# Number non-blank lines only
cat -b file.txt
```
**Input**: `file.txt` contains "Hello\n\nWorld"  
**Output**: 
```
     1	Hello

     2	World
```

### Show tabs and line endings
```bash
# Show tabs as ^I and line endings as $
cat -A file.txt
```
**Input**: `file.txt` contains "Hello\tWorld\nTest\n"  
**Output**: 
```
Hello^IWorld$
Test$
```

### Squeeze blank lines
```bash
# Compress multiple blank lines into one
cat -s file.txt
```
**Input**: Multiple consecutive blank lines  
**Output**: Single blank line between content

### Read from stdin
```bash
# Read from standard input
echo "Hello World" | cat -n
```
**Output**: 
```
     1	Hello World
```

## Command-line options
### Tool flags
| Flag                       | Description                                               |
|----------------------------|-----------------------------------------------------------|
| `-A`, `--show-all`         | Equivalent to `-vET`                                      |
| `-b`, `--number-nonblank`  | Number nonempty output lines; overrides `-n`              |
| `-e`                       | Equivalent to `-vE`                                       |
| `-E`, `--show-ends`        | Display `$` at the end of each line                       |
| `-n`, `--number`           | Number all output lines                                   |
| `-s`, `--squeeze-blank`    | Suppress repeated empty output lines                      |
| `-t`                       | Equivalent to `-vT`                                       |
| `-T`, `--show-tabs`        | Display TAB characters as `^I`                            |
| `-u`                       | Accepted for compatibility with POSIX `cat`; does nothing |
| `-v`, `--show-nonprinting` | Use `^` and `M-` notation, except for LFD and TAB         |

### Shared flags
These flags are common to every tool in this workspace and have no short form:

| Flag                       | Description                                                                                                                                                                                                                                   |
|----------------------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `--app-header`             | Print the header with tool name, version, and runtime options before the output                                                                                                                                                               |
| `--log-level <LEVEL>`      | Set the log level: `trace`, `debug`, `info`, `warn`, `warning`, `error`, or `disabled` (case-insensitive). Defaults to `warn`. `disabled` silences everything and overrides `RUST_LOG`; every other level yields to `RUST_LOG` when it is set |
| `--log-to-console`         | Send log output to stdout instead of the default stderr                                                                                                                                                                                       |
| `--log-to-file`            | Also write log output to a file                                                                                                                                                                                                               |
| `--rotate-log-file-by-day` | Rotate the log file daily                                                                                                                                                                                                                     |
| `--verbose`                | Accepted, but cat does not use it                                                                                                                                                                                                             |

## Notes and caveats
- A missing or unreadable file is reported on stderr, and processing continues with the remaining files; the exit code is non-zero when any input failed.
- When the reading end of a pipe closes early (`cat big.txt | head`), the tool stops quietly and exits 0. GNU `cat` exits 141 from SIGPIPE instead; re-raising the signal is not portable to Windows.
- `--log-to-console` sends log output to stdout, the same channel as the file data, so log lines interleave with the output unpredictably.
- `--log-level disabled` silences all diagnostics, so a failed file prints nothing and the exit code is the only sign of failure.
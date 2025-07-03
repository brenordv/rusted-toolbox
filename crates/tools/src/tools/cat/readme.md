# Cat Tool

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
**Input**: `file.txt` contains "Hello\tWorld\nTest"  
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

## Known Issues

- Not sure if this is an actual issue, but this app uses 128Kb buffer instead of the original 8kb for the GNU `cat`.
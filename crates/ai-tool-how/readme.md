# How - CLI Command Helper

A smart CLI tool that helps users with command-line syntax by fixing broken commands and suggesting commands from 
natural language requests.

Are there other options to do this using AI? Yes, like GitHub/Copilot that you can use with the `gh` tool.
I also know about `ShellGPT`, `Amazon Q Developer`, and even some other famous tools like `thefuck`, `navi`,
`explainshell`, `cheat.sh`, etc.

This one is fully dependent on AI, but you can use OpenAI, OpenRouter or even your local OpenWebUI instance. Another
difference from the other AI tools is that I don't collect any data from the user whatsoever, so it is way more private.

Granted that my tool is most likely not the most efficient one, but it works, and it is as private as it can be 
(considering the reliance on AI).


## Features

- **Command Fixing**: Automatically corrects common syntax errors in CLI commands
- **Natural Language Suggestions**: Converts plain English requests into proper CLI commands
- **OS-Aware**: Provides OS-specific commands for Windows, Linux, and macOS
- **Shell Detection**: Automatically detects your current shell for better accuracy
- **Clipboard Integration**: Optionally copy results directly to the clipboard

## Installation
Build from source using the build scripts in the repository root.

## Usage
### Fix a Broken Command

```bash
# Fix common syntax mistakes
how find . -name -i "*bacon*"
# Output: find . -iname "*bacon*"

how ls -la --color
# Output (on macOS): ls -la -G

how grep -r pattern .
# Output (on Windows): findstr /S "pattern" *
```

### Suggest Commands from Natural Language
```bash
# Get command suggestions
how --ask "How to find files with bacon in the name"
# Output: find . -iname "*bacon*"

how -a "show disk usage"
# Output (Linux): df -h
# Output (Windows): dir /s

how -a "kill process by name"
# Output (Linux): pkill processname
# Output (Windows): taskkill /IM processname.exe /F
```

### Copy to Clipboard
```bash
# Fix command and copy to clipboard
how --copy find . -name -i "*log*"

# Suggest command and copy to clipboard
how -a -c "compress this folder"
```

## Command Line Options
- `<command>` - Command to fix (default mode)
- `--ask, -a <REQUEST>` - Natural language request for command suggestion
- `--copy, -c` - Copy the result to clipboard
- `--help, -h` - Show help information
- `--version, -V` - Show version information

## Examples
### Command Fixing Examples
```bash
how find . -name -i "*log*"        # → find . -iname "*log*"
how ps aux grep chrome             # → ps aux | grep chrome
how tar -xvf file.tar              # → tar -xvf file.tar
how rm -r folder                   # → rmdir /s folder (Windows)
how cat file.txt                   # → type file.txt (Windows)
```

### Natural Language Examples
```bash
how -a "show running processes"     # → ps aux (Linux) / tasklist (Windows)
how -a "download file from url"     # → curl -O https://example.com/file.txt
how -a "search for text in files"   # → grep -r "text" . (Linux) / findstr /S "text" * (Windows)
how -a "create symbolic link"       # → ln -s target link (Linux) / mklink link target (Windows)
how -a "install package"            # → apt install pkg (Linux) / brew install pkg (macOS) / winget install pkg (Windows)
```

## Environment Variables
The tool uses AI functionality, so ensure your AI service is properly configured via environment variables as required by the `ai-shared` crate.

## Error Handling
- **Invalid AI responses**: Clear error message with suggestion to try again or report the issue
- **Network issues**: Graceful failure with helpful error message  
- **Unknown OS**: Warning message with fallback to Linux commands
- **Empty input**: Shows usage help (same as --help)
- **AI function failures**: Clear error message with suggestion to try again or report the issue

## OS Support
- **Linux**: Full support for common bash/shell commands
- **macOS**: Full support with macOS-specific variations (e.g., `ls -G` instead of `ls --color`)
- **Windows**: Support for both Command Prompt and PowerShell commands

## Shell Detection
The tool automatically detects your shell environment:
- Bash, Zsh, Fish on Unix-like systems
- PowerShell and Command Prompt on Windows
- Falls back to OS defaults if shell cannot be detected
#![allow(dead_code, unused_doc_comments)]
use ai_macros::ai_function;

const OUTPUT: &str = "";

#[ai_function]
pub fn fix_cli_command(_command: &str, _os: &str) -> &'static str {
    /// ROLE
    /// - Repair one possibly broken CLI command so it runs on the specified OS/shell.
    /// - Output EXACTLY ONE corrected command string.
    ///
    /// INPUTS
    /// - `command`: a single command line (may include stray words or code fences; extract the command).
    /// - `os`: "windows (shell: powershell|cmd)" | "linux (shell: bash)" | "macos (shell: zsh)" | "unknown (value)".
    ///
    /// OS/SHELL RESOLUTION
    /// - If `unknown (...)`, infer from the text in parentheses:
    ///   - mentions "win" or "powershell" -> windows (powershell)
    ///   - mentions "cmd" -> windows (cmd)
    ///   - mentions "wsl" or "linux" -> linux (bash)
    ///   - mentions "darwin" or "mac" -> macos (zsh)
    ///   - mentions "git-bash"/"msys"/"mingw"/"cygwin" -> treat as POSIX (bash) unless paths are clearly Windows
    /// - If shell unspecified: windows→powershell; linux/macos→bash/zsh (POSIX).
    ///
    /// REPAIR STRATEGY (FOLLOW IN ORDER)
    /// 1) Normalize: strip code fences/backticks, collapse extra whitespace.
    /// 2) Classify: identify the task family by trigger tokens/patterns (see REPAIR RULES below).
    /// 3) Snap-to-canonical: choose the canonical form for the resolved OS/shell and **fill the user’s concrete arguments** (patterns, paths, URLs, process names).
    /// 4) Fix mechanics: correct flags/options, add missing pipes (`|`), add missing `-exec` terminator `\;`, and normalize quoting/escaping for the target shell.
    /// 5) Minimal-change rule: preserve the original intent and argument order; make the smallest edits required to reach a working canonical command.
    /// 6) Confidence gate: if you cannot confidently classify/repair, **return the original command unchanged**.
    ///
    /// QUOTING/ESCAPING
    /// - POSIX (linux/macos): prefer single quotes for literals; escape `find -exec` terminator as `\;`.
    /// - PowerShell: single quotes are literal; double quotes expand. For embedded single quotes, use `''`.
    /// - cmd.exe: double-quote paths/patterns; escape meta-characters with `^` only when necessary.
    ///
    /// SAFETY
    /// - Do NOT add `sudo` or destructive flags unless already present or clearly implied by the input.
    /// - Prefer safer repairs for deletion (`rm -ri` on POSIX; omit `/Q` on Windows unless clearly intended).
    ///
    /// REPAIR RULES (TRIGGERS ▶ SNAP-TO-CANONICAL)
    /// - Find files by name (case-insensitive):
    ///   Triggers: tokens like `find`+`-name`±`-i`, or obvious filename-glob search intent.
    ///   POSIX ▶ `find . -iname "<pattern>"`
    ///   cmd   ▶ `dir /s /b <pattern>`
    ///   pwsh  ▶ `Get-ChildItem -Recurse -Filter <pattern>`
    ///
    /// - Recursive text search (grep-like):
    ///   Triggers: `grep` ± `-r`/`-R`, `ripgrep`/`ag` (convert to built-ins), or `findstr`/`Select-String` hints.
    ///   POSIX ▶ `grep -r "<pattern>" .`
    ///   cmd   ▶ `findstr /S /I "<pattern>" *`
    ///   pwsh  ▶ `Select-String -Path * -Pattern '<pattern>' -List`
    ///
    /// - Locate executable (which/where):
    ///   Triggers: `which`, `command -v`, `where`.
    ///   POSIX ▶ `command -v <tool>`
    ///   windows ▶ `where <tool>`
    ///
    /// - List processes / grep process:
    ///   Triggers: `ps`, `tasklist`, `Get-Process`, or broken `ps aux grep X`.
    ///   POSIX ▶ `ps aux | grep <name>`
    ///   cmd   ▶ `tasklist`
    ///   pwsh  ▶ `Get-Process`
    ///
    /// - Kill by name:
    ///   Triggers: `kill`, `pkill`, `taskkill`, `Stop-Process`.
    ///   POSIX ▶ `pkill <name>`
    ///   cmd   ▶ `taskkill /IM <name>.exe /F`
    ///   pwsh  ▶ `Stop-Process -Name <name> -Force`
    ///
    /// - Show network connections / filter by port:
    ///   Triggers: `netstat`, `ss`, `Get-NetTCPConnection`, or `| grep :<port>`/`findstr :<port>`.
    ///   POSIX ▶ `netstat -tuln` (or keep user’s concrete port filter with `| grep :<port>`)
    ///   cmd   ▶ `netstat -an` (keep `| findstr :<port>` if present)
    ///   pwsh  ▶ `Get-NetTCPConnection` (add `-LocalPort <port>` if the input indicates a port)
    ///
    /// - Disk usage / free space:
    ///   Triggers: `df`, `wmic logicaldisk`, `Get-Volume`, `Get-PSDrive`.
    ///   POSIX ▶ `df -h`
    ///   cmd   ▶ `wmic logicaldisk get size,freespace,caption`
    ///   pwsh  ▶ `Get-Volume`
    ///
    /// - Print/show file contents:
    ///   Triggers: `cat`, `type`, `Get-Content`.
    ///   POSIX ▶ `cat <file>`
    ///   cmd   ▶ `type <file>`
    ///   pwsh  ▶ `Get-Content <file>`
    ///
    /// - Download via HTTP or HTTPS:
    ///   Triggers: `curl`, `wget`, `Invoke-WebRequest`.
    ///   POSIX ▶ `curl -L -o <outfile> <url>`
    ///   cmd/pwsh ▶ `curl.exe -L -o <outfile> <url>` OR `Invoke-WebRequest -Uri <url> -OutFile <outfile>`
    ///
    /// EDGE-CASE FIXES (EXPLICIT)
    /// - `ps aux grep X` → insert missing pipe: `ps aux | grep X`
    /// - `find . -name -i "*x*"` → combine: `find . -iname "*x*"`
    /// - `find -exec ... {}` → ensure terminator: `find ... -exec ... {} \;`
    /// - macOS `ls --color` → BSD coloring: `ls -G` (preserve other flags)
    /// - Windows `df -h` → PowerShell: `Get-Volume` ; cmd: `wmic logicaldisk get size,freespace,caption`
    /// - Commands with no good Windows equivalent (e.g., `chmod`) → **return the original unchanged**
    ///
    /// OUTPUT CONTRACT (STRICT)
    /// - Output MUST be exactly one command line. No code fences, comments, or extra whitespace.
    /// - If unsure, return the original command unchanged.
    OUTPUT
}

#[ai_function]
pub fn suggest_cli_command(_request: &str, _os: &str) -> &'static str {
    /// ROLE
    /// - Generate OS/shell-appropriate command(s) that accomplish the natural-language request.
    /// - Do all reasoning internally; OUTPUT ONLY commands.
    ///
    /// INPUTS
    /// - `request`: a single natural-language task description (may be brief/vague).
    /// - `os`: "windows (shell: powershell|cmd)" | "linux (shell: bash)" | "macos (shell: zsh)" | "unknown (value)".
    ///
    /// OS/SHELL RESOLUTION
    /// - If `os` is "unknown (...)", infer:
    ///   - contains "win" or "powershell" -> windows (powershell)
    ///   - contains "cmd" -> windows (cmd)
    ///   - contains "wsl" or "linux" -> linux (bash)
    ///   - contains "darwin" or "mac" -> macos (zsh)
    ///   - contains "git-bash"/"msys"/"mingw"/"cygwin" -> treat as POSIX (bash) unless paths are clearly Windows
    /// - If shell unspecified: windows→powershell; linux/macos→bash/zsh (POSIX).
    ///
    /// GENERATION STRATEGY (FOLLOW IN ORDER)
    /// 1) Extract concrete tokens from the request (paths, filenames, patterns, URLs, ports, process names).
    /// 2) Classify the task into a family (see TASK FAMILIES below).
    /// 3) Snap to the canonical command for the resolved OS/shell; **fill slots with the user’s concrete tokens**.
    /// 4) Compose 1–3 commands total:
    ///    - Single-step tasks: one command.
    ///    - Multi-step tasks: up to 3 lines, ordered, idempotent where possible.
    ///    - Prefer `&&` within a line only when the steps are inseparable; otherwise separate lines.
    /// 5) Quote/escape for the target shell (see QUOTING).
    /// 6) Safety gates: avoid destructive flags unless explicitly requested.
    ///
    /// OUTPUT CONTRACT (STRICT)
    /// - Return command(s) ONLY—no prose, no code fences, no leading/trailing whitespace.
    /// - Use literal newlines between commands (max 3 total).
    /// - Use concrete placeholders **only** if the request is generic, with obvious tokens: `<pattern>`, `<name>`, `<file>`, `<dir>`, `<url>`, `<outfile>`, `<port>`, `<archive>`.
    ///
    /// QUOTING
    /// - POSIX (linux/macos): single quotes for literals; escape `find -exec` terminator as `\;`.
    /// - PowerShell: single quotes are literal; double quotes expand. For embedded single quotes use `''`.
    /// - cmd.exe: double-quote paths/patterns; escape meta chars with `^` only if necessary.
    ///
    /// SAFETY
    /// - Prefer non-destructive defaults (e.g., list/preview before delete; omit `/Q` on Windows, `-f` on POSIX, unless requested).
    /// - Don’t add `sudo` unless the task clearly requires elevation (e.g., package installation).
    ///
    /// TASK FAMILIES (TRIGGERS ▶ CANONICAL COMMANDS)
    /// - Find files by name (case-insensitive)
    ///   POSIX ▶ `find . -iname "*<pattern>*"`
    ///   cmd   ▶ `dir /s /b *<pattern>*`
    ///   pwsh  ▶ `Get-ChildItem -Recurse -Filter *<pattern>*`
    ///
    /// - Search text recursively
    ///   POSIX ▶ `grep -r "<pattern>" .`
    ///   cmd   ▶ `findstr /S /I "<pattern>" *`
    ///   pwsh  ▶ `Select-String -Path * -Pattern '<pattern>' -List`
    ///
    /// - Show disk usage / free space
    ///   POSIX ▶ `df -h`
    ///   cmd   ▶ `wmic logicaldisk get size,freespace,caption`
    ///   pwsh  ▶ `Get-Volume`
    ///
    /// - List processes / kill by name
    ///   POSIX ▶ `ps aux`      | kill ▶ `pkill <name>`
    ///   cmd   ▶ `tasklist`    | kill ▶ `taskkill /IM <name>.exe /F`
    ///   pwsh  ▶ `Get-Process` | kill ▶ `Stop-Process -Name <name> -Force`
    ///
    /// - Network connections / filter by port
    ///   POSIX ▶ `netstat -tuln` (append `| grep :<port>` if a port is specified)
    ///   cmd   ▶ `netstat -an`   (append `| findstr :<port>` if a port is specified)
    ///   pwsh  ▶ `Get-NetTCPConnection` (add `-LocalPort <port>` when a port is specified)
    ///
    /// - Directory size summary
    ///   POSIX ▶ `du -sh <dir>`
    ///   cmd   ▶ `for /f "usebackq" %A in (\`dir /s /-c "<dir>" ^| find "File(s)"\`) do @echo %A`   // fallback is awkward; prefer PowerShell
    ///   pwsh  ▶ `(Get-ChildItem -Recurse -File "<dir>" | Measure-Object -Sum Length).Sum`
    ///
    /// - Quick HTTP server (current directory)
    ///   POSIX ▶ `python3 -m http.server 8000`
    ///   cmd/pwsh ▶ `py -m http.server 8000`
    ///
    /// - Download file
    ///   POSIX ▶ `curl -L -O <url>`
    ///   cmd/pwsh ▶ `curl.exe -L -O <url>`  OR  `Invoke-WebRequest -Uri <url> -OutFile <outfile>`
    ///
    /// - Compress / extract
    ///   POSIX ▶ compress dir ▶ `tar -czf <archive>.tar.gz <dir>`
    ///           extract tar.gz ▶ `tar -xzf <archive>.tar.gz`
    ///   pwsh  ▶ compress dir ▶ `Compress-Archive -Path <dir> -DestinationPath <dir>.zip`
    ///           extract zip  ▶ `Expand-Archive -Path <archive>.zip -DestinationPath <dir>`
    ///
    /// - Show file metadata / permissions
    ///   POSIX ▶ `ls -la`
    ///   cmd   ▶ `dir /Q`
    ///   pwsh  ▶ `Get-ChildItem | Format-List Mode,Name,Length,LastWriteTime`
    ///
    /// - Create a symbolic link
    ///   POSIX ▶ `ln -s <target_file> <link_name>`
    ///   cmd   ▶ `mklink <link_name> <target_file>` (use `/D` for directories)
    ///
    /// - Install a package
    ///   linux ▶ `sudo apt install <package_name>`   // default to Debian/Ubuntu; switch if request explicitly mentions a distro/PM
    ///   macos ▶ `brew install <package_name>`
    ///   windows ▶ `winget install <package_name>`
    ///
    /// DISAMBIGUATION HEURISTICS
    /// - If the request is underspecified, choose the most common, non-destructive interpretation for that OS/shell.
    /// - Do not output alternatives (which would all run if pasted). Pick one canonical solution per task family.
    ///
    /// OUTPUT EXAMPLES (BEHAVIORAL, NOT LITERAL)
    /// - “find files with bacon in the name” on linux → `find . -iname "*bacon*"`
    /// - “show disk usage” on windows (powershell) → `Get-Volume`
    /// - “kill process by name chrome” on macos → `pkill chrome`
    /// - “search for text in files” on windows (cmd) → `findstr /S /I "search_text" *`
    /// - “start a local web server” on linux → `python3 -m http.server 8000`
    ///
    /// REMINDER
    /// - Return commands only. No explanations, no extra whitespace.
    OUTPUT
}

# Gitignore Tool
An intelligent CLI tool that automatically creates or updates `.gitignore` files based on the file types detected in 
your project. 
It scans your project directory and fetches appropriate gitignore rules from authoritative sources like GitHub's 
gitignore repository.

## What It Does
The gitignore tool analyzes your project structure, identifies file extensions and project types, then automatically 
downloads and merges the most relevant `.gitignore` patterns from trusted sources. It intelligently combines multiple
gitignore templates when your project uses multiple technologies.

Key features:
- **Automatic Detection**: Scans your project to identify programming languages and frameworks
- **Smart Merging**: Combines existing `.gitignore` content with new rules without duplication
- **Multiple Sources**: Fetches gitignore patterns from GitHub's official gitignore repository and other authoritative sources
- **Broad Coverage**: Supports 25+ programming languages and development environments
- **Conflict Resolution**: Sanitizes and deduplicates gitignore entries

## Merge behavior
When a `.gitignore` already exists, its lines keep their original order and come first in the
merged file; downloaded template lines are appended after them, in download order. Comment lines
and blank lines are dropped, duplicates collapse to their first occurrence, and the output is not
alphabetically sorted, so `!` re-include lines stay after the patterns they negate. The dedupe also
applies to lines duplicated within the existing file: a pattern repeated after its own negation
(pattern, `!` negation, the pattern again) loses the repeat, so the negation stays effective.

## Supported Languages & Frameworks
The tool automatically detects and generates gitignore rules for:

**Programming Languages**: Python, Java, JavaScript, TypeScript, Go, PHP, Ruby, Swift, Dart, Scala, C++, Kotlin, Rust, C#, Objective-C, Perl, Elixir, Haskell, R, Julia, MATLAB, TeX
**Frameworks & Tools**: Node.js, React, Unity, .NET, Godot, Next.js, Hugo, Unreal Engine
**Development Environments**: Visual Studio Code, Visual Studio, JetBrains IDEs, Emacs, Vim
**AI Agents**: footprints of Claude Code, Cursor, Windsurf, Gemini, Aider, Continue, Cline, Codex, and Codeium queue a shared agents artifacts template (see below)
**Operating Systems**: macOS, Windows

## AI agent artifacts
When the target folder carries an AI agent footprint (a state dir such as `.claude`, `.cursor`,
`.windsurf`, `.gemini`, `.continue`, `.cline`, `.codex`, or `.codeium`, or files such as
`.cursorrules`, `CLAUDE.local.md`, the Aider history files, or the Gemini CLI debug log and
clipboard dir), the
tool queues the upstream
[`Global/Agents.gitignore`](https://github.com/github/gitignore/blob/main/Global/Agents.gitignore)
template so local agent state stays out of git. Files inside the agent state dirs are excluded
from language detection, the same way `.vscode` and `.idea` contents are.

A footprint only exists after an agent has run, so the `--ai` flag queues the same template
unconditionally, for fresh checkouts:

```bash
gitignore --ai
```

One caveat on coverage: the upstream template's active entries cover local state for Aider
(chat/input history), Claude Code (`.claude/*.local.json`, `.claude/**/*.log`, `CLAUDE.local.md`),
and Gemini CLI (debug log, clipboard dir). The other agents it lists appear only as commented-out
examples, and comment lines are dropped during the merge, so they contribute nothing today. The
template URL will be swapped for a fork that opts in more agents.

## Examples
### Basic Usage - Analyze Current Directory
```bash
gitignore
```
**What it does**: Scans the current working directory for file types and creates/updates `.gitignore`

**Example Output** (progress messages go to stderr at `--log-level info`):
```
Figuring out which .gitignore files to download...
New .gitignore data queued for download: .rs, .js, .ts
Fetching new .gitignore data...
Successfully fetched 45 lines of gitignore data from https://raw.githubusercontent.com/github/gitignore/main/Rust.gitignore
Successfully fetched 32 lines of gitignore data from https://raw.githubusercontent.com/github/gitignore/main/Node.gitignore
Successfully fetched 28 lines of gitignore data from https://raw.githubusercontent.com/microsoft/TypeScript/main/.gitignore
Fetched 105 lines of data for the .gitignore file...
The .gitignore already exists. Merging with the new data...
Writing 98 lines to .gitignore...
All done!
```

### Showing the header
The header block only prints when the `--app-header` flag is passed:
```bash
gitignore --app-header
```
**Example Output**:
```
gitignore (2.1.0)
---------------------------------------------------
- Basic Runtime Config
  - Verbose mode: <unused>
  - Log level: Warning
  - Log to stdout: false
  - Log to file: false
  - Rotate log file by day: false
- Tool Runtime Config
  - Target folder: /home/user/my-project
  - Include AI artifacts: false
```

### Analyze Specific Directory
```bash
gitignore /path/to/project
```
**What it does**: Analyzes the specified directory instead of the current working directory

### Project with Multiple Languages
For a project containing:
```
my-app/
├── src/
│   ├── main.rs          # Rust
│   ├── server.js        # Node.js
│   └── app.tsx          # React/TypeScript
├── package.json
└── Cargo.toml
```

**Output**: Downloads and merges gitignore rules for Rust, Node.js, TypeScript, and React

### Already Has .gitignore
If your project already has a `.gitignore` file:
- **Preserves existing rules**: Your custom gitignore entries are kept, in their original order, at the top of the file
- **Adds new rules**: Only adds rules for newly detected file types, appended after the existing ones
- **Removes duplicates**: Duplicate entries are removed (first occurrence wins)
- **Keeps negations working**: The output is not sorted, so `!` re-include lines stay after the patterns they negate

### No Matching File Types
```bash
gitignore
```
**Output** (at `--log-level info`):
```
Figuring out which .gitignore files to download...
No new .gitignore data to download. Guess I won't touch the .gitignore...
```

## Smart Filtering

The tool automatically excludes:
- **Git directories**: `.git/` and its contents
- **Common build folders**: `node_modules/`, `target/`, `dist/`, `build/`
- **Virtual environments**: `venv/`, `env/`
- **IDE folders**: `.idea/`, `.vs/`, `.vscode/`

## Data Sources

Gitignore patterns are fetched from authoritative sources:
- **GitHub's gitignore repository**: Official templates for most languages, plus the
  `Global/Agents.gitignore` template for AI agent artifacts
- **Project repositories**: Direct from language/framework maintainers (e.g., TypeScript from Microsoft)
- **Custom sources**: Curated templates for specific use cases

## Use Cases
- **New Projects**: Quickly set up gitignore rules
- **Multi-language Projects**: Automatically handle complex project structures
- **Legacy Projects**: Add missing gitignore rules to existing codebases
- **Team Standardization**: Ensure consistent gitignore patterns across team projects
- **CI/CD Integration**: Automatically maintain gitignore files in automated workflows

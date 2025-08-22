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
- **Comprehensive Coverage**: Supports 25+ programming languages and development environments
- **Conflict Resolution**: Sanitizes and deduplicates gitignore entries

## Supported Languages & Frameworks
The tool automatically detects and generates gitignore rules for:

**Programming Languages**: Python, Java, JavaScript, TypeScript, Go, PHP, Ruby, Swift, Dart, Scala, C++, Kotlin, Rust, C#, Objective-C, Perl, Elixir, Haskell, R, Julia, MATLAB, TeX
**Frameworks & Tools**: Node.js, React, Unity, .NET, Godot, Next.js, Hugo, Unreal Engine
**Development Environments**: Visual Studio Code, Visual Studio, JetBrains IDEs, Emacs, Vim, Cursor AI
**Operating Systems**: macOS, Windows

## Examples
### Basic Usage - Analyze Current Directory
```bash
gitignore
```
**What it does**: Scans the current working directory for file types and creates/updates `.gitignore`

**Example Output**:
```
📋 Gitignore v1.0.0
---------------------------
Target folder: /home/user/my-project

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
- **Preserves existing rules**: Your custom gitignore entries are kept
- **Adds new rules**: Only adds rules for newly detected file types
- **Removes duplicates**: Ensures no duplicate entries in the final file
- **Sorts output**: Creates a clean, organized gitignore file

### No Matching File Types
```bash
gitignore
```
**Output**:
```
📋 Gitignore v1.0.0
---------------------------
Target folder: /home/user/text-files

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
- **GitHub's gitignore repository**: Official templates for most languages
- **Project repositories**: Direct from language/framework maintainers (e.g., TypeScript from Microsoft)
- **Community repositories**: Specialized templates for tools like Cursor AI
- **Custom sources**: Curated templates for specific use cases

## Use Cases
- **New Projects**: Quickly set up comprehensive gitignore rules
- **Multi-language Projects**: Automatically handle complex project structures
- **Legacy Projects**: Add missing gitignore rules to existing codebases
- **Team Standardization**: Ensure consistent gitignore patterns across team projects
- **CI/CD Integration**: Automatically maintain gitignore files in automated workflows
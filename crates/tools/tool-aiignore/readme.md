# AiIgnore Tool

A CLI tool that automatically creates or updates common AI-ignore files in your project directory. These files help 
control which files and directories are excluded from AI assistant indexing and processing.

This tool is similar to [gitignore](../tool-gitignore/readme.md), but simpler and focused on keeping AI out of things
it shouldn't see.

I think it works well enough for now, but this will definitely need to evolve, since every day, a brand-new shiny AI 
tool appears in the wild. 

## What It Does
The AiIgnore tool manages multiple AI-ignore files that are used by various AI development tools and assistants. It 
fetches ignore patterns from authoritative sources and creates/updates all supported AI-ignore files in your project.

Key features:
- **Multiple AI Tools Support**: Creates ignore files for Cursor, Windsurf, Codeium, and other AI assistants
- **Template Fetching**: Downloads ignore patterns from curated online templates
- **Smart Merging**: Combines existing ignore rules with new templates without duplication
- **Automatic Sanitization**: Removes comments, empty lines, and duplicates for clean output
- **Unified Management**: Updates all AI-ignore files at once with the same ruleset

## Supported AI-Ignore Files

The tool creates and maintains the following AI-ignore files:
- `.aiignore` - Generic AI ignore file
- `.cursorignore` - Cursor AI IDE
- `.cursorindexingignore` - Cursor AI indexing exclusions
- `.windsurfignore` - Windsurf AI assistant
- `.codeiumignore` - Codeium AI assistant
- `.windsurfrules` - Windsurf specific rules
- `.claudeignore` - Claude AI assistant (proposed standard)
- `.aiexclude` - Gemini/Bard AI assistant

## Examples
### Basic Usage - Current Directory
```bash
aiignore
```
**What it does**: Analyzes the current working directory and creates/updates all AI-ignore files

**Example Output**:
```
📋 AiIgnore v1.0.0
---------------------------
Target folder: /home/user/my-project

Checking for existing AI ignore files...
Found existing file: .cursorignore
Downloading AI ignore templates...
Successfully fetched 45 lines of AI ignore data from https://raw.githubusercontent.com/brenordv/gitignore-files/refs/heads/master/get-out-of-my-land.ai
Added 32 new lines of AI ignore data...
Writing 67 lines to AI ignore files...
Creating file: .aiignore
Updating file: .cursorignore
Creating file: .cursorindexingignore
Creating file: .windsurfignore
Creating file: .codeiumignore
Creating file: .windsurfrules
Creating file: .claudeignore
Creating file: .aiexclude
All done!
```

### Analyze Specific Directory
```bash
aiignore /path/to/project
```
**What it does**: Creates/updates AI-ignore files in the specified directory instead of the current working directory

### Project with Existing AI-Ignore Files
If your project already has AI-ignore files:
- **Preserves existing rules**: Your custom ignore entries are kept
- **Adds new rules**: Only adds new patterns from templates
- **Removes duplicates**: Ensures no duplicate entries in the final files
- **Sorts output**: Creates clean, organized ignore files

### No New Data Available
```bash
aiignore
```
**Output when no new templates are found**:
```
📋 AiIgnore v1.0.0
---------------------------
Target folder: /home/user/my-project

Checking for existing AI ignore files...
Found existing file: .aiignore
Downloading AI ignore templates...
No new lines of AI ignore data found in https://raw.githubusercontent.com/brenordv/gitignore-files/refs/heads/master/get-out-of-my-land.ai
No new AI ignore data found...
```
# Contributing
## Philosophy
Each tool follows a few principles I try to stick to:
- **Do one thing well**: Each tool has a focused purpose
- **Reasonable defaults**: Should work out of the box for common cases
- **Graceful handling**: Proper error messages and cleanup
- **Performance matters**: Use async I/O and parallel processing where it makes sense

## Project structure
1. **Workspace crates**
   - **Individual tool crates**: Each CLI tool has its own dedicated crate (e.g., `crates/tool-cat`, `crates/tool-jwt`, `crates/tool-split`, etc.)
     - Each tool crate contains:
       - `main.rs` (thin entrypoint that orchestrates the tool logic);
       - `cli_utils.rs` (argument parsing/validation only);
       - `models.rs` (structs and data models);
       - `<tool_name>_app.rs` (the actual tool logic);
       - `readme.md` (manual for the tool);
       - `Cargo.toml` (tool-specific dependencies and metadata);
       - Additional files as needed to keep things tidy, scoped to the tool;
   - `crates/ai-tools`: AI-powered agents and helpers (e.g., the chatbot) [I'll probably refactor this later]
     - Binary definitions in `Cargo.toml` using `[[bin]]` sections;
     - Each agent lives under `crates/ai-tools/src/agents/<agent_name>` with:
       - `cli_utils.rs` (runtime info, argument/env validation, but no agent logic);
       - `models.rs` (agent-specific types);
       - `<agent_name>_app.rs` (the agent logic);
       - Optional `readme.md` for agent-specific docs;
     - Supporting modules: `ai_functions`, `message_builders`, `models`, `requesters`, `tasks`, `utils`;
   - `crates/shared`: Cross-cutting utilities and modules shared by multiple binaries
     - Organized by functionality: `command_line`, `constants`, `eventhub`, `logging`, `sqlite`, `system`, `utils`;
     - If more than one tool/agent needs it, it belongs here rather than duplicating logic;
   - `crates/macros`: Procedural macros used across the workspace.
2. **Root `readme.md` file**: Add a reference to any new tools/agents to this file, or update/review any behaviors here;
3. **Build scripts (`build.sh`, and `build.bat`)**: The build command for every tool and agent binary should be covered for all platforms (Windows, Linux, and MacOs). Each individual tool crate is built separately;
4. **Installing on Non-Windows systems**: For non-Windows systems, where tools like `cat` and `touch` already exist, we build them but may skip copying to `dist`;
5. **Testing**: Try to add tests to every tool/agent and shared utility when it makes sense;
6. **Adding new tools**: To add a new CLI tool, create a new crate under `crates/tool-<name>` and add it to the workspace members in the root `Cargo.toml`;

## Tool/Agent structure
1. Tools and agents can print/log information, trace, and warnings, but not errors;
2. If any errors that need to stop the execution happen, return the error to the entrypoint and use `context` (from Anyhow) to add info on what went wrong;
3. If the execution is successful, use `exit_success()` function;
4. If the execution fails, use `exit_error()` function to exit the app (from the entrypoint file only). Right now, I'm not using different exit codes;
5. Unless mimicking another tool, every tool/agent should have an implementation of a `print_runtime_info` method;

## Other
1. I might have forgotten something;
2. YAGNI: Let's try to keep the code simple, adding parallelism and more complex features as the need arises;
3. Be nice;
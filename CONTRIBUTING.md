# Contributing
## Philosophy
Each tool follows a few principles I try to stick to:
- **Do one thing well**: Each tool has a focused purpose
- **Reasonable defaults**: Should work out of the box for common cases
- **Graceful handling**: Proper error messages and cleanup
- **Performance matters**: Use async I/O and parallel processing where it makes sense

## Project structure
1. **Workspace crates**
   - `crates/tools`: The classic CLI tools (e.g., `cat`, `ts`, `jwt`, `eh-read`, ...)
     - Entrypoints live in `crates/tools/src/bin` (one very thin file per binary);
     - Tool implementation lives in `crates/tools/src/tools/<tool name>` with:
       - `cli_utils.rs` (argument parsing/validation only);
       - `models.rs` (structs and data models);
       - `<tool name>_app.rs` (the actual tool logic);
       - `readme.md` (manual for the tool);
       - Additional files as needed to keep things tidy, scoped to the tool;
   - `crates/ai-tools`: AI-powered agents and helpers (e.g., the chatbot)
     - Entrypoints live in `crates/ai-tools/src/bin` (same rule: orchestration only);
     - Each agent lives under `crates/ai-tools/src/agents/<agent name>` with:
       - `cli_utils.rs` (runtime info, argument/env validation, but no agent logic);
       - `models.rs` (agent-specific types);
       - `<agent name>_app.rs` (the agent logic);
       - Optional `readme.md` for agent-specific docs;
   - `crates/shared`: Cross-cutting utilities and modules shared by multiple binaries
     - Group code by theme (e.g., `constants`, `eventhub`, `logging`, `sqlite`, `system`, `utils`);
     - If more than one tool/agent needs it, it belongs here rather than duplicating logic;
   - `crates/macros`: Procedural macros used across the workspace.
2. **Root `readme.md` file**: Add a reference to any new tools/agents to this file, or update/review any behaviors here;
3. **Build scripts (`build.sh`, and `build.bat`)**: The build command for every binary should be covered for all platforms (Windows, Linux, and MacOs);
4. **Installing on Non-Windows systems**: For non-Windows systems, where tools like `cat` and `touch` already exist, we build them but may skip copying to `dist`;
5. **Testing**: Try to add tests to every tool/agent and shared utility when it makes sense;

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
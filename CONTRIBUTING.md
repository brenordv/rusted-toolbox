# Contributing
## Philosophy
Each tool follows a few principles I try to stick to:
- **Do one thing well**: Each tool has a focused purpose
- **Reasonable defaults**: Should work out of the box for common cases
- **Graceful handling**: Proper error messages and cleanup
- **Performance matters**: Use async I/O and parallel processing where it makes sense

## Project structure
1. **Entrypoint**: Every tool must have its entrypoint in `src/bin` folder;
   - The code in this entrypoint file must contain only initialization and orchestration code, no "business logic";
2. **Tool source code**: The source for each tool must live in the `src/tools/<tool name>`, and inside the tool's folder, we must have;
   - `cli_utils.rs`: File that handles parsing and validating the CLI arguments, but no actual tool logic;
   - `models.rs`: File with all the structs that the tool uses;
   - `<tool name>_app.rs`: In here will live the "business logic" for the tool;
   - `readme.md`: Manual for the tool;
   - `other files in the tool's folder`: If we have too much code to leave in the previous file, we can create other files to organize everything, as long as it is specific to the tool;
3. **Shared code**: Code that can be used by any tool.
   -  If multiple tools use the shared code, create a module for that specific theme (like we have with `eventhub`);
4. **Root `readme.md` file**: Add a reference to any new tools to this file, or update/review any behaviors here;
5. **Build scripts (`build.sh`, and `build.bat`)**: The build command for every tool should be included here, in all platforms (Windows, Linux, and MacOs);
6. **Installing on Non-Windows systems**: For non-Windows systems, where tools like `cat`, and `touch` are already available, we're building the tools, but not copying them to the `dist` folder;
7. **Testing**: Try to add tests to every tool, and utility file;

## Tool structure
1. The tools can print/log information, trace, and warnings, but not errors;
2. If any errors that need to stop the execution happen, return the error to the entrypoint of the tool, and use `context` (from Anyhow) to add info what went wrong;
3. If the execution is successful, use `exit_success()` function;
4. If the execution fails, use `exit_error()` function to exit the app (from the entrypoint file only). Right now, I'm not using different exit codes;
5. Unless mimicking another tool, every tool should have an implementation of `print_runtime_info` method;

## Other
1. I might have forgotten something;
2. YAGNI: Let's try to keep the code simple, adding parallelism and more complex features as the need arises;
3. Be nice;
# Basic
[X] Migrate cli to the new pattern.
[X] Add more test coverage to the tool. Covered `from_args` flag expansion, the `needs_line_processing` truth table, parse-to-run paths for `-b` and `-A`, and combined `-n -s` / `-n -E` runs.
[X] Research improvements to the app.
[ ] Adopt `common_cli::broken_pipe` (added in common-cli 1.5.0; head, tail, and rxget already use it); cat defines its own BrokenPipe error type today.
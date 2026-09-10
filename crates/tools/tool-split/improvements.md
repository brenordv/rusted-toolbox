# Basic
[x] Migrate cli to the new pattern.
[x] Add more test coverage to the tool.
    Outcome (2.0.1): added tests for build_args path resolution (via an explicit base-dir
    parameter), CRLF input, read-error propagation, the interrupt outcome (line kept, exit 130),
    feedback formatting through a Write seam, and the zero-feedback-interval rejection.
[x] Research improvements to the tool.
    Outcome (2.0.1): four defects found and fixed: silent truncation with exit 0 on read errors,
    interrupt exiting 0 and dropping the in-flight line, a reachable panic on stdout flush in the
    progress feedback, and float-modulo cadence that silently disabled feedback for interval 0.

# Ideas (unchecked)
[ ] Adopt `common_cli::broken_pipe` (added in common-cli 1.5.0; head, tail, and rxget already use
    it); split now disables feedback on stdout write errors, but chunk writes still go to files
    only.

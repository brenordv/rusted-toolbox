# Basic
[ ] `--fail-on-skip` (or `--strict`) so `--check` pipelines can treat skipped files (binary, UTF-16/32, extension-filtered) as failures instead of unverified passes.
[X] Migrate cli to the new pattern.
[X] Add more test coverage to the tool.
    Outcome (2.0.1): direct tests for the extension filter, the on-disk classifier, and the stdin path (via a `Read` seam); the fixtures under `test-files/` are now exercised end to end.
[X] Research improvements to the tool.
    Outcome (2.0.1): no code defects found; the crate was already clean. The one finding was the tracked-but-unused fixtures in `test-files/`, now wired into tests.

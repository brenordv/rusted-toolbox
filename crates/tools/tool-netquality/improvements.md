# Basic
[X] Migrate cli to the new pattern.
[x] Add more test coverage to the tool.
    Outcome (2.1.0): 10 tests before, 39 after. Covered threshold parsing, the config-file resolvers (connectivity, speed, storage, notifications) with explicit paths, the connectivity outage state machine (entry, capped backoff growth, recovery), ThresholdCategory ranking, and CLI regressions (min-download flag, replace-urls, missing db file).
[x] Research improvements to the tool.
    Outcome (2.1.0): found and fixed: --min-download-threshold read from the upload flag; --replace-urls parsed but never applied; CLI mode refusing to start when the db file did not exist although startup creates it; the header showing the seconds-based cleanup interval as "0 day(s)"; --expected-upload wrongly required in CLI mode; threshold CLI values kebab-cased against the documented snake_case; the --config conflict list using flag spellings instead of argument IDs, so it never conflicted.
[x] Add missing versions in the changelog.
    Outcome (2.1.0): reconstructed the 1.1.0 entry; git history shows the crate entered the workspace at 1.1.0 with no earlier changes tracked here.
[x] Fix otel endpoint initialization. (this must come after the `logging-otel` crate improvement)
    Outcome (2.1.0): wired via logging-otel's app_boot_up_with_otel with the otel feature; endpoint from --otel-endpoint, the top-level otel_endpoint config key, or OTEL_EXPORTER_OTLP_ENDPOINT; main drops the guard before the exit helpers so telemetry flushes; the endpoint value is never logged and the header shows presence only.
[ ] Drop the crate-local reqwest 0.12 pin back to the workspace version when cfspeedtest moves to reqwest 0.13.
[x] Update the Dockerfile base from rust:1.93.0 to the workspace toolchain (1.98).
[ ] Expose a presence-check helper from logging-otel (is an endpoint configured, CLI value or OTEL_EXPORTER_OTLP_ENDPOINT) so the header line stops re-implementing the lib's resolution rules.
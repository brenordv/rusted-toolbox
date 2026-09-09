# Basic
[x] Add more test coverage to the tool, if reasonable. Also let's check we if we can group similar tests and group them using rstest.
    Outcome (1.1.1): added the disabled-level test (return-value assertion only, so it never touches the global dispatcher). The enabled export path stays compile-level covered (needs a live collector plus the once-per-process subscriber slot). Two tests with different fixtures; nothing for rstest to group.
[x] Research improvements to the app.
    Outcome (1.1.1): setup-failure warning no longer echoes the raw error (could plausibly embed the endpoint; unverifiable without raccoon-otel sources locally, so the invariant is now kept by construction).
[x] Create the `readme` file.
    Outcome (1.1.1): written, including the adoption recipe and the contracts that only lived in doc comments.
[x] Review otel endpoint implementation so it can be used by other crates as transparently as possible.
    Outcome (1.1.1): kept the extension-trait API unchanged (the one-call app_boot_up_with_otel already is the transparent path); wrote the adoption recipe in the readme instead of churning the API before the tools stage adopts it.
[ ] The disabled-level check couples to the empty-string sentinel from ToolLogLevel::to_tracing_level(); replace with an explicit is_disabled() seam if a new level ever lands.

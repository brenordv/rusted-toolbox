# Basic
[x] Add more test coverage to the tool, if reasonable. Also let's check we if we can group similar tests and group them using rstest.
    Outcome (1.1.1): grouped the per-generator non-empty tests with rstest in personal and commerce (case parameters over fn(&MockOptions) -> Result<String>); added description-length boundary cases and truncate-helper cases; dropped one password test subsumed by another. random/internet shape tests left ungrouped (each asserts a different structure).
[x] Research improvements to the app.
    Outcome (1.1.1): fixed commerce.product-description returning "" for lengths below one sentence (the truncate branch was dead for every input); contract documented on the function and MockOptions::length. Verified that validate() in lib.rs gates min>max and range==0 at the public entry, so direct-call panics in the generators are a documentation matter (noted on the generators module); idea recorded below.
[x] Create the `readme` file.
    Outcome (1.1.1): written: catalog, options table, password positioning (mock data, not credentials).
[ ] Narrow the generator functions' visibility (or move validation into them) so out-of-range options cannot panic via direct calls; today only generate_mock_data validates. Coordinate with tool-mock in the tools stage.
[ ] Support locale-aware generation: add a locale field to MockOptions and honor it in the generators; tool-mock then re-adds --locale wired to it (the flag was removed in tool-mock 2.0.1 because it was parsed and discarded).

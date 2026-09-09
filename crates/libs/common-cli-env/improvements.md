# Basic
[x] Add more test coverage to the tool, if reasonable. Also let's check we if we can group similar tests and group them using rstest.
    Outcome (1.1.1): added the process-env-wins-over-.env test. The five tests are structurally distinct scenarios (different fixtures and assertions), so rstest grouping would not clarify anything; left as plain #[test]s.
[x] Research improvements to the app.
    Outcome (1.1.1): reviewed; behavior is sound. One idea recorded below instead of built.
[x] Create the `readme` file.
    Outcome (1.1.1): written: search order, precedence, trust caveats.
[ ] Consider swallowing or replacing dotenvy's raw-line echo in parse errors (a mistyped secret can currently surface on stderr/logs). Trades away debuggability; decide when a real incident or reviewer pushes for it.

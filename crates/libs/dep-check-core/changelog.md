# Changelog

## 1.0.0
- First release: ecosystems, package input, the OSV.dev and deps.dev sources, the merge, the
  report, the rate-limit store.
- `Checker::check` asks OSV.dev about every package with a version (`querybatch` in batches of
  1,000, then each advisory's record four at a time) and returns a `Report`; every failure ends
  inside the report.
- deps.dev names the latest version of a package given without one, and gives each version's
  supply-chain facts (deprecation, licenses, published date, source repository, OpenSSF
  Scorecard, attestations) and its advisories, which merge with OSV.dev's.
- `render` writes the report as text, every outside string cleaned for a terminal.
- Rate-limit blocks are saved in `~/.rusted-toolbox/dep-check/rate-limits.json` and merged on
  save, so two runs at once keep each other's blocks.
- Feature `test-support`: `test_support::MockServer` and the clocks the tools' tests use.

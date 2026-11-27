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
- The GitHub Advisory Database joins as the third source: reviewed and malware advisories per
  package version, unauthenticated, one request at a time, under GitHub's rate rules; its
  severity labels and EPSS values join the merge.
- `render` writes the report as text, every outside string cleaned for a terminal;
  `render::not_found_sentence` gives a `not_found` package's words to any other view.
- `package::InputIssue` says which kind of refusal it is (`IssueKind`: a bad line, more than
  1,000 packages, or none), so a caller can word each one.
- Rate-limit blocks are saved in `~/.rusted-toolbox/dep-check/rate-limits.json` and merged on
  save, so two runs at once keep each other's blocks.
- Feature `test-support`: `test_support::MockServer` and the clocks the tools' tests use.
- Offline data (`offline`): `update::Updater` downloads OSV.dev's `all.zip` for an ecosystem
  (whole, with `If-None-Match`, no redirects, size required up front, 8 MiB per record) and
  installs it into `offline/<ecosystem>.sqlite` in one transaction, with progress and a cancel;
  an automatic update touches only Ready data that is due. `store::status` and `store::remove`
  read and delete a store under its lock; `settings` holds `offline.json` (`enabled`, `mode`,
  `auto_update`, changed under a lock) and `OfflineUse::decide`; `due::due` says which data a
  daily update refreshes.
- `CheckRequest` gains `offline` (`Off`, `Fallback`, `Only`). `Fallback` sends the packages
  OSV.dev did not answer to the offline data; `Only` asks no service. A no-finding offline answer
  counts only while the data is fresh (`OfflineUse::max_age`). Version ranges are matched by
  SemVer 2.0, NuGet, PEP 440, and RubyGems rules, and anything that cannot be read with
  certainty fails the package closed.
- The report gains `offline`, `sources[].data_as_of`, the `offline` source and its answers, and
  six `offline_*` failure kinds. `Checker::with_bases` takes the offline folder.
- `test_support` gains `raw_answer`, `zip_of`, `osv_zip`, and `osv_record`.

# Changelog

## 1.0.0
- First release: checks the packages of one ecosystem (Cargo, NuGet, PyPI, npm, RubyGems)
  against OSV.dev and deps.dev and prints one merged report, as text or `--format json`.
- Packages come from the arguments or, one per line, from stdin (at most 1,000 packages and
  1 MiB); a package without a version is checked at deps.dev's latest version.
- Exit codes: 0 nothing found, 1 vulnerabilities found, 2 usage or input error, 3 some
  packages unknown or not checked, 4 no service answered.
- Rate-limit blocks are saved in `~/.rusted-toolbox/dep-check/rate-limits.json`.

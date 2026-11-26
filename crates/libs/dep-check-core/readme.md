# dep-check-core

The engine of `dep-check` and `dep-check-gui`: it asks vulnerability services about one
ecosystem's packages and merges their answers into one report. Every failure of a service ends
inside the report, so a check always returns one.

```rust
let checker = Checker::new("dep-check 1.0.0", "dep-check/1.0.0 (+https://github.com/brenordv/rusted-toolbox)")?;
let packages = match parse_specs(Ecosystem::Cargo, ["tokio@1.20.0", "serde@1.0.215"]) {
    Ok(packages) => packages,
    Err(issue) => bail!("line {}: {}", issue.line, issue.message),
};
let report = checker
    .check(CheckRequest { ecosystem: Ecosystem::Cargo, packages }, system_clock())
    .await;
```

## Services and what each gets

| Service | Asked with | Gets |
|---|---|---|
| OSV.dev (`api.osv.dev`) | `POST /v1/querybatch`, then `GET /v1/vulns/{id}` | each package's name, ecosystem, and version |
| deps.dev (`api.deps.dev`, API v3) | GetPackage, GetVersion, GetProject, GetAdvisory | each package's name, ecosystem, and version; the source repository ids and advisory ids its own answers named |

A package written without a version is checked at the version deps.dev installs by default
(GetPackage's `isDefault`), and the report says so (`version_from: "deps_dev_latest"`). That
version passes the same rule as typed input before any other request uses it. A package
deps.dev does not know is `not_found`.

From GetVersion, every package also gets supply-chain facts: deprecated (and why), licenses,
published date, whether the version is the latest, the source repository and how deps.dev
knows it, the repository's OpenSSF Scorecard (GetProject), and how many attestations it has
and how many deps.dev verified. deps.dev's advisory ids are OSV ids, so its advisories merge
with OSV.dev's. The facts never change a status.

No service needs a key. The addresses are constants: no flag, file, or setting points a query
somewhere else, so a caller cannot be steered into asking a host that answers "clean". The
process environment still steers the connection the way it does for every reqwest program:
`HTTPS_PROXY` and `ALL_PROXY` set a proxy, and on Linux `SSL_CERT_FILE` and `SSL_CERT_DIR`
replace the system's trusted certificates.

OSV.dev states no rate limit, and deps.dev's reference states none either. The engine sends at
most four requests at a time to each, batches at most 1,000 packages per `querybatch`, stops
asking a source for the run when it answers 429, and stops after three connection failures or
timeouts in a row.

## Modules

- `ecosystem`: `Ecosystem` (Cargo, NuGet, PyPI, npm, RubyGems), each service's name for it, the
  words the parser takes (`rust`, `dotnet`, `python`, `node`, `ruby`, ...), and each
  registry's name matching rule.
- `package`: `parse_spec` and `parse_specs`, the input rules below, and `InputIssue`.
- `http`: the bounded client (10 s to connect, 30 s in all, 16 MiB per answer, no redirects),
  `SourceFailure`, and `encode_segment`.
- `rate_limits`: the saved blocks.
- `advisory`, `merge`, `report`: one source's advisory, the merge into findings, and the
  report.
- `engine`: `Checker`, `CheckRequest`, `Clock`.
- `sources::osv`: the OSV.dev client.
- `sources::deps_dev`: the deps.dev client: latest versions, version facts, projects,
  advisories.
- `render`: the report as text (`text_report`) and the lines a window builds its rows from
  (`header_line`, `sources_line`, `package_line`, `finding_lines`, `supply_chain_line`),
  every outside string on one line with control and bidirectional characters as spaces, and
  `count_phrase` ("1 package", "1,000 packages").

## Input

One package per line or argument: `name` or `name@version`. The text splits at its last `@`
that is not the first character, so npm's `@scope/name@1.0.0` keeps its scope.

- A name is ASCII letters, digits, and `-._~`, starting with a letter or a digit, at most 256
  characters. npm also takes `@scope/name`; the part after the `/` may start with `.` or `_`,
  and cannot be only dots.
  npm allows `!*'()` in names too, and this tool refuses them.
- A version is ASCII letters, digits, and `-._~+!`, starting with a letter or a digit, at most
  256 characters.
- At most 1,000 packages per check. Blank lines are skipped; a package given twice (same name
  by the registry's rule, same version) is checked once.

Nothing outside those sets reaches a URL, so a name cannot add a query parameter or retarget a
path.

## The report

`Report` serializes to JSON with `snake_case` fields. A script reads `outcome` (`clean`,
`vulnerable`, `incomplete`, `failed`) and each package's `status`:

- `vulnerable`: at least one finding, from any source, even one that failed later.
- `not_found`: the package or version is unknown and nothing has a finding.
- `clean`: a source answered and nothing has a finding.
- `not_checked`: no source answered; `reason` says why.

`outcome` is `failed` when every package is `not_checked`, else `vulnerable` when any package
is, else `incomplete` when any is `not_found` or `not_checked`, else `clean`. Advisories that
share an id or an alias are one finding, headed by OSV's id. Each package also carries
`supply_chain` (null when deps.dev gave none) and `answers`, one word per source. Strings in the
report are as the service sent them: a program that shows them must clean them first.

## The rate-limit file

`~/.rusted-toolbox/dep-check/rate-limits.json` holds one block per source that a service
rate-limited (a 429), with when it was seen, when it ends, and how many came in a row. A
source with a saved block is not asked until the block ends. Every block ends at most an hour
after it was seen. Both binaries share the file: a save re-reads it and merges this run's
changes source by source, so a CLI run and the window at once keep each other's blocks, and a
run that answered clears only the block it loaded. A save writes nothing when nothing changed.
The file holds blocks only: one whose content does not parse is logged at warn and replaced, and
one that cannot be read or written is logged at warn and left as it is. On Linux and macOS the
`dep-check` folder is created readable by you only.

## Logging

At info, `check started` and `check finished`; one `source finished` line per source, at info
when it answered everything and at warn otherwise, with the grouped failure reasons. Below that,
debug lines name each request's method, host, kind (`querybatch`, `vulns`), status, and time.
No line holds a URL, a package name at info or above, or text from an answer.

## Test support

The `test-support` feature adds `test_support::MockServer`, a local fake with routes, and the
`closed_port`, `cut_short`, `fixed_clock`, and `stepping_clock` helpers. A consumer lists the
crate again under `[dev-dependencies]` with the feature on:

```toml
[dev-dependencies]
dep-check-core = { workspace = true, features = ["test-support"] }
```

The log test runs only with the feature: `cargo test -p dep-check-core --features
test-support`. The live checks ask the real services and never run in CI:
`cargo test -p dep-check-core -- --ignored live_`.

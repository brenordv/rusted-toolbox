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
let request = CheckRequest {
    ecosystem: Ecosystem::Cargo,
    packages,
    offline: OfflineUse::Off,
};
let report = checker.check(request, system_clock()).await;
```

## Services and what each gets

| Service | Asked with | Gets |
|---|---|---|
| OSV.dev (`api.osv.dev`) | `POST /v1/querybatch`, then `GET /v1/vulns/{id}` | each package's name, ecosystem, and version |
| deps.dev (`api.deps.dev`, API v3) | GetPackage, GetVersion, GetProject, GetAdvisory | each package's name, ecosystem, and version; the source repository ids and advisory ids its own answers named |
| GitHub Advisory Database (`api.github.com`, REST 2026-03-10) | `GET /advisories?affects=...` for `reviewed` and `malware`, then the same for one package alone when an advisory names it beside another | up to 100 `name@version` per request |

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

GitHub allows 60 unauthenticated requests an hour per IP. Its requests run one at a time, in
chunks of at most 100 distinct names (a second version of a name goes into the next chunk), so
a check of up to 100 packages costs 2 requests plus extra pages and confirms. A confirm asks one
package alone, once per type, when an advisory names it beside another package of the chunk;
the advisory is that package's when the lone answer lists it. Any 403 or 429 is a block: until
`retry-after`, else until `x-ratelimit-reset` when `x-ratelimit-remaining` is 0, else a minute
doubled per block already saved in a row, never more than an hour. A 2xx that leaves no
requests is used and then blocks. A run that answered any package and then blocked starts the
saved count again at one. The count goes by packages, not requests: a run whose `reviewed`
request answered and whose `malware` request was refused answered no package, so it adds one. A package is answered by GitHub only when both advisory types, every
page, and every confirm about it finished. An advisory that names none of the packages asked
fails its chunk, since the mapping would otherwise be a guess; the rest of the answer is still
kept. Next pages follow the `Link` header only to the advisories address itself. GitHub's
severity label wins a finding's severity, and its EPSS values are kept as sent.

GitHub matches names exactly as its advisories spell them, and one package's advisories can
use several spellings (`Django` and `django` found disjoint sets on 2026-11-26), so GitHub's
answers can miss advisories for a name spelled another way. OSV.dev holds GitHub's reviewed
advisories under normalized names; in that check the merged report was the same for every
spelling.

## Offline data

OSV.dev publishes all of each ecosystem's records as one zip
(`https://storage.googleapis.com/osv-vulnerabilities/<ecosystem>/all.zip`). They aggregate the
GitHub Advisory Database, the PyPI and Rust advisory databases, and OpenSSF Malicious Packages.
`offline::update::Updater` downloads one on request and installs it into
`~/.rusted-toolbox/dep-check/offline/<ecosystem>.sqlite` (`cargo`, `nuget`, `pypi`, `npm`,
`rubygems`). Nothing is downloaded until a caller asks, and no flag, file, or variable sets the
address. Measured on 2026-11-27: npm 222.6 MB, PyPI 38.3 MB, RubyGems 5.4 MB, crates.io 4.0 MB,
NuGet 2.8 MB.

- An update downloads the whole zip. It sends the stored `ETag` as `If-None-Match`, so an
  unchanged copy costs one answer (a 304) and is marked current. A redirect fails the update, and
  so does a download whose `Content-Length` is missing or above 8 GiB, a body that stops short,
  an entry whose bytes fail zip's checksum, a record above 8 MiB, a record whose `id` or
  `affected` does not read, and a download with no record for the ecosystem.
- Records are read into memory, never extracted: entry names are not paths. Each record is kept
  without `details`, `credits`, or other ecosystems' entries, and withdrawn records are left out.
- The install replaces the rows inside one SQLite transaction. A failed or cancelled update keeps
  the old data, and a check that reads during an install keeps the snapshot it started with. Data
  counts as current from when its download was asked for. One updater runs at a time
  (`offline/update.lock`); a second one answers `Busy` without touching anything. The install's
  blocking work holds that lock too, so an update whose caller drops it mid-install still keeps a
  removal or another update out until the install ends. Progress comes at most every 100 ms; an
  automatic update stops after 10 minutes. A cancel flag that was set stops every update that
  reads it, so each update takes a fresh one.
- An automatic update (`Trigger::Auto`) re-reads the data under the update lock and goes on only
  when it is Ready and due (below); otherwise it answers `Skipped` and touches nothing, so no
  automatic run installs data nobody asked for or deletes a store to rebuild it.
- `store::status` reads one store as `NotDownloaded`, `Ready`, `Unavailable` (busy or held right
  now), `Locked` (a removal or rebuild holds it), `Unreadable` (not a database, corrupt, not in
  WAL mode), or `NewerFormat`. Only `Unreadable` is ever rebuilt, and only by an update someone
  asked for, which deletes the old files after its download is complete and marks the new store
  as being rebuilt until its install commits, so a rebuild that fails still reads `Unreadable`.
  `store::remove` deletes whichever of the store's database, WAL, shared-memory file, rollback
  journal, and leftover download are there, by name, and refuses while a check reads it or an
  update runs.
- `settings` keeps `~/.rusted-toolbox/dep-check/offline.json`: `enabled` (default on), `mode`
  (`fallback` or `always`), and `auto_update` (default off). A change holds
  `offline.json.lock` from its read to its write, so two writers keep each other's changes. A file
  a newer dep-check saved is never written; it is read for the fields this version knows, and a
  mode this version does not know reads as `always`. `OfflineUse::decide` turns the settings, the
  store's status, and a one-run force into the check's `offline`: `Off`, `Fallback`, or `Only`.
  Always mode checks online only an ecosystem with no data at all (which a first install still
  reads as); data that exists but cannot be used right now fails the check closed, so no package
  name is sent.
- `due::due` says whether Ready data is due for the daily update: confirmed current a day ago or
  more (or at a time later than now), with no failed attempt in the last hour.

In a check, `Fallback` sends the packages OSV.dev did not answer to the offline data, whether
OSV.dev failed or a saved block kept it from being asked. `Only` asks no service: a package
without a version is `not_checked`, and with no data every package fails. A finding from offline
data always counts. A no-finding answer counts only while the data was confirmed current within
`OfflineUse::max_age` (7 days in `Fallback`, 48 hours in `Only`); after that the package
is `not_checked` with the data's date. Inside that window, a package flagged after the data was
last confirmed reads `clean`. An offline-only check never asks deps.dev, so a name no registry
knows is not caught either.

Matching follows the OSV schema's evaluation. A `SEMVER` range sorts by SemVer 2.0 precedence; an
`ECOSYSTEM` range by the ecosystem's own rules: SemVer for crates.io and npm, NuGet's (one to four
numbers, labels without regard to case), PEP 440's (the `pep440_rs` crate), and RubyGems'
(`Gem::Version` from rubygems 4.1.1). Names are looked up under the registry's name rule. A
version that does not parse, a range type the tool does not know, a range with no `introduced`
event, a `GIT`-only entry, or a record that does not name the package leaves the package
`not_checked` ("Offline data has an advisory with a version this tool cannot compare"), never
`clean`.

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
- `sources::github`: the GitHub Advisory Database client: chunks, pages through `Link`
  headers, confirms, and GitHub's rate rules (`rate_limits::github_block_until`).
- `sources::offline`: `OfflineSource`, the offline data as a source, with its freshness rule.
- `offline`: `settings`, `store`, `update`, and `due` (above), the version orderings, and the
  matching.
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

`offline` (`off`, `fallback`, `only`) says how the check used offline data. When it was used,
`sources` ends with the `offline` source, whose `data_as_of` is when the data was last confirmed
current (null for every other source), and each package's `answers` has an `offline` word.

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

At info, `check started` (with `offline`) and `check finished`; one `source finished` line per
source, at info when it answered everything and at warn otherwise, with the grouped failure
reasons, and for offline data its `data_as_of`. Below that, debug lines name each request's
method, host, kind (`querybatch`, `vulns`, `offline_archive`), status, and time.

Offline data adds `offline update started` and `offline update finished` (info, or warn for a
failure other than "in use"; with `trigger`, `from` (the status the run found), `outcome`, `kind`,
sizes, times, `reason`, and `attempt_write_failed`), `offline data deleted to be rebuilt`,
`offline data removed`, `offline settings changed`, a warn when the settings file cannot be read
or written, and at debug the daily update's `offline update due` and the checkpoint after an
install. No line holds a URL, a package name at info or above, or text
from an answer or a record, except a record id (on one line, cut to 64 characters) in the words of
an install that failed on it.

## Test support

The `test-support` feature adds `test_support::MockServer`, a local fake with routes, the
`closed_port`, `cut_short`, `raw_answer`, `fixed_clock`, and `stepping_clock` helpers, and
`zip_of`, `osv_zip`, and `osv_record` to build an offline dump. A consumer lists the
crate again under `[dev-dependencies]` with the feature on:

```toml
[dev-dependencies]
dep-check-core = { workspace = true, features = ["test-support"] }
```

The log tests run only with the feature: `cargo test -p dep-check-core --features
test-support`. The live checks ask the real services and never run in CI:
`cargo test -p dep-check-core -- --ignored live_`.

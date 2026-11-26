# dep-check

Checks packages of one ecosystem for known vulnerabilities and supply-chain facts. It asks
OSV.dev and deps.dev about each package, merges what they say into one report, and prints it
as text or JSON. The exit code says whether anything was found, so a script can gate on it.

```bash
dep-check -e cargo tokio@1.20.0 serde
dep-check -e npm --format json @types/node@20.1.0 left-pad@1.3.0
printf 'tokio@1.20.0\nserde@1.0.215\n' | dep-check -e cargo
```

## Usage

```
dep-check [OPTIONS] --ecosystem <ECOSYSTEM> [PACKAGE]...

  PACKAGE                     name or name@version; when none is given and stdin is piped,
                              one per line from stdin
  -e, --ecosystem ECOSYSTEM   cargo, nuget, pypi, npm, rubygems (also rust, crates, crates.io,
                              dotnet, csharp, c#, python, pip, node, javascript, js,
                              typescript, ts, ruby, gem)
  -f, --format FORMAT         text (default) or json
```

plus the shared logging flags (`--log-level`, `--log-to-file`, `--log-to-console`,
`--rotate-log-file-by-day`, `--app-header`). With `--format json`, `--log-to-console` and
`--app-header` are refused: both write to stdout, which then holds the JSON report and nothing
else.

`--help` and `--version` work only on their own: among other arguments they are a usage error
(exit 2), so a package list built by a script can never print the help and exit 0 with nothing
checked. A list that may hold a name starting with `-` goes after `--`:

```bash
dep-check -e cargo -- "$@"
```

Log lines go to stderr, warnings and up by default: one line per service that did not answer
every package, and one error line on exit 2 or 4.

## Exit codes

| Code | When |
|---|---|
| 0 | every package was checked and nothing was found |
| 1 | at least one package has a finding: a vulnerability or a malicious-package advisory |
| 2 | a usage error, or the packages could not be read |
| 3 | nothing was found, but some package is not known to deps.dev or could not be checked |
| 4 | no service answered for any package, or the run itself failed (stdin unreadable, stdout unwritable) |

A package no service knows (a typo, or a name someone is squatting on) ends in 3, never 0. A
vulnerability beats everything else: one vulnerable package and one unchecked one end in 1.

## Input

- One package per argument, or per stdin line: `name` or `name@version`. npm's scoped names
  keep their scope: `@types/node@20.1.0`.
- A package without a version is checked at the version deps.dev installs by default, and
  the report says so ("(latest, from deps.dev)").
- Names are ASCII letters, digits, and `-._~`, starting with a letter or a digit; versions add
  `+` and `!`. npm allows `!*'()` in names too, and dep-check refuses them. Anything else is
  refused with the argument or line number.
- At most 1,000 packages, 1 MiB of stdin (UTF-8, a leading BOM and CRLF line ends are fine).
  Blank lines are skipped and a package given twice is checked once.

## What each service gets

| Service | Gets |
|---|---|
| OSV.dev | each package's name, ecosystem, and version |
| deps.dev | the same, plus the source repository ids and advisory ids its own answers named |

No service needs a key, and no flag changes which services are asked or where: the addresses
are built in. The process environment still steers the connection, as for any program built
on reqwest: `HTTPS_PROXY` and `ALL_PROXY` set a proxy, and on Linux `SSL_CERT_FILE` and
`SSL_CERT_DIR` replace the system's trusted certificates.

## Rate limits

When a service answers 429, dep-check stops asking it for that run and saves when it may be
asked again in `~/.rusted-toolbox/dep-check/rate-limits.json`. The next run (of dep-check or
dep-check-gui, which share the file) skips that service until then, and the other services
still answer. A service that answered clears the block it found. The file holds those times
and nothing else; deleting it is safe.

## The report

Text output, one block per package:

```
dep-check 1.0.0  Cargo  3 packages: 1 vulnerable, 1 clean, 1 not found
OSV.dev answered; deps.dev answered

tokio 1.20.0  vulnerable, 1 advisory
  RUSTSEC-2023-0005  medium  fixed in 1.18.4, 1.20.3, 1.23.1
    tokio::io::ReadHalf<T>::unsplit is unsound
    also CVE-2023-22466, GHSA-7rrj-xr53-82p7
    CVSS 3 5.4 (deps.dev). From OSV.dev, deps.dev
    https://rustsec.org/advisories/RUSTSEC-2023-0005.html
  MIT. Source github.com/tokio-rs/tokio (unverified metadata). Scorecard 8.1 (2026-11-17). Published 2022-07-12. Not the latest version.

serde 1.0.215 (latest, from deps.dev)  clean
  MIT OR Apache-2.0. Source github.com/serde-rs/serde (unverified metadata). Scorecard 7.6 (2026-11-17). Published 2024-11-11.

left-padd  not found: deps.dev does not know this package
```

Every text from a service or a package's metadata is shown on one line, with control and
bidirectional characters as spaces, and only `https://` links are shown.

`--format json` prints the same report as one JSON document with `snake_case` fields. A script
reads `outcome` (`clean`, `vulnerable`, `incomplete`; when no service answered there is no
report, only exit 4 and the error line) and each package's `status`
(`vulnerable`, `not_found`, `clean`, `not_checked`); `findings`, `supply_chain`, `answers` (one
word per service), and `sources` (what each service did) carry the rest. Strings in the JSON
are as the services sent them: a program that shows them must clean them first.

# Whurl
Whurl, a Wrapper for [Hurl](https://hurl.dev/), that orchestrates composable Hurl request suites. 
It discovers requests under `requests/`, expands top-of-file `@include` directives, and runs the merged document with
the embedded Hurl engine so you can chain requests, reuse captures, and ship curated collections.

Hurl handles the actual requests, assertions, reporting, capturing, etc., so it will work with your existing `hurl` files!

This kind of replaces my old one made using Go ([go-request](https://github.com/brenordv/go-request)).

## Why?
While I love the simplicity of Hurl, I hate the idea of having to repeat the same requests on multiple files. This would
make my request library way less maintainable.

For instance, when using it to test a medium to large API, where all endpoints are behind authentication, I would need
to repeat the request to get a token on all the request files.

To avoid this, I created this tool that allows me to include one request into another, while still relying on Hurl to
do the heavy work. This way, I can reuse the same request in multiple files, and I can also share variables between
them.

It's not perfect, but it's a good start, and helps me solve this problem.

## What It Does
- Discovers APIs and Hurl files under a `requests/<api>/` hierarchy (or a custom `WHURL_REQUEST_HOME`).
- Resolves `# @include ...` directives before execution, including `quiet`, `silent`, and `env=NAME` options.
  - Options can be added with the following syntax: `# @include:[option1, option2,...optionN]  <file>` 
- Provides commands to list collections, preview the merged document, and execute through Hurl.
- Surfaces rich reporting: optional JSON artifacts, test-friendly summaries, and source remapping.

## Features
- Automatic `requests/` root detection (crate-relative, binary-relative, or `WHURL_REQUEST_HOME` override).
- Include graph cycle detection plus optional boundary markers for readability.
- Per-include environment overrides (`env=NAME`) and inline variable feeds (`-> { k=v }`) on `@include`.
- Source-to-merged line mapping so failures are reported against original files.
- Variable layering from `HURL_*` environment variables, shared `_global.hurlvars`, named env files, arbitrary files, and `--var`.
- Dynamic variables via `_vars/*.dvars` files and `# @vars` directives, including generators like `$uuid`, `$date[+2]`, `$random["a", "b"]`, and guarded `$shell(...)` execution.
- Secret-aware variable injection (keys containing `token`, `secret`, etc. stay hidden in logs).
- Per-entry elapsed-time display with a running total and repeat-hit deltas (within one run).
- Embedded Hurl runner with controllable verbosity (`-v` / `-vv`) and context-aware file resolution.

### WHURL_REQUEST_HOME
The `WHURL_REQUEST_HOME` environment variable can be used to override the default `requests/` root.

By default, Whurl will look for `requests/` in the current working directory, and if not found, in the binary's 
directory.

## Syntax
### #@include
The star of the show for this app is the ability to reference one `hurl` file from another.
This is done with the `# @include` directive. 
When used, Whurl resolves the file path relative to the file it's included in, and merges the content into a single 
Hurl, making it possible to reuse request snippets and share them across collections.

With this, variables captured in one request can be reused in another. Which means that if you have the same variable
being captured in multiple requests with the same name, it will be overridden.

Multiple includes can be used, one per line, and they will be executed in the order they appear. This also works for
nested includes (including a file that contains another include).

Whurl will do all that in memory, keeping the original files untouched.

#### Include options
Options ride the bracket block: `# @include:[option1, option2] path`.

- `quiet`: skip response-body logging for that file's entries.
- `silent`: suppress all whurl log lines for that file's entries (implies `quiet`).
- `env=NAME`: run the included file's API against environment `NAME` instead of the run's `--env`. The override
  inherits down that include's subtree (an include's own `env=` beats an inherited one) and applies even when no
  `--env` was passed. Environments resolve per API while variables merge globally, so the override redirects the env
  layer of the whole API the included file belongs to; two includes of the same API with different `env=` values
  collide, and the first one encountered wins with a warning. A missing environment fails before any request runs,
  citing the directive that set it. Since the override selects which `NAME.hurlvars`/`NAME.dvars` files load, with the
  shell gate enabled it also selects which `.dvars` code runs; the trust notes under "Fetching secrets with $shell"
  apply.

For example, a request that always authenticates through the `auth` API's `production` environment, while the
including file's own API keeps following the run's `--env`:

```hurl
# @include:[env=production] auth/login

GET https://httpbin.org/anything
```

Here `login` (and anything it includes) loads the `auth` API's variable files from `production`, even when the run
was started with `--env dev` or no `--env` at all.

Options inherit down the include subtree. An unknown `key=value` option logs a warning (a typo like `evn=dev` would
otherwise silently run the default environment); unknown bare words are ignored as before.

#### Feeding variables to an include
`# @include path -> { key=value, ... }` hands variables to the run when that include is pulled in (the arrow clause is
called a variable feed):

```hurl
# @include my-get-request -> { queryStringType="question", correctAnswer=42, extra={{user}} }
```

- A value is a bare word (`42`), a double-quoted literal (`"question"`; commas allowed inside, no escape sequences), or
  a `{{reference}}` to another variable. A reference must exist or the run fails before any request executes, citing
  the file and line.
- Precedence: feeds apply after every file-based layer (`_global`, env files, `@vars`) and lose only to `--vars-file`
  and `--var`. A `{{reference}}` sees the variable's final value when it comes from `--var`, `--vars-file`, or any file
  layer; a key set only by a later feed is not visible to an earlier one.
- Variables merge globally: a fed variable is visible to every entry in the merged run, not only the included file's
  entries. Colliding sources log the usual collision warning; feed origins name the include and the feeding line,
  never the value.
- An include expands once per run, but every occurrence's feed applies, in order; when two feeds set the same key, the
  later one wins. On a repeated include the feed still applies even though a conflicting `env=` on that repeat is
  dropped with a warning (environments are one per API; feeds are cumulative inserts).
- Nothing may follow the closing brace, and paths containing a literal `->` are not supported; both fail as a parse
  error with the file and line (`dry-run` included).

### #@vars
Top-of-file `# @vars <name>` directives load `_vars/<name>.hurlvars` first (when present) followed by `_vars/<name>.dvars` (extensions optional and case-insensitive).
The `.hurlvars` files provide static `KEY=VALUE` entries while `.dvars` files use generator expressions evaluated at runtime.
Whurl automatically layers `_vars/_global.{hurlvars|dvars}` and `<env>.{hurlvars|dvars}` (when present) for the primary API **and** any cross-API includes before applying CLI overrides.
It now errors when a directive references a name with no matching files so missing variables are surfaced early.

Supported generators include:
- `$now`, `$utcnow`: ISO8601 timestamps (local or UTC).
- `$date`, `$date[+N]`, `$utcdate`, `$utcdate[-N]`: dates in `YYYY-mm-dd`.
- `$time`, `$time[+N]`, `$utctime`, `$utctime[-N]`: times in `HH:mm:ss` (offsets in seconds).
- `$uuid`: random UUIDv4 values.
- `$int`, `$int[min, max]`: random integers (inclusive, negatives allowed, `min < max`).
- `$float`, `$float[min, max]`: random floats (inclusive, negatives allowed, `min < max`).
- `$random["option1", "option2", ...]`: pick a random quoted option (commas allowed inside the quotes).
- `$shell(<command>)`: run a shell command (only when `WHURL_ALLOW_DYN_SHELL_VARS=true`; legacy `WHURL_ALLOW_DYN_BASH_VARS=true` is still respected; destructive commands are blocked and the detected shell is platform-aware).

On Windows, Whurl uses `cmd /C` for `$shell(...)` commands. On Linux and macOS it honors the `SHELL` environment variable, falling back to `sh` when it is unset.

> **Note:** When you pass `--env NAME`, every API involved (the main request and any included APIs) must provide either `NAME.hurlvars` or `NAME.dvars`. Whurl raises an error if both files are missing so you can catch incomplete environment definitions early.

When not running in silent mode Whurl logs each generated variable's name and source file; the values stay out of the logs since they routinely hold secrets.

`$shell` results are cached for the run: a byte-identical `$shell(...)` expression executes once per run, and every
later occurrence, in any file or API, reuses the first result (the assignment log marks these with `cached=true`).
Distinct expressions always execute. Two consequences: a token fetched at run start lives for the whole run, and a
non-deterministic command (a timestamp, a one-time-code fetcher) duplicated across files now produces one shared value
instead of several. Only successful results are cached; other generators (`$uuid`, `$int`, ...) never are.

### Hurl files
This app still relies on [Hurl files](https://hurl.dev/docs/hurl-file.html), and its syntax.
So, if you need to learn or a refresher, check the official docs:
- [Entry](https://hurl.dev/docs/entry.html)
- [Request](https://hurl.dev/docs/request.html)
- [Response](https://hurl.dev/docs/response.html)
- [Capturing Response](https://hurl.dev/docs/capturing-response.html)
- [Asserting Response](https://hurl.dev/docs/asserting-response.html)
- [Filters](https://hurl.dev/docs/filters.html)
- [Templates](https://hurl.dev/docs/templates.html)
- [Grammar](https://hurl.dev/docs/grammar.html)

## Request Examples
Here's a basic hurl request:
```hurl
# Basic GET request to httpbin. Asserts success and captures returned data.
GET https://httpbin.org/get
HTTP 200

[Asserts]
jsonpath "$.url" == "https://httpbin.org/get"
status >= 200
status < 300

[Captures]
status_code: status
response_url: jsonpath "$.url"
```
In this request, the `status` and `response_url` variables are captured from the response.
This request is in a file named `basic.hurl` and lives under `requests/httpbin/`.

And here's how to include this into another request:
```hurl
# @include basic

# Builds on the basic request by reusing captured data.
GET https://httpbin.org/anything?source={{ response_url }}
HTTP 200

[Asserts]
jsonpath "$.args.source" == "{{ response_url }}"
jsonpath "$.url" == "https://httpbin.org/anything?source=https:%2F%2Fhttpbin.org%2Fget"
```
With the first line (`# @include basic`), Whurl resolves the `# @include` directive and merges the two files. So 
everything in the `basic` will be done first, and will be available to the second line.
You can add as many includes as you need. Just add one line after the other.

### Runtime example
If you run the provided example `requests/httpbin/extended.hurl` file with `--app-header`, you'll get the following
result (without the flag, only the log lines print):
```text
whurl (3.1.0)
---------------------------------------------------
- Basic Runtime Config
  - Verbose mode: 0
  - Log level: Info
  - Log to stdout: false
  - Log to file: false
  - Rotate log file by day: false
- Tool Runtime Config
  - API: httpbin
  - Request: extended

 INFO whurl::vars::dynamic: dynamic variable assigned variable=call_id file=<requests-root>/httpbin/_vars/session.dvars line=1
 INFO whurl::vars::dynamic: dynamic variable assigned variable=first_name file=<requests-root>/httpbin/_vars/session.dvars line=3
 WARN whurl::models: Environment variable collision; newer source overrides previous value key=call_id new_source=dynamic vars file `httpbin/_vars/session.dvars` previous_source=# @vars `session` hurlvars `httpbin/_vars/session.hurlvars`
 INFO whurl::whurl_app: Entry #1 Call #1 → GET https://httpbin.org/get
 INFO whurl::whurl_app: Status: 200 (Http2)
 INFO whurl::whurl_app: Response Body:
{
  "args": {},
  "headers": {
    "Accept": "*/*",
    "Host": "httpbin.org",
    "User-Agent": "hurl/8.0.1",
    "X-Amzn-Trace-Id": "<redacted>"
  },
  "origin": "<redacted>",
  "url": "https://httpbin.org/get"
}
 INFO whurl::whurl_app: [Elapsed: 308 ms | Total: 308 ms]
 INFO whurl::whurl_app: Entry #2 Call #1 → GET https://httpbin.org/anything?source=https%3A%2F%2Fhttpbin.org%2Fget&call_id=<redacted>&first_name=Larue&food=apple&score=834
 INFO whurl::whurl_app: Status: 200 (Http2)
 INFO whurl::whurl_app: Response Body:
{
  "args": {
    "call_id": "<redacted>",
    "first_name": "Larue",
    "food": "apple",
    "score": "834",
    "source": "https://httpbin.org/get"
  },
  "method": "GET",
  "origin": "<redacted>",
  "url": "https://httpbin.org/anything?source=https:%2F%2Fhttpbin.org%2Fget&call_id=<redacted>&first_name=Larue&food=apple&score=834"
}
 INFO whurl::whurl_app: [Elapsed: 27 ms | Total: 335 ms]
```
(Some dynamic-variable lines and response fields are trimmed for brevity. Logs go to stderr; the header goes to
stdout.)

## Requests Layout
Organize your collections like this:
```
requests/
  httpbin/
    basic.hurl
    extended.hurl
    env-demo.hurl
    _vars/
      dev.hurlvars
      production.hurlvars
```
- Each API gets its own directory; request files use the `.hurl` extension.
- `_vars/` holds named environment files addressed by `--env`.
- Override the root directory by setting `WHURL_REQUEST_HOME=/path/to/requests`.

## Subcommands
### list
- `whurl list`: prints every API discovered under the requests root.
- `whurl list <api>`: lists the requests (file stems) available for that API. Reports when empty.

### run
Runs the selected request after all includes are expanded.
```
whurl run <API> <FILE> [OPTIONS]
```
- `--env NAME`: load `<API>/NAME.hurlvars` first, falling back to `_vars/NAME.hurlvars`; a `NAME.dvars` file (same lookup order) is also accepted.
- `--vars-file PATH`: merge variables from an arbitrary file.
- `--var KEY=VALUE`: inline variable overrides (repeatable, highest precedence).
- `--file-root PATH`: adjust the base directory for response/file assertions (relative values are resolved against the API directory; this does **not** change where Whurl discovers request files).
- `--json PATH`: emit the Hurl JSON report alongside console output.
- `--print-only-full-response`: suppress header/logs and stream the JSON report to stdout.
- `--print-only-response-body`: suppress header/logs and print only the last response body.
- `--silent`: suppress whurl's runtime output (includes marked `[quiet]` / `[silent]` also hush logs).
- `--test`: print a concise summary with failure snippets after execution.
- `-v` / `-vv`: increase embedded Hurl verbosity (request/response debug logs). This never changes whurl's own log level.
- `--app-header`: print the standard runtime header (tool name, version, logging config, and the run's inputs) before execution. Suppressed by `--silent` and the `--print-only-*` modes, which promise header-free output.
- `--log-level LEVEL`: set whurl's tracing level explicitly. When omitted, whurl derives it: Error under `--silent`/`--print-only-*`, Info otherwise. An explicit value beats the derivation; `RUST_LOG` beats both when set, and only `disabled` silences everything including `RUST_LOG`.
- `--log-to-console`: send logs to stdout instead of the default stderr. Leave this off with `--print-only-*` in pipelines, or logs interleave with the machine-readable stdout payload.
- `--log-to-file` / `--rotate-log-file-by-day`: append logs to the tool's logs folder under your home directory. Execution logs include response bodies, so the file can end up holding whatever your APIs return.

`dry-run` accepts the same shared flags.

#### About `--file-root`
Whurl resolves relative paths in the `.hurl` file against the API directory.
This is useful for when you want to use a file from the API directory as a payload, but you don't want to copy it 
into the API directory.
Consider the following file structure:

```
requests/
  httpbin/
    send-json.hurl
payloads/
  fixtures/
    create-user.json
```

and the following request:
```hurl
POST https://httpbin.org/post

[Body]
file,"fixtures/create-user.json"
```

To run this, you should use the following command:
```bash
whurl run httpbin send-json --file-root /path/to/payloads
```

### dry-run
```
whurl dry-run <API> <FILE> [--show-boundaries <true|false>] [other exec flags]
```
- Expand includes and prints the merged `.hurl` document.
- Boundary markers (`# --- begin include ... ---`) are shown by default; disable with `--show-boundaries false`.
- Accepts the same execution arguments (`--env`, `--vars-file`, etc.) to confirm resolution.

## Variables & Secrets
- `HURL_*` process environment variables are ingested automatically (prefix stripped, key lower-cased).
- Add an optional `_global.hurlvars` alongside each API (either directly under the API folder or inside `_vars/`). 
Whurl loads it automatically for every run, so you only keep truly shared values there. When a request includes another
API, that API’s global file is pulled in as well.
- Named env files live in the API directory or `_vars/` subdirectory.
- `--vars-file` supports absolute paths or paths relative to the API directory.
- Inline `--var KEY=VALUE` flags win last and are ideal for ad-hoc overrides.
- Keys containing `token`, `secret`, `password`, or `authorization` are treated as secrets when passed to Hurl. This 
means that when Whurl hands variables to the embedded Hurl engine, it checks each key; if the key’s name includes token,
secret, password, or authorization, it marks those as sensitive. The Hurl runner then keeps the value out of verbose
logs so you don’t leak credentials.

### Fetching secrets with $shell
`$shell(...)` in a `.dvars` file can pull a secret from a vault at run start, so API keys never sit in your request
files. Example with Azure Key Vault, in `_vars/session.dvars`:

```
api_token=$shell(az keyvault secret show --vault-name my-vault --name api-key --query value -o tsv)
```

Enable the gate per invocation instead of exporting it globally:

```bash
WHURL_ALLOW_DYN_SHELL_VARS=true whurl run my-api login --env dev
```

Three things to know before relying on this:

- Masking is keyed on the variable name, nothing else. Keys containing `token`, `secret`, `password`, or
  `authorization` reach the Hurl engine as secrets and stay out of its verbose output. A name like `api_key` matches
  none of those needles and is not masked, which is why the example uses `api_token`. Whurl's own logs never carry
  variable values either way (the assignment log records name, file, and line only), but engine-side redaction depends
  on the key name.
- The command text and its stderr can surface in error output. A denylist rejection echoes the full command, and a
  non-zero exit echoes trimmed stderr. Never inline secret material in the command itself (a `--client-secret abc`
  flag, say); fetch by name, and prefer quiet output flags like `-o tsv` so a failing command doesn't spill values.
- While the gate is set, `.dvars` files are executable code. The destructive-command denylist is a courtesy guard, not
  a security boundary: wrappers, scripts, and command substitution get around it. Treat a `.dvars` file with the same
  trust as a shell script you'd run, and do not run collections from untrusted sources with the gate enabled.

## Logging & Reports
- Default runs print info-level per-entry logs to stderr; the runtime header is opt-in via `--app-header`.
- Each entry logs `[Elapsed: 121 ms | Total: 147 ms]` after its calls: elapsed is that entry's transfer time, total is
  the running sum across all entries so far (entries hidden by `silent` includes still count toward the total). A
  repeat hit of the same method + URL (+ environment) within one run adds a delta against the previous attempt, e.g.
  `[Elapsed: 25 ms (-96 ms) | Total: 147 ms]`; nothing persists across runs. Durations print as whole milliseconds
  under one second and one-decimal seconds from there up.
- The `--test` summary shows each entry's elapsed time in the same format; the column prints whenever the summary
  prints (`--test` output is not suppressed by `--silent`).
- Includes tagged `quiet` skip response body logging; `silent` suppresses logs entirely for that file.
- `--test` mode summarizes pass/fail counts and annotates failures with source file/line snippets.
- `--json PATH` writes the canonical Hurl JSON report; combine with `--print-only-full-response` for pipelines.
- Non-zero exit codes reflect either include/resolve errors or Hurl assertion failures (exit code 1).

## Examples
- List of APIs:
  ```bash
  whurl list
  ```
- Preview a merged request without running it:
  ```bash
  whurl dry-run httpbin extended --show-boundaries false
  ```
- Run with an environment file, inline override, JSON artifact, and verbose logging:
  ```bash
  whurl run httpbin env-demo --env production --var message="Smoke test" --json reports/httpbin.json -v
  ```
- Read payload fixtures outside the API directory by setting a file root:
  ```bash
  whurl run httpbin send-json --file-root /path/to/payloads
  ```
- Export only the execution result (no logs):
  ```bash
  whurl run httpbin basic --print-only-full-response > result.json
  ```
- Print only the final response body:
  ```bash
  whurl run httpbin basic --print-only-response-body
  ```
- Run with a concise pass/fail summary (failures are annotated with source file/line snippets):
  ```bash
  whurl run httpbin extended --test
  ```

The `requests/httpbin/` folder in this crate ships runnable samples: `quiet-import` and `silent-import` for the
include options, `nested-extend` for nested includes, `extended cross-api` for cross-API includes, and `env-demo`
plus `env-demo-basic` for environment layering.

# Build
## Linux
> If anything fails here, please check [the official docs](https://hurl.dev/docs/installation.html#build-on-linux).

### Debian-based distributions
```bash
apt install -y build-essential pkg-config libssl-dev libcurl4-openssl-dev libxml2-dev libclang-dev
```

### Fedora based distributions
```bash
dnf install -y pkgconf-pkg-config gcc openssl-devel libxml2-devel clang-devel
```

### Red Hat based distributions
```bash
yum install -y pkg-config gcc openssl-devel libxml2-devel clang-devel
```

### Arch based distributions
```bash
pacman -S --noconfirm pkgconf gcc glibc openssl libxml2 clang
```

### Alpine based distributions
```bash
apk add curl-dev gcc libxml2-dev musl-dev openssl-dev clang-dev
```

## Build on macOS
> Same as before, if anything fails here, please check [the official docs](https://hurl.dev/docs/installation.html#build-on-linux).

```bash
xcode-select --install
brew install pkg-config
```

## Windows
> If anything fails here, please check [the official docs](https://github.com/Orange-OpenSource/hurl/blob/master/contrib/windows/README.md).


### Build prerequisites for embedding Hurl (dynamic linking)
The steps below set up everything needed to compile this Rust app (that embeds **Hurl**) on Windows using 
**dynamic linking**.

### Why do we need these steps?

- **Hurl crate from crates.io fails on Windows** because its build script tries to embed an icon from a path not 
included in the published crate. Pointing Cargo at the **GitHub repo tag** fixes that.
- **`libxml2` is required** by Hurl. On MSVC, the `libxml` Rust crate discovers it via **vcpkg**. 
Installing `libxml2:x64-windows` makes the header/libs available to the build.
- **`bindgen` needs `libclang.dll`** to generate bindings on Windows. 
Installing **LLVM** and pointing `LIBCLANG_PATH` to its `bin` folder solves this.

### 1) Use the Hurl GitHub repo (avoids missing icon during build)

> The project is already setup like this. I'm keeping this here so you know why we're doing it this way.

In `Cargo.toml`:

```toml
hurl = { git = "https://github.com/Orange-OpenSource/hurl", tag = "8.0.1" }
hurl_core = { git = "https://github.com/Orange-OpenSource/hurl", tag = "8.0.1" }
```

> This avoids the `RC2135 : file not found: ../../bin/windows/logo.ico` error during the `hurl` build on Windows.

### 2) Install vcpkg and libxml2 (MSVC x64)

Open a `PowerShell` terminal:

```powershell
# Install vcpkg (once)
git clone https://github.com/microsoft/vcpkg $env:USERPROFILE\vcpkg
& $env:USERPROFILE\vcpkg\bootstrap-vcpkg.bat

# Make vcpkg discoverable to build scripts
$env:VCPKG_ROOT = "$env:USERPROFILE\vcpkg"
[Environment]::SetEnvironmentVariable("VCPKG_ROOT", $env:VCPKG_ROOT, "User")

# Install libxml2 for your MSVC x64 toolchain
# In this step, you might need to expand the env to the actual path.
& $env:VCPKG_ROOT\vcpkg.exe install libxml2:x64-windows

# (Optional) Integrate with MSBuild shells
& $env:VCPKG_ROOT\vcpkg.exe integrate install

# Tell vcpkg-rs to link dynamically (matches dynamic-link packaging)
$env:VCPKGRS_DYNAMIC = "1"
[Environment]::SetEnvironmentVariable("VCPKGRS_DYNAMIC", "1", "User")
```

> `libxml2:x64-windows` provides headers and DLLs; `VCPKGRS_DYNAMIC=1` ensures the Rust build links against the dynamic (DLL) triplet.

### 3) Install LLVM and point bindgen to libclang

```powershell
# Install LLVM (ships libclang.dll)
winget install LLVM.LLVM

# Point bindgen at libclang.dll (path to LLVM\bin)
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
[Environment]::SetEnvironmentVariable("LIBCLANG_PATH", $env:LIBCLANG_PATH, "User")
```

> `bindgen` requires `libclang.dll` at build time to parse C headers (like libxml2’s).

### 4) Re-open the terminal and build

```powershell
cargo clean -p libxml
cargo build -p whurl
```

### Notes
- **Architecture must match**: if you target `x86_64-pc-windows-msvc`, use `libxml2:x64-windows` and x64 LLVM.
- **Runtime DLLs**: with dynamic linking, your built exe will need `libxml2.dll` (and its deps like `zlib1.dll`,
`iconv-2.dll`, `charset-1.dll`) at runtime. Add `%VCPKG_ROOT%\installed\x64-windows\bin` to `PATH` during development,
or copy those DLLs next to your exe when packaging.
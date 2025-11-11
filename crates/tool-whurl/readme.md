# Whurl
Whurl, a Wrapper for [Hurl](https://hurl.dev/), that orchestrates composable Hurl request suites. 
It discovers requests under `requests/`, expands top-of-file `@include` directives, and runs the merged document with
the embedded Hurl engine so you can chain requests, reuse captures, and ship curated collections.

Hurl handles the actual requests, assertions, reporting, capturing, etc., so it will work with your existing `hurl` files!

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
- Resolves `# @include ...` directives before execution, including `quiet` and `silent` modifiers.
  - Modifiers can be added with the following syntax: `# @include:[modifier1, modifier2,...modifierN]  <file>` 
- Provides commands to list collections, preview the merged document, and execute through Hurl.
- Surfaces rich reporting: optional JSON artifacts, test-friendly summaries, and source remapping.

## Features
- Automatic `requests/` root detection (crate-relative, binary-relative, or `WHURL_REQUEST_HOME` override).
- Include graph cycle detection plus optional boundary markers for readability.
- Source-to-merged line mapping so failures are reported against original files.
- Variable layering from `HURL_*` environment variables, shared `_global.hurlvars`, named env files, arbitrary files, and `--var`.
- Secret-aware variable injection (keys containing `token`, `secret`, etc. stay hidden in logs).
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
If you run the provided example `requests/httpbin/extended.hurl` file, you'll get the following result:
```text
WHURL v1.0.0
---------------------------------------------------
- API: httpbin
- Request: httpbin\extended.hurl
---------------------------------------------------

2025-11-09T21:07:28.513993Z  INFO whurl: Entry #1 Call #1 → GET https://httpbin.org/get
2025-11-09T21:07:28.514117Z  INFO whurl: Status: 200 (Http11)
2025-11-09T21:07:28.514245Z  INFO whurl: Response Body:
{
  "args": {},
  "headers": {
    "Accept": "*/*",
    "Host": "httpbin.org",
    "User-Agent": "hurl/7.0.0",
    "X-Amzn-Trace-Id": "<redacted>"
  },
  "origin": "<redacted>",
  "url": "https://httpbin.org/get"
}
2025-11-09T21:07:28.514358Z  INFO whurl: Entry #2 Call #1 → GET https://httpbin.org/anything?source=https://httpbin.org/get
2025-11-09T21:07:28.514455Z  INFO whurl: Status: 200 (Http11)
2025-11-09T21:07:28.514576Z  INFO whurl: Response Body:
{
  "args": {
    "source": "https://httpbin.org/get"
  },
  "data": "",
  "files": {},
  "form": {},
  "headers": {
    "Accept": "*/*",
    "Host": "httpbin.org",
    "User-Agent": "hurl/7.0.0",
    "X-Amzn-Trace-Id": "<redacted>"
  },
  "json": null,
  "method": "GET",
  "origin": "<redacted>",
  "url": "https://httpbin.org/anything?source=https:%2F%2Fhttpbin.org%2Fget"
}
```

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
- `whurl list` — prints every API discovered under the requests root.
- `whurl list <api>` — lists the requests (file stems) available for that API. Reports when empty.

### run
Runs the selected request after all includes are expanded.
```
whurl run <API> <FILE> [OPTIONS]
```
- `--env NAME` — load `_vars/NAME.hurlvars` (or `<API>/NAME.hurlvars`).
- `--vars-file PATH` — merge variables from an arbitrary file.
- `--var KEY=VALUE` — inline variable overrides (repeatable, highest precedence).
- `--file-root PATH` — adjust the base directory for response/file assertions (relative values are resolved against the API directory; this does **not** change where Whurl discovers request files).
- `--json PATH` — emit the Hurl JSON report alongside console output.
- `--print-only-result` — suppress header/logs and stream the JSON report to stdout.
- `--silent` — suppress runtime header/log info (includes marked `[quiet]` / `[silent]` also hush logs).
- `--test` — print a concise summary with failure snippets after execution.
- `-v` / `-vv` — increase embedded Hurl verbosity (request/response debug logs).

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

## Logging & Reports
- Default runs print a header with API/request context plus info-level per-entry logs.
- Includes tagged `quiet` skip response body logging; `silent` suppresses logs entirely for that file.
- `--test` mode summarizes pass/fail counts and annotates failures with source file/line snippets.
- `--json PATH` writes the canonical Hurl JSON report; combine with `--print-only-result` for pipelines.
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
  whurl run httpbin send-json --file-root Z:\dev\projects\rust\rusted-toolbox\payloads
  ```
- Export only the execution result (no logs):
  ```bash
  whurl run httpbin basic --print-only-result > result.json
  ```

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
hurl = { git = "https://github.com/Orange-OpenSource/hurl", tag = "7.0.0" }
hurl_core = { git = "https://github.com/Orange-OpenSource/hurl", tag = "7.0.0" }
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
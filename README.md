# Rusted Toolbox 🦀
This is a collection of free command-line tools I made in an attempt to learn Rust.

We currently have the following tools:
1. A tool to read the [public info on JWT tokens](crates/tools/tool-jwt/readme.md);
2. A high-performance tool to [read messages from EventHub](crates/tools/tool-eventhub-read/readme.md),
3. and another is to [export the messages](crates/tools/tool-eventhub-export/readme.md);
4. A [CSV data normalizer](crates/tools/tool-csvn/readme.md) tool;
5. A tool that [splits large files](crates/tools/tool-csv-split/readme.md) (including CSV) into smaller ones;
6. A tool [that searches for multiple terms](crates/tools/tool-get-lines/readme.md) inside a text file and creates one output file per search term;
7. A tool that mimics the [cat](crates/tools/tool-cat/readme.md) command from Unix (useful on Windows);
8. A tool that mimics the [touch](crates/tools/tool-touch/readme.md) command from Unix (also useful on Windows);
9. A tool that [generates GUID](crates/tools/tool-guid/readme.md) (uuidv4) in the terminal with some nice options;
10. A tool that [converts unix timestamp](crates/tools/tool-timestamp/readme.md) to readable format and vice versa;
11. A lightweight [HTTP server](crates/tools/tool-http-server/readme.md) for serving static files during development;
12. A [mock data generator](crates/tools/tool-mock/readme.md) for creating test data with various types of realistic information;
13. A bare-bones, fully private, encrypted P2P chat tool called [Whisper](crates/tools/tool-whisper/readme.md);
14. A tool called [Gitignore](crates/tools/tool-gitignore/readme.md) that generates/updates the `.gitignore` file of your project automatically;
15. An image editor called [IMGx](crates/tools/tool-image/readme.md) that allows you to quickly do common operations like resizing, converting to another format, and to greyscale;
16. An [MQTT cli tool](crates/tools/tool-mqtt/readme.md) that can be used to quickly send or receive messages from a specific topic;
17. A tool to generate [QRCodes](crates/tools/tool-qrcode/readme.md) that, as the name suggests, can be used to generate QRCodes to file or just print them to the terminal;
18. A [lookup](crates/tools/tool-lookup/readme.md) tool that can either find text in multiple files or find files where the filename contains a specific text/pattern/regex;
19. A tool that is like ping, but with extra features. It is called [pingx](crates/tools/tool-pingx/readme.md);
20. A wrapper tool called [Whurl](crates/tools/tool-whurl/readme.md), that allows referencing one hurl file in another;
21. A drop-in replacement for base64 called [b64](crates/tools/tool-b64/readme.md) that comes with a few extra features;
22. A network quality monitor called [netquality](crates/tools/tool-netquality/readme.md) that checks connectivity and speed, and reports when things are not as expected;
23. A tool called [remove-zw](crates/tools/tool-remove-zw/readme.md) that removes zero-width Unicode format characters and a leading byte-order mark (BOM) from text, with a `--dry-run` preview and a `--check` mode for pipelines;
24. A tool that mimics the [head](crates/tools/tool-head/readme.md) command from Unix (also useful on Windows);
25. A tool that mimics the [tail](crates/tools/tool-tail/readme.md) command from Unix, including follow mode (also useful on Windows);
26. A regex value extractor called [rxget](crates/tools/tool-rxget/readme.md) that pulls matched values out of text files, with per-file or per-run uniqueness and optional filename prefixes;
27. A file encryption tool called [seal](crates/tools/tool-seal/readme.md) that seals files to `age1...` public keys and opens them with identity files (the age format, key-based only).
28. A desktop companion to seal called [seal-gui](crates/tools/tool-seal-gui/readme.md): the same key-based age encryption in a window, with text sealing to ASCII armor, drag-and-drop file batches, key generation, and a saved recipient book.

## Ok, but why?
Well, three main reasons:
- **1st**: I have a few tools and helpers made using Go, like my [Azure Eventhub Tools](https://github.com/brenordv/azure-eventhub-tools),
the [go Whisper](https://github.com/brenordv/go-whisper), [gitignore](https://github.com/brenordv/gitignore), and the [go help tools](https://github.com/brenordv/go-help).

While I love Golang, it had a few incidents where malicious software was sneakily added to legit packages:
1. https://thehackernews.com/2025/02/malicious-go-package-exploits-module.html
2. https://thehackernews.com/2025/03/seven-malicious-go-packages-found.html

So I decided to recreate them using Rust. I'm not abandoning Go or saying we shouldn't use it or anything like that. 
I still love Go, I'm using those incidents as an opportunity to improve my knowledge in another great programming 
language and centralizing my tools and helpers in one place.

- **2nd**: I use a couple of tools that are spread around a bunch of repositories, and that's a bit annoying to set up on new
machines. So I also ported the [JWT decoder tool](https://github.com/brenordv/python-snippets/tree/master/jwt_decoder_cli) that I created using Python, and the csv-split tool is an evolution of
a powershell script I wrote a long time ago in a blog post.

- **3rd**: Nice to have all the tools in a single repository, and being able to generate a cross-platform executable, which
helps a lot when you have to use Linux, MacOS, and Windows machines frequently.

## Use cases
The list above says what each tool is. This section is about what they are good for: the tricks that are not
obvious from the flag list, and what happens when you point the tools at each other. Unless a tool says
otherwise, stdout carries only data and diagnostics go to stderr, so chaining them works the way you'd hope.
(The EventHub pair has its own readmes and is skipped here.)

### Logs, CSVs, and other big files

- **rxget + get-lines**: `rxget` pulls every distinct ID out of a log, and `get-lines` takes a comma-separated
  term list and writes one output file per term. Chain them and a single pass over a huge log becomes one
  trace file per request:
  ```bash
  get-lines -f app.log -o traces -s "$(rxget -p 'request-id=([0-9a-f-]+)' -m unique-per-run app.log | paste -sd, -)"
  ```
- **csvn before a database import**: fill blanks with real defaults so NOT NULL columns stop rejecting the
  load, or use a sentinel (`--value-map "*=__MISSING__"`) that stays easy to count after the import. It also
  repairs ragged rows (short ones padded, long ones truncated), which is often the difference between a clean
  import and an error at line 180,412.
- **csv-split + tail**: in CSV mode every chunk gets the header, so each part is valid on its own: small
  enough for Excel, an email attachment, or an upload form with a size cap. To merge chunks back, keep the
  first file whole and strip the header from the rest with `tail -n +2`.
- **head's negative count**: everyone knows `head -n 20 export.csv` for a schema peek before writing import
  code. The sleeper feature is `head -n -1`, all but the last line, which drops the trailing summary row some
  exports append and parsers choke on.
- **tail -F on Windows**: follow a log through rotation during a deploy, on a platform where tail does not
  exist natively.
- **cat -A as an invisible-character detector**: when YAML looks right but will not parse, `cat -A` renders
  tabs as `^I`, line ends as `$`, and stray CRs as `^M`, so the invisible problem becomes visible. Follow up
  with `remove-zw` if the culprit is a zero-width character.
- **remove-zw as a CI gate**: text copied from web pages and chat tools can carry zero-width characters that
  compilers, linters, and reviewers never see. `remove-zw --check --recursive src` exits 1 when any file would
  change, so the pipeline rejects them before a human wastes an hour on an invisible bug. It also strips the
  UTF-8 BOM that breaks a shell script's shebang line.
- **lookup for bare machines**: content search (`lookup text "TODO" -e rs`) and filename search
  (`lookup files -s regex "^mydoc\.(pdf|epub|mobi)$"`) on boxes where grep and find are not available. Matches
  go to stdout and the summary to stderr, so piping stays clean.
- **b64 beyond plain encoding**: `--ignore-garbage` rescues base64 that got line-wrapped and
  whitespace-mangled in an email or ticket; `b64 --wrap 0 logo.png` emits a single line ready for a CSS data
  URI or a YAML field; and decoding a value out of `kubectl get secret -o yaml` is a one-liner instead of a
  Python detour.

### Moving files and secrets around

- **https + qrcode, the cable-free file transfer**: serve a folder with `https --host 0.0.0.0`, print the URL
  as a QR right in the terminal (`qrcode --text "http://192.168.1.50:4200"`), scan it with your phone, and
  download away. Each directory listing has a zip link, and checkboxes let you grab a hand-picked set of files
  as one archive.
- **https as an HTTP test fixture**: it implements Range requests, ETags, `If-Range`, and 304 revalidation
  properly, so resumable-download code, media-player seeking, and cache logic can be exercised against a real
  server without configuring nginx. Scripts can join in:
  `curl -o parts.zip "http://127.0.0.1:4200/project?download=zip&pick=src&pick=readme.md"`.
- **seal watch + the sync client you already run**: point the watch folder at a scanner or download inbox and
  the safe folder at whatever Dropbox, OneDrive, or Syncthing replicates. Every file is encrypted to your
  public key before the sync client sees it, and the watching machine never holds a secret key. That is
  end-to-end encrypted cloud backup assembled from parts you already have.
- **seal in a cron job**: `tar cz docs | seal encrypt -R team.txt -o docs.tar.gz.age` backs up to a file that
  several named people can open and nobody else. Since seal implements the age format, the archives also open
  with `age` or `rage`; no lock-in to this repo.
- **seal-gui for the colleague who will not touch a terminal**: they get drag-and-drop file batches, a
  recipient book, and a Text tab that turns a secret into ASCII armor ready to paste into chat; you stay on
  the CLI. Both speak the same format, so files cross between them freely.
- **whisper instead of pasting credentials into chat**: spin up a direct encrypted session between two
  machines (`whisper --wait` on one, `whisper --connect host:2428` on the other), exchange the secret, close
  it. Fresh keys per session, no server in the middle, no history written anywhere; compare the displayed
  fingerprints out loud and you have also ruled out a man in the middle.
- **qrcode for the guest network**: `qrcode --wifi-ssid Guest --wifi-password "..." -f svg -o guest-wifi`
  makes a printable vector QR; frame it by the door and stop dictating the password. The terminal output mode
  is also the fastest way to get a URL from an SSH session onto your phone.

### Test data and API work

- **mock + guid for fixtures**: loop them into CSVs or INSERT statements for seed data. The locale flag
  doubles as an encoding test: `mock person.full-name --locale ja-jp` produces names (and emails with
  non-ASCII local parts) that flush out UTF-8 bugs long before a real Japanese customer does.
- **guid's edge cases**: `guid --empty` prints the all-zeros GUID for testing the `Guid.Empty` code path
  everyone forgets, and `guid -m 500` fills a spreadsheet column in one paste.
- **jwt + ts**: `jwt` decodes a token (it strips the `Bearer ` prefix itself, so paste straight from devtools)
  and shows `iat`/`exp` as epoch values; hand those to `ts` for UTC and local time side by side. "Why did this
  token stop working" rarely survives the pair. For a pile of tokens, `jwt --print csv` builds a comparable
  claims table.
- **whurl for the auth wall**: write the login request once, `# @include auth/login` everywhere else, and the
  captured token flows into every request that follows. A `.dvars` line like
  `api_token=$shell(az keyvault secret show ...)` (behind an explicit env-var gate) fetches the credential at
  run start so it never sits in a file, and `--test --json report.json` turns the same collection into a
  post-deploy smoke test for CI.
- **mqtt + mock as a fake sensor fleet**:
  `mqtt post --host localhost --topic home/office/temp --message "$(mock random.float --min 18 --max 26 --precision 1)"`
  in a loop feeds a dashboard before the hardware exists. On the read side, payloads print one per line with
  logs on stderr, so `mqtt read --host broker --topic "home/+/temp" | jq .` works as-is.
- **touch -d for time travel**: code that acts on file age (cache eviction, "stale after 30 days" cleanup) is
  a pain to test until you manufacture old files: `touch -d "2024-01-15 10:30:00" cache.bin`. And
  `touch -r original.txt edited.txt` copies timestamps from a reference file, so a bulk edit does not make
  incremental build tools and backup dedupers reprocess the world.
- **gitignore --ai before the agents arrive**: a fresh checkout has no `.claude/` or `.cursor/` footprint yet,
  so the flag queues the agent-artifacts template preemptively and that state never lands in a commit. Plain
  `gitignore` re-run on a polyglot repo picks up newly added languages and merges with the existing file
  instead of clobbering it.
- **imgx as a metadata scrub**: EXIF orientation is baked into the pixels and the EXIF block (GPS coordinates
  included) is dropped, so a batch resize doubles as a privacy pass before photos leave your machine. It uses
  every core, so `imgx photos/ --resize 50 --convert webp` chews through a vacation folder quickly.

### Watching the network

- **pingx as evidence**: `pingx 192.168.1.1 -T -D -o csv -e 60 > overnight.csv` runs all night and leaves a
  timestamped, machine-readable record with a loss-stats line every minute. Import it into a spreadsheet and
  the "the WiFi drops every evening" argument settles itself. `--beep` gives audible packet-loss feedback
  while you walk around repositioning the router, and custom templates (`-o "%timestamp% %time%ms"`) emit
  exactly the fields a downstream script wants.
- **netquality as the long game**: the always-on version of the same argument. It keeps a year of
  connectivity checks and speed tests in SQLite, pings you on Telegram when speed degrades or an outage ends,
  and exports to OpenTelemetry for a Grafana dashboard. Run the Docker image on any always-on box and the next
  support call to your ISP comes with receipts.

## Demos
### Whisper
![Whisper Demo](https://github.com/brenordv/rusted-toolbox/raw/refs/heads/master/.demos/whisper-demo-0001.mp4)

### HTTP Server
![HTTP Server Demo](https://github.com/brenordv/rusted-toolbox/raw/refs/heads/master/.demos/http-server-demo-0001.mp4)

### QR Code
![QR Code Demo](https://github.com/brenordv/rusted-toolbox/raw/refs/heads/master/.demos/qrcode-demo-0001.mp4)

## Installation
### Building all tools locally
Considering you have Rust installed, you can build all tools by running:

**On Windows:**
```terminal
build.bat
```

**On Linux/MacOs:**
```bash
chmod +x ./build.sh
./build.sh
```
### Convenience Scripts
You can use the convenience scripts which will:
1. Install Rust if you don't already have it;
2. Clone the repo;
3. Build the tools;
4. Make the tools available globally for the current user;

Running this script again will update the tools (but not Rust).

#### Convenience build for Ubuntu
Convenience command:
```bash
curl -sSL https://raw.githubusercontent.com/brenordv/rusted-toolbox/refs/heads/master/convenience-build-ubuntu.sh | bash
```

#### Convenience build for MacOs
Convenience command:
```bash
curl -sSL https://raw.githubusercontent.com/brenordv/rusted-toolbox/refs/heads/master/convenience-build-macos.sh | bash
```

## Contributing
By the time I'm writing this, we have about 8.2 billion people in the world. Being optimistic, this means that the 
chances of someone wanting to contribute (or maybe even use the tools here) are about `1:8,200,000,000` (that one 
person being me).

Even so, I've created the [contributing readme](CONTRIBUTING.md) so future-me can remember how to organize things
when I come back to this project after a while. 

## License
Everything under [GNU Public License V3](LICENSE.md). 

TL;DR:
1. Anyone can copy, modify, and distribute this software.
2. You have to include the license and copyright notice with every distribution.
3. You can use this software privately.
4. You can use this software for commercial purposes.
5. If you dare to build your business solely from this code, you risk open-sourcing the whole code base.
6. If you modify it, you have to indicate changes made to the code.
7. Any modifications of this code base MUST be distributed with the same license, GPLv3.
8. This software is provided without a warranty.
9. The software author or license cannot be held liable for any damage inflicted by the software.
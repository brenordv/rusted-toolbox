# seal-gui

The desktop companion to [seal](../tool-seal/readme.md): key-based file
encryption over the age format, in a window instead of a terminal.

This release is the shell: the themed window, the Text | Files | Keys tabs,
the status bar, and the background worker the encryption jobs will run on.
The tabs are placeholders; sealing, opening, and key management arrive in the
next release. What the shell already proves end to end: the dark theme from
`common-gui`, tab switching, the worker round trip (the status bar's "Send a
test toast" button pushes a job through the worker and renders the reply as a
toast), and toast expiry without busy-repainting.

![screenshot placeholder](screenshot.png)

## Logging

One deliberate deviation from the CLI tools: **file logging is on by
default**, because a windowed app has no visible stderr. Events at `warn` and
above land in `~/.seal-gui/logs/seal-gui.log` (hover the `logs` label in the
status bar for the exact folder on your machine). Job failures are logged as
errors, so "why did sealing fail last night" is answerable from that file.

The shared logging flags still apply:

- `--log-level <level>`: `trace` to `error`, or `disabled` to silence
  everything, the file sink included.
- `--log-to-console`: also log to stdout (useful when started from a shell).
- `--rotate-log-file-by-day`: date-stamped log files instead of one file.

## Jobs

One background worker runs one job at a time; buttons that would start a
second job are greyed out while one is in flight. If the worker ever dies,
the status bar says so and the app needs a restart; nothing hangs.

## Exit codes

| Code | Meaning                                    |
|------|--------------------------------------------|
| 0    | window closed normally                     |
| 1    | the window or the app could not be created |
| 2    | usage error (unknown flag)                 |

## Manual checklist (per release)

- Launch on each OS available: the dark theme renders, tabs switch.
- "Send a test toast" shows a toast that expires on its own.
- An idle window sits at zero CPU (the toast timer must not busy-repaint).
- The log file exists after a run and its folder shows on the `logs` tooltip.
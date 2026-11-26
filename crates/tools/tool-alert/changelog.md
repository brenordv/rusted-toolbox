# Changelog

## 1.1.0
- Log files go to `~/.rusted-toolbox/alert/logs` (they went to `~/.alert/logs`). The old
  folder is not moved, since another program may use that name; delete it if it holds only
  alert's logs.

## 1.0.2
- Both windows take their theme, the Body's line-break key, the window watch, and the failure
  log line from `common-gui-rune`, and logging boots through `common-cli`'s GUI boot, which
  also logs a panic (thread, location, message) before the default hook prints it. Nothing the
  windows do changed.

## 1.0.1
- The tester's single-line fields, its key checks, the appearance setter, and the GPU hint now
  come from `common-gui-rune`.
- In the tester's Body, Shift+Return types a line break, as Return does.

## 1.0.0
- Initial release.
- `alert <BODY>` shows a window above other windows with the message and one button, Close,
  and returns when the user closes it. `-t`/`--title` adds a title and `-s`/`--sound` plays a
  sound file (WAV, MP3, FLAC, Ogg Vorbis) when the window shows.
- A pointer press on Close works at once. Return, Space on Close, and Escape close only as a
  fresh press at least one second after the window appeared, so a window that takes focus
  while you type does not vanish on your next Return.
- A sound that cannot play never hides the alert: the window says why and the exit code is
  still 0. On Windows a sound on a network share is refused without touching the network.
- `-g`/`--gui` opens a tester with Title, Body, and Sound file fields and a Test button
  that shows the alert through the same command.
- Logs to a file at `info` by default (`~/.alert/logs`): when the alert showed and how it was
  closed.
- Ships an agent skill in `skill/SKILL.md`.

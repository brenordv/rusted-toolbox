# Changelog

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

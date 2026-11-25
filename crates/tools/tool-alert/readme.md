# alert

Shows a small window above your other windows with a message, and waits until you close it.
It exists so an agent can ping you when you asked it to ("alert me when the build is done"),
and it works the same from your own scripts.

```bash
alert -t "Build finished" "All 214 tests passed on the release branch."
```

The window has an optional title, the message, and one button, Close. There is nothing to
answer: closing it is the acknowledgement.

## Usage

```
alert [OPTIONS] <BODY>

  BODY             the message; required unless --gui
  -t, --title T    a title shown above the message
  -s, --sound F    a sound file to play when the alert shows (WAV, MP3, FLAC, Ogg Vorbis)
  -g, --gui        open the tester window, filled with any title, body, and sound given
```

plus the shared logging flags (`--log-level`, `--log-to-file`, `--log-to-console`,
`--rotate-log-file-by-day`, `--app-header`).

alert returns when you close the window. A caller that must not wait starts it in the
background:

```bash
alert "The export finished." &
```

A body that starts with `-` goes after `--`:

```bash
alert -- "-5 degrees outside, the sensor check passed."
```

stdout stays empty.

| Exit code | Meaning |
|---|---|
| 0 | the alert was shown and closed |
| 1 | a failure: a blank body, or the window could not be opened |
| 2 | a usage error (no body, an unknown flag) |

## The window

- It stays above other windows and does not resize. A long message scrolls inside it, and
  its text can be selected and copied.
- A pointer press on Close works at once. Return, Space on Close, and Escape close it too,
  but only as a fresh press at least one second after the window appeared. A window that
  takes focus while you are typing would otherwise vanish on your next Return; a held key
  never closes it.
- The window's title in the taskbar starts with "alert", and its footer reads "Sent with
  the alert command", so a message that imitates a system prompt still says where it came
  from. The window has no input of its own.
- Control characters and text-direction marks in the title and the message show as spaces.
- It follows the system's light or dark appearance.

## Sound

`--sound` plays a file once when the window shows. Closing the window stops it.

A sound that cannot play never hides the alert. The window opens, a line in it reads
"Couldn't play the sound:" with the reason, the log gets a warning, and the exit code is
still 0: you were reached, which is the tool's job.

On Windows, a sound path that starts with two slashes or backslashes (a network share) is
refused without touching the network. Opening such a path makes Windows offer your sign-in
to the host it names, and the path can come from an agent that was steered by something it
read. Copy the sound to a local folder to use it.

## The tester

`alert --gui` opens a window with Title, Body, and Sound file fields and a Test button.
Test shows the alert by starting `alert` again with the form's values as its arguments, so
it exercises the same command an agent would run. `Ctrl+Enter` (`Cmd+Enter` on macOS)
presses Test; Return in Body types a new line.

## Logs

alert writes its log to a file by default, at `info`:
`~/.rusted-toolbox/alert/logs/alert.log`. An alert started in the background has no
terminal anyone watches, so the file is where "shown"
(whether it had a title and a sound, and how long the message was) and "closed" (after how
long, and by the button, a key, or the window's own close control) land. The message
itself is never logged. The same lines go to stderr; `--log-level warn` keeps the info
lines out, and `--log-level disabled` turns logging off.

## The agent skill

`skill/SKILL.md` in this folder teaches an agent to use alert: only when you asked to be
pinged, the message in the body and a few words in the title, and in the background when
its work should go on. To install it, copy or link the `skill` folder into your agent's
skills folder under the name `alert`. For Claude Code:

```bash
mkdir -p ~/.claude/skills
cp -r crates/tools/tool-alert/skill ~/.claude/skills/alert
```

Then allow the `alert` command in the agent's permissions if you do not want a prompt each
time.

## Building on Linux

alert plays sound through ALSA, so building it needs the ALSA development files
(`sudo apt-get install -y libasound2-dev` on Debian and Ubuntu). Windows and macOS need
nothing extra.

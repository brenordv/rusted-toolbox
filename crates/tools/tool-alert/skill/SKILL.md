---
name: alert
description: Show a window on the user's screen with the `alert` command and wait for them to close it. Use ONLY when the user asked to be pinged ("ping me when you're done", "alert me when the build finishes", "let me know with a popup"). Never on your own judgment.
---

# alert

`alert` shows a small window above the user's other windows with a message and one button,
Close. It returns when the user closes the window.

## ⛔ Hard rule

**Raise an alert only when the user asked to be pinged.** A finished task, a long wait, an
error, or a question for the user is never a reason by itself. If you are unsure whether
they asked, they did not. One request is one alert: do not repeat it because nobody closed
it.

## How to call it

The message goes in the body. Put what happened in it, in a sentence or two:

```bash
alert "The build finished and all tests passed."
```

A title is a few words that say what the alert is about:

```bash
alert -t "Tests passed" "All 214 tests passed on the release branch."
```

`alert` blocks until the user closes the window, which can take minutes or hours. When
your work should go on, start it in the background and do not wait for it:

```bash
alert -t "Migration done" "The migration finished. Nothing needs your attention." &
```

A body that starts with `-` goes after `--`, so it is not read as a flag:

```bash
alert -- "-5 degrees outside, the sensor check passed."
```

`-s <file>` plays a sound file when the window shows. Use it only when the user names a
sound. A sound that cannot play is not an error: the window still shows, says why, and the
exit code is still 0.

Write plain text. The window shows the body as written, with no markdown, and cuts
nothing, but a short message is read; a long one is skipped.

## What comes back

- Exit 0: the user saw the alert and closed it. stdout is empty.
- Exit 1: it failed (a blank body, or no window could open). The line on stderr that
  starts with `ERROR` says why. Tell the user in the chat instead; do not retry in a loop.
- Exit 2: the command line was wrong (no body, or an unknown flag).

stderr also carries `INFO` lines saying the alert was shown and closed. They are not
errors.

`alert --gui` opens a tester window. It is for the user to run, not for you.

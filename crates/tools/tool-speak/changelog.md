# Changelog

## 1.2.0
- Settings has two fields where it had one URL field: Server (scheme, host, and port, such as
  `http://my-server.local:10157`; `http://` is assumed when there is no scheme) and Endpoint
  (the path on the server, `/v1/audio/speech` on a first run). Save joins them into the
  settings file's one `url`, and opening the window cuts that `url` back into the two fields.
  The file's shape and the command line are unchanged.
- A server that cannot be used (no host, another scheme, a user name or password, a `?` or a
  `#`) says why under Server. A first run with `--gui` puts the caret in Server.

## 1.1.0
- `-g`/`--gui` opens a window instead of speaking. The text argument, stdin, `--model`, and
  `--voice` are ignored with it.
- Compose: a text editor, one button per vocal reaction that types its tag at the caret (with
  a space before it when it follows a word), Generate, and Play. Play is enabled once audio
  was generated and restarts a sound that is still playing. Editing the text keeps the
  generated audio. `mod+enter` generates; Return types a line break.
- Settings: the URL, the request method, the model, the voice, and the reaction list (one tag
  per line, in square brackets), with Save. Save keeps payload fields the form does not show.
  Each field's issue shows under it and focus moves to the first one.
- A first run with `--gui` opens on Settings with the caret in URL; Generate stays disabled
  until a server is saved. A settings file that cannot be read exits 1 naming it, before any
  window opens.
- `mod+1` and `mod+2` switch between Compose and Settings, also while typing.
- With `--gui`, speak logs to a file at `info` by default (`~/.speak/logs`).
- A failure in the window is shown on one line, cut at 140 characters; the log holds the whole
  text.

## 1.0.0
- Initial release.
- `speak [TEXT]` sends the text to a text-to-speech server (`POST` or `PUT` with a JSON body)
  and plays the audio it answers with, returning when the audio ends. With no argument and a
  piped stdin, the text is read from stdin (up to 1 MB).
- Settings live in `~/.rusted-toolbox/tool-speak/config.json`. A first run in a terminal
  asks for the server address and port (10157 by default) and saves the file; a first run
  anywhere else exits 1 and says how to set the server.
- `--model` and `--voice` replace those two payload values for one run and save nothing. No
  flag sets, saves, or selects the server address.
- The request has a 10 s connect timeout and a 5 minute total timeout, follows no
  redirects, and refuses an answer over 100 MB. A server error is shown on one line, cut at
  140 characters, and never repeats the text that was sent.
- Ships an agent skill in `skill/SKILL.md`.

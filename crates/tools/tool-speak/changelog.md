# Changelog

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

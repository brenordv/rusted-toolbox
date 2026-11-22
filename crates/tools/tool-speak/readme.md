# speak

Sends text to a text-to-speech server and plays the audio it answers with. It exists so an
agent can say an explanation aloud when you ask for it, and it works the same from your own
terminal.

```bash
speak "Hello. [chuckle] The build is green."
```

speak talks to a server you run that takes the OpenAI-style speech request
(`POST /v1/audio/speech` with a JSON body of `model`, `voice`, and `input`) and answers with
audio. It was built against Chatterbox TTS Server. speak returns when the audio has
finished playing.

## Usage

```
speak [OPTIONS] [TEXT]

  TEXT             the text to speak; read from stdin when absent and stdin is piped
      --model M    model for this run, in place of the configured one (nothing is saved)
      --voice V    voice for this run, in place of the configured one (nothing is saved)
  -g, --gui        open the window (TEXT, stdin, --model, and --voice are ignored)
```

plus the shared logging flags (`--log-level`, `--log-to-file`, `--log-to-console`,
`--rotate-log-file-by-day`, `--app-header`).

The text comes from one of two places:

- the `TEXT` argument: `speak "Tests passed."`
- stdin, when there is no argument and stdin is piped. This is the safe way to pass long
  text or text with quotes in it, up to 1 MB of UTF-8:

  ```bash
  speak <<'EOF'
  The migration has three steps. First it adds the "status" column, then it backfills it
  in batches, and last it drops the old flag.
  EOF
  ```

Text that starts with `-` is read as a flag when passed as the argument; pipe it instead.

stdout stays empty. On failure one line on stderr says why.

| Exit code | Meaning |
|---|---|
| 0 | the audio played to its end |
| 1 | a failure: no text, no server set, the server refused or timed out, the answer was not audio, no sound device |
| 2 | a usage error (an unknown flag) |

## First run

speak keeps its settings in `~/.rusted-toolbox/tool-speak/config.json`. When that file does
not exist:

- in a terminal, speak asks two questions, saves the file, says where, and then speaks:

  ```
  Speech server address: my-server.local
  Port [10157]:
  Saved the speech server to <the settings file's full path>
  ```

  The address is a host name or IP address, with `http://` or `https://` in front when you
  want to say which (`http://` otherwise). A path, a query, a port, or a user name in the
  address is refused: the port has its own question, and an empty answer means 10157.

- anywhere else (an agent, a pipe, a redirected stderr), speak exits 1 and tells you to run
  it once in a terminal, or to run `speak --gui` and save the server in Settings. It never
  guesses a server.

- with `--gui`, the window opens on Settings with the caret in the URL field.

No command-line flag sets, saves, or selects the server address. That is on purpose: an
agent allowed to run `speak` cannot be talked into sending your text to another host.

## The window

`speak --gui` opens a window with two areas. `Ctrl+1` and `Ctrl+2` switch between them
(`Cmd` on macOS), also while you type.

Compose is for trying a text:

- Type the text. Return starts a new line.
- Each vocal reaction the settings list has a button. A press types the tag where the caret
  is, with a space before it when it follows a word, and leaves the caret after the tag.
- Generate (`Ctrl+Enter`) asks the server for the audio. Play is enabled once there is
  audio; pressing it again restarts the sound. Editing the text afterwards keeps the audio,
  and the next Generate replaces it.
- The line beside the buttons says what is going on: generating, ready, or why it failed. A
  long failure is cut at 140 characters; the log file has the whole text.

Settings is the settings file as a form: the URL, the request method, the model, the voice,
and the reactions, one tag in square brackets per line. Save writes the file and says so.
A field that cannot be saved says why under it. Payload fields the form does not show
(`speed`, `seed`) stay in the file as they are.

A settings file that speak cannot read stops `speak --gui` before the window opens, with an
error that names the file, so the form never overwrites something it could not load.

With `--gui`, speak logs to a file at `info` by default, under `~/.speak/logs`.

## The settings file

```json
{
  "url": "http://my-server.local:10157/v1/audio/speech",
  "request_method": "POST",
  "supported_vocal_reactions": ["[laugh]", "[chuckle]", "[sigh]", "[gasp]", "[cough]", "[clear throat]", "[sniff]", "[groan]", "[shush]"],
  "payload": {
    "model": "chatterbox-turbo",
    "voice": "sj-clip-001.wav"
  }
}
```

- `url`: the speech endpoint, `http` or `https`, with no user name or password in it.
- `request_method`: `POST` or `PUT`.
- `payload`: sent as the request body with `input` set to the text. Any other field your
  server takes (`speed`, `seed`, `language`, `response_format`) can be added here by hand
  and is sent as written. An `input` in the file is ignored.
- `supported_vocal_reactions`: the tags the model understands inline in the text. speak
  sends them as part of the text; the list is for people and for the agent skill.

Edit the file, or use Settings in `speak --gui`, to change the server, the default model, or
the default voice. A file speak cannot read is an error that names the file; speak never
overwrites it.

## Vocal reactions

With the default model, a tag in the text becomes a sound at that spot:

```bash
speak "Hey, please pay attention! [clear throat]"
```

## Privacy and the network

- With an `http://` URL the text crosses the network in the clear. Anyone on the path
  between this machine and the server can read it. An `https://` URL works the same way and
  encrypts it; use one when the server is not on a network you trust.
- speak does not follow redirects: a redirect would send the text to a host you did not
  choose.
- Logs name the server by host and port only. They never hold the text or the URL.

## Limits

- Connecting may take 10 seconds; the whole request, 5 minutes. The error says which limit
  was hit.
- An audio answer larger than 100 MB is refused.
- The answer must be WAV, MP3, FLAC, or Ogg Vorbis. Opus is not supported: leave
  `response_format` out of `payload`, or set it to `wav` or `mp3`.
- An error from the server is shown on one line, cut at 140 characters.

## The agent skill

`skill/SKILL.md` in this folder teaches an agent to use speak: only when you ask to hear
something aloud, short text as the argument, long text on stdin, and never to touch speak's
settings folder. To install it, copy or link the `skill` folder into your agent's skills
folder under the name `speak`. For Claude Code:

```bash
mkdir -p ~/.claude/skills
cp -r crates/tools/tool-speak/skill ~/.claude/skills/speak
```

Then allow the `speak` command in the agent's permissions if you do not want a prompt each
time.

## Building on Linux

speak plays sound through ALSA, so building it needs the ALSA development files
(`sudo apt-get install -y libasound2-dev` on Debian and Ubuntu). Windows and macOS need
nothing extra. The window needs a GPU with Vulkan on Linux, DirectX 12 on Windows, or Metal
on macOS.

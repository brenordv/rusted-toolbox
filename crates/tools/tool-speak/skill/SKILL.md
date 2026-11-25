---
name: speak
description: Say text aloud through the user's text-to-speech server with the `speak` command. Use ONLY when the user asks to hear something aloud ("say that aloud", "read this to me", "speak the summary"). Never on your own judgment.
---

# speak

`speak` sends text to the text-to-speech server the user set up and plays the audio on
their machine. It returns when the audio ends.

## ⛔ Hard rules

1. **Speak only when the user asks to hear something aloud.** A long answer, a finished
   task, or an error is never a reason by itself. If you are unsure whether they asked,
   they did not.
2. **Never create, edit, or delete anything under `~/.rusted-toolbox/speak/` or
   `~/.rusted-toolbox/tool-speak/`.** Those folders decide which server receives the text.
   Only the user changes them.

## How to call it

Short text with no quotes in it goes as the argument:

```bash
speak "The build finished and all tests passed."
```

Anything long, or holding quotes, backticks, `$`, or a leading `-`, goes on stdin, where
the shell cannot mangle it:

```bash
speak <<'EOF'
The migration has three steps. First it adds the "status" column, then it backfills it
in batches, and last it drops the old flag.
EOF
```

Stdin takes up to 1 MB of UTF-8 text. Write for the ear: plain sentences, no markdown,
no code blocks, no tables, no URLs read out letter by letter.

The default model understands these reaction tags inline, where the sound belongs:
`[laugh]`, `[chuckle]`, `[sigh]`, `[gasp]`, `[cough]`, `[clear throat]`, `[sniff]`,
`[groan]`, `[shush]`. Use one only when it fits; most text needs none.

```bash
speak "That was the last failing test. [chuckle] It was a typo."
```

`--voice <file>` and `--model <name>` change the voice or model for one run and save
nothing. Use them only when the user names one.

`speak --gui` opens a window for the user to try texts and change the settings. It is for
the user to run, never you: it speaks nothing by itself and waits until someone closes it.

## What comes back

- Exit 0: the audio played to its end. stdout is empty.
- Exit 1: it failed, with one line on stderr that says why. Tell the user what it says;
  do not retry in a loop.
- Exit 2: the command line was wrong (an unknown flag, or text starting with `-` passed
  as the argument: put it on stdin).

`speak` blocks until the audio ends, so a long text holds your turn for as long as it
takes to say.

## First run

When stderr says `speak has no speech server set yet`, stop. Tell the user speak needs its
server once, and give them the two ways the message lists: run `speak` once in their own
terminal and answer the two questions, or run `speak --gui` and save the server in
Settings. Do not try to set it up for them (hard rule 2).

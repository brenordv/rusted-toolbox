# common-audio

Decodes a sound and plays it on the default output device. A thin layer over
[rodio](https://docs.rs/rodio/0.23.0/rodio/), so the tools that play sound share one copy of
the decoding checks, the size cap, and the failure wording.

Two types:

- `Clip`: sound bytes a decoder recognized. `Clip::from_bytes(Vec<u8>)` refuses empty data
  and data no decoder recognizes; `Clip::from_file(&Path)` reads a file of at most 64 MB
  (`MAX_SOUND_FILE_BYTES`) and does the same. Cloning a clip shares its bytes.
- `Speaker`: the default output device. `Speaker::open()`, then `play(&clip)` starts the clip
  from its beginning and stops any sound still playing, and `wait()` blocks until the sound
  ends. Dropping the speaker stops playback.

```rust
let clip = Clip::from_file(Path::new("done.wav"))?;
let mut speaker = Speaker::open()?;
speaker.play(&clip)?;
speaker.wait();
```

## Formats

WAV, MP3, FLAC, and Ogg Vorbis. Nothing else is compiled in: Opus and AAC data is refused as
"not a sound this tool can play".

## Contract

- Errors are `anyhow` errors worded for the person at the keyboard, and the texts are public
  constants: `NO_SOUND_DATA`, `NOT_A_PLAYABLE_SOUND` (it names the four formats), and
  `NO_OUTPUT_DEVICE`. A file error names the path.
- The crate logs nothing; the caller decides what a failure is worth.
- A `Speaker` holds the device open. Open it when there is something to play, so a machine
  with no sound device can still do everything else.

## Building on Linux

rodio's output goes through ALSA, so the build needs the ALSA development files
(`libasound2-dev` on Debian and Ubuntu). Windows and macOS need nothing extra.

## Test support

The `test-support` feature adds `test_support::tone_wav()`, a valid 50 ms WAV built in
memory, so tests need no committed sound file. A consumer lists the crate again under
`[dev-dependencies]` with the feature on:

```toml
[dev-dependencies]
common-audio = { workspace = true, features = ["test-support"] }
```

The one test that opens the real device is `#[ignore]`d (CI runners have no sound card):
`cargo test -p common-audio -- --ignored` plays a short tone.

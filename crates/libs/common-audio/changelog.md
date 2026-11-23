# Changelog

## 1.0.0
- Initial release.
- `Clip`: sound bytes checked once against the decoders (WAV, MP3, FLAC, Ogg Vorbis), from
  memory or from a file of at most 64 MB.
- `Speaker`: the default output device; `play` starts a clip and stops the one before it,
  `wait` blocks until the sound ends.
- Feature `test-support`: `test_support::tone_wav`, a valid WAV built in memory.

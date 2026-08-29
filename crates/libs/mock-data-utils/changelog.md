# Changelog

## 1.1.0
- Invalid options now return an error instead of panicking on an empty random
  range: `min > max` for `random.integer` and `random.float`, and a `range` of
  zero for the date, datetime, and timestamp types.
- Fixed password generation to index the charset by character rather than by
  byte, binding the RNG once and collecting the charset once.
- `random.time` now honors `--past` and `--future` (previously the flags did
  nothing), sampling seconds of the day at or before, or at or after, the current
  time.
- Documented every `MockOptions` field and which data types honor it, and added a
  crate doc.
- Grew the test suite from 25 to 47: validation guards, a dispatch table over all
  32 types, a `from_command` round-trip, and the previously untested generators.

## 1.0.0
- Initial release: mock data generators for tool-mock.

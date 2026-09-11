# Changelog

## 1.1.2
- Validation now bounds the year-offset options and float precision, closing panic paths that
  were reachable through `generate_mock_data`: `person.birthday` with an `age` past chrono's
  representable years panicked on the fallback date, and `random.date`/`random.datetime`/
  `random.timestamp` with a large `range` panicked inside chrono's date arithmetic. `age` and
  `range` now reject values above 100,000 (`MAX_YEAR_OFFSET`), `precision` above 100
  (`MAX_PRECISION`), each with a clear error.
- `generate_birthday`'s fallback date is built without `expect`; an unrepresentable birth year
  reports an error instead of panicking (defense in depth behind the new validation).

## 1.1.1
- `commerce.product-description` no longer returns an empty string when
  `length` is smaller than one sentence: the first sentence is now cut at the
  last whole word inside the limit (mid-word for limits shorter than the first
  word), so any nonzero `length` yields a non-empty description. Zero still
  yields an empty string, now documented on `MockOptions::length`.
- Removed the dead post-loop truncation branch that the old sentence loop could
  never reach.
- Created the readme: generator catalog, option-to-generator table, and the
  password-is-mock-data positioning.
- Documented that the generator functions assume options pre-validated by
  `generate_mock_data`.
- Grouped the per-generator non-empty tests with rstest (personal, commerce)
  and dropped a password test subsumed by another; added boundary tests for the
  description length contract.

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

use std::ops::Range;

/// Outcome of interpreting a `Range` request header against a resource of a
/// known length.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RangeOutcome {
    /// No usable range: serve the whole resource with a 200.
    Full,
    /// A single satisfiable byte range: serve a 206 with this half-open span.
    Partial(Range<u64>),
    /// A syntactically valid range that selects nothing: answer 416 with
    /// `Content-Range: bytes */len`.
    Unsatisfiable,
}

/// Interprets a `Range` header value against a resource of `len` bytes, per
/// RFC 9110 §14. Exactly one `bytes=` spec is supported (the unit compared
/// case-insensitively per §14.1), in the forms `a-b` (inclusive), `a-`, and
/// `-suffix`. Everything else (other units, multiple ranges, non-numeric or
/// overflowing tokens, `a > b`) is ignored and the whole resource is served,
/// which the RFC permits a server to do. A spec that is well-formed but
/// selects no bytes (`start >= len`, a `-0` suffix) is unsatisfiable. An
/// inclusive end at or past the resource clamps to it.
pub fn parse_byte_range(header: Option<&str>, len: u64) -> RangeOutcome {
    let Some(header) = header else {
        return RangeOutcome::Full;
    };
    let header = header.trim();
    let Some(spec) = header
        .get(..6)
        .filter(|unit| unit.eq_ignore_ascii_case("bytes="))
        .and_then(|_| header.get(6..))
    else {
        return RangeOutcome::Full;
    };
    if spec.contains(',') {
        return RangeOutcome::Full;
    }
    let spec = spec.trim();

    let (start, end) = if let Some(suffix) = spec.strip_prefix('-') {
        match parse_digits(suffix) {
            Some(n) => (len.saturating_sub(n), len),
            None => return RangeOutcome::Full,
        }
    } else {
        let Some((first, last)) = spec.split_once('-') else {
            return RangeOutcome::Full;
        };
        let Some(start) = parse_digits(first) else {
            return RangeOutcome::Full;
        };
        let end = if last.is_empty() {
            len
        } else {
            let Some(last) = parse_digits(last) else {
                return RangeOutcome::Full;
            };
            if last < start {
                return RangeOutcome::Full;
            }
            // The inclusive end becomes a half-open one; u64::MAX has no
            // successor, but any end at or past the resource clamps anyway.
            last.saturating_add(1).min(len)
        };
        (start, end)
    };

    if start < end {
        RangeOutcome::Partial(start..end)
    } else {
        RangeOutcome::Unsatisfiable
    }
}

/// Parses a range token as bare ASCII digits. `u64::from_str` also accepts a
/// leading `+`, which the RFC grammar does not, so the digit check runs
/// first.
fn parse_digits(token: &str) -> Option<u64> {
    if token.is_empty() || !token.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    token.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_header_serves_full() {
        assert_eq!(parse_byte_range(None, 10), RangeOutcome::Full);
    }

    #[test]
    fn closed_range_is_inclusive() {
        assert_eq!(
            parse_byte_range(Some("bytes=2-5"), 10),
            RangeOutcome::Partial(2..6)
        );
    }

    #[test]
    fn open_range_runs_to_the_end() {
        assert_eq!(
            parse_byte_range(Some("bytes=3-"), 10),
            RangeOutcome::Partial(3..10)
        );
    }

    #[test]
    fn suffix_range_takes_the_final_bytes() {
        assert_eq!(
            parse_byte_range(Some("bytes=-4"), 10),
            RangeOutcome::Partial(6..10)
        );
    }

    #[test]
    fn suffix_longer_than_the_resource_clamps_to_the_whole_file() {
        assert_eq!(
            parse_byte_range(Some("bytes=-999"), 10),
            RangeOutcome::Partial(0..10)
        );
    }

    #[test]
    fn end_past_the_resource_clamps() {
        assert_eq!(
            parse_byte_range(Some("bytes=2-999"), 10),
            RangeOutcome::Partial(2..10)
        );
    }

    #[test]
    fn end_at_the_last_byte_covers_it() {
        assert_eq!(
            parse_byte_range(Some("bytes=0-9"), 10),
            RangeOutcome::Partial(0..10)
        );
    }

    #[test]
    fn range_unit_matches_case_insensitively() {
        assert_eq!(
            parse_byte_range(Some("Bytes=2-5"), 10),
            RangeOutcome::Partial(2..6)
        );
        assert_eq!(
            parse_byte_range(Some("BYTES=-4"), 10),
            RangeOutcome::Partial(6..10)
        );
    }

    #[test]
    fn multi_range_is_ignored() {
        assert_eq!(
            parse_byte_range(Some("bytes=0-1,3-4"), 10),
            RangeOutcome::Full
        );
    }

    #[test]
    fn other_units_are_ignored() {
        assert_eq!(parse_byte_range(Some("lines=0-1"), 10), RangeOutcome::Full);
    }

    #[test]
    fn malformed_specs_are_ignored() {
        assert_eq!(parse_byte_range(Some("bytes=abc-"), 10), RangeOutcome::Full);
        assert_eq!(parse_byte_range(Some("bytes=1-x"), 10), RangeOutcome::Full);
        assert_eq!(parse_byte_range(Some("bytes=5"), 10), RangeOutcome::Full);
        assert_eq!(parse_byte_range(Some("bytes=5-2"), 10), RangeOutcome::Full);
        // u64::from_str tolerates a leading +, but the RFC grammar is
        // digits-only.
        assert_eq!(parse_byte_range(Some("bytes=+2-5"), 10), RangeOutcome::Full);
        assert_eq!(parse_byte_range(Some("bytes=-+4"), 10), RangeOutcome::Full);
        // One digit past u64::MAX overflows the token parser.
        assert_eq!(
            parse_byte_range(Some("bytes=18446744073709551616-"), 10),
            RangeOutcome::Full
        );
    }

    #[test]
    fn start_past_the_resource_is_unsatisfiable() {
        assert_eq!(
            parse_byte_range(Some("bytes=10-"), 10),
            RangeOutcome::Unsatisfiable
        );
        assert_eq!(
            parse_byte_range(Some("bytes=18446744073709551615-"), 10),
            RangeOutcome::Unsatisfiable
        );
    }

    #[test]
    fn zero_suffix_is_unsatisfiable() {
        assert_eq!(
            parse_byte_range(Some("bytes=-0"), 10),
            RangeOutcome::Unsatisfiable
        );
    }

    #[test]
    fn max_inclusive_end_clamps_without_overflow() {
        assert_eq!(
            parse_byte_range(Some("bytes=0-18446744073709551615"), 10),
            RangeOutcome::Partial(0..10)
        );
    }

    #[test]
    fn any_range_on_an_empty_resource_is_unsatisfiable() {
        assert_eq!(
            parse_byte_range(Some("bytes=0-"), 0),
            RangeOutcome::Unsatisfiable
        );
        assert_eq!(parse_byte_range(None, 0), RangeOutcome::Full);
    }
}

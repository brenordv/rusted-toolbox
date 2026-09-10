use crate::models::{Count, CountPrefix};

/// Parses a head count: unsigned, or `-NUM` for "all but the last NUM".
///
/// # Errors
/// Fails on a leading `+` (head has no from-the-start form), a missing
/// number, or an unknown multiplier suffix. A value past `u64::MAX`
/// saturates rather than failing.
pub fn parse_head_count(raw: &str) -> Result<Count, String> {
    let (sign, value) = split_count(raw)?;
    match sign {
        Sign::Plus => Err(format!(
            "invalid count '{raw}': head does not accept a leading '+'"
        )),
        Sign::Minus => Ok(Count {
            value,
            prefix: CountPrefix::FromEnd,
        }),
        Sign::None => Ok(Count {
            value,
            prefix: CountPrefix::Plain,
        }),
    }
}

/// Parses a tail count: `+NUM` starts at NUM from the beginning; `-NUM` and
/// unsigned both count from the end (the POSIX spellings).
///
/// # Errors
/// Fails on a missing number or an unknown multiplier suffix. A value past
/// `u64::MAX` saturates rather than failing.
pub fn parse_tail_count(raw: &str) -> Result<Count, String> {
    let (sign, value) = split_count(raw)?;
    let prefix = match sign {
        Sign::Plus => CountPrefix::FromStart,
        Sign::Minus | Sign::None => CountPrefix::FromEnd,
    };
    Ok(Count { value, prefix })
}

enum Sign {
    None,
    Plus,
    Minus,
}

/// Splits a raw count into its sign, digits, and multiplier suffix.
fn split_count(raw: &str) -> Result<(Sign, u64), String> {
    let (sign, rest) = match raw.as_bytes().first() {
        Some(b'+') => (Sign::Plus, &raw[1..]),
        Some(b'-') => (Sign::Minus, &raw[1..]),
        _ => (Sign::None, raw),
    };

    let digits_end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    let (digits, suffix) = rest.split_at(digits_end);
    if digits.is_empty() {
        return Err(format!("invalid count '{raw}': expected a number"));
    }

    // The digits are guaranteed non-empty ASCII, so the only possible parse
    // failure is overflow, which saturates: GNU quietly clamps an oversized
    // count to its maximum, and a saturated count already means "everything".
    let value: u64 = digits.parse().unwrap_or(u64::MAX);

    if suffix.is_empty() {
        return Ok((sign, value));
    }

    let (base, power) = suffix_factor(suffix)
        .ok_or_else(|| format!("invalid count '{raw}': unknown suffix '{suffix}'"))?;
    Ok((sign, scale_saturating(value, base, power)))
}

/// Multiplies `value` by `base`, `power` times, saturating at `u64::MAX` the
/// way GNU clamps an overflowing count. Scaling the value step by step (not
/// the multiplier first) keeps `0Z` at 0 even though `1024^7` itself
/// overflows.
fn scale_saturating(value: u64, base: u64, power: u32) -> u64 {
    let mut scaled = value;
    for _ in 0..power {
        scaled = match scaled.checked_mul(base) {
            Some(next) => next,
            None => return u64::MAX,
        };
    }
    scaled
}

/// Maps a multiplier suffix to its base and exponent, following the grammar
/// of GNU's parser (the `bkKmMGTPEZYRQ0` set coreutils feeds xstrtol): a
/// scale letter, optionally modified by `B` (powers of 1000) or `iB` (powers
/// of 1024); a bare letter means powers of 1024. Only `k` and `m` are also
/// accepted in lowercase, exactly as in GNU.
fn suffix_factor(suffix: &str) -> Option<(u64, u32)> {
    if suffix == "b" {
        return Some((512, 1));
    }
    let mut chars = suffix.chars();
    let power = match chars.next()? {
        'k' | 'K' => 1,
        'm' | 'M' => 2,
        'G' => 3,
        'T' => 4,
        'P' => 5,
        'E' => 6,
        'Z' => 7,
        'Y' => 8,
        'R' => 9,
        'Q' => 10,
        _ => return None,
    };
    let base = match chars.as_str() {
        "" | "iB" => 1024,
        "B" => 1000,
        _ => return None,
    };
    Some((base, power))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("5", 5)]
    #[case("0", 0)]
    #[case("18446744073709551615", u64::MAX)]
    #[case("2b", 1024)]
    #[case("2k", 2048)]
    #[case("2kB", 2000)]
    #[case("2kiB", 2048)]
    #[case("2K", 2048)]
    #[case("2KB", 2000)]
    #[case("2KiB", 2048)]
    #[case("2m", 2 * 1024 * 1024)]
    #[case("2mB", 2_000_000)]
    #[case("2miB", 2 * 1024 * 1024)]
    #[case("2MB", 2_000_000)]
    #[case("2M", 2 * 1024 * 1024)]
    #[case("2MiB", 2 * 1024 * 1024)]
    #[case("2GB", 2_000_000_000)]
    #[case("2G", 2 * 1024 * 1024 * 1024)]
    #[case("2TB", 2_000_000_000_000)]
    #[case("2TiB", 2u64 << 40)]
    #[case("2PB", 2_000_000_000_000_000)]
    #[case("2P", 2u64 << 50)]
    #[case("2EB", 2_000_000_000_000_000_000)]
    #[case("2E", 2u64 << 60)]
    fn accepts_plain_and_suffixed_values(#[case] raw: &str, #[case] expected: u64) {
        let count = parse_head_count(raw).unwrap();
        assert_eq!(count.value, expected);
        assert_eq!(count.prefix, CountPrefix::Plain);
    }

    #[rstest]
    #[case("")]
    #[case("-")]
    #[case("+")]
    #[case("abc")]
    #[case("5X")]
    #[case("5Ki")]
    #[case("5KIB")]
    #[case("5iB")]
    #[case("5g")]
    #[case("5t")]
    fn rejects_garbage(#[case] raw: &str) {
        assert!(parse_head_count(raw).is_err(), "{raw} should fail");
        if raw != "+" {
            assert!(parse_tail_count(raw).is_err(), "{raw} should fail for tail");
        }
    }

    #[rstest]
    #[case("18446744073709551616")]
    #[case("16E")]
    #[case("1Z")]
    #[case("1ZiB")]
    #[case("1YB")]
    #[case("1R")]
    #[case("1QB")]
    #[case("99999999999999999999K")]
    fn oversized_values_saturate_at_u64_max(#[case] raw: &str) {
        assert_eq!(parse_head_count(raw).unwrap().value, u64::MAX, "{raw}");
        assert_eq!(parse_tail_count(raw).unwrap().value, u64::MAX, "{raw}");
    }

    #[test]
    fn zero_scales_to_zero_even_when_the_multiplier_alone_overflows() {
        assert_eq!(parse_head_count("0Z").unwrap().value, 0);
        assert_eq!(parse_tail_count("0QiB").unwrap().value, 0);
    }

    #[test]
    fn head_rejects_plus_and_tail_accepts_it() {
        assert!(parse_head_count("+5").is_err());
        let count = parse_tail_count("+5").unwrap();
        assert_eq!(count.prefix, CountPrefix::FromStart);
        assert_eq!(count.value, 5);
    }

    #[test]
    fn signs_map_to_the_tool_specific_prefixes() {
        assert_eq!(parse_head_count("-2").unwrap().prefix, CountPrefix::FromEnd);
        assert_eq!(parse_head_count("2").unwrap().prefix, CountPrefix::Plain);
        assert_eq!(parse_tail_count("-5").unwrap().prefix, CountPrefix::FromEnd);
        assert_eq!(parse_tail_count("5").unwrap().prefix, CountPrefix::FromEnd);
        assert_eq!(parse_head_count("-2K").unwrap().value, 2048);
    }
}

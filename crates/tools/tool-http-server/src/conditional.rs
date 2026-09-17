use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Builds the strong entity tag for a file response from its metadata:
/// modification time (seconds and nanoseconds since the epoch) and length,
/// hex-encoded and wrapped in the DQUOTEs the ETag grammar requires. Being
/// metadata-based rather than a content hash, two files with the same
/// mtime and length share a tag; the nanosecond field keeps same-second
/// replacements apart on filesystems that record it. A pre-epoch
/// modification time yields no tag.
pub fn strong_etag(modified: SystemTime, len: u64) -> Option<String> {
    let since_epoch = modified.duration_since(UNIX_EPOCH).ok()?;
    Some(format!(
        "\"{:x}.{:x}-{:x}\"",
        since_epoch.as_secs(),
        since_epoch.subsec_nanos(),
        len
    ))
}

/// Decides whether a `Range` header may be honored given the request's
/// `If-Range` validator, per RFC 9110 §13.1.5. With no `If-Range` the range
/// is always honored. An entity-tag form must strongly match `etag` (a weak
/// `W/` tag never matches). A date form matches only when it equals the
/// resource's modification time truncated to whole seconds, and only when
/// that time is at least one second before `now`: §8.8.2.2 counts a
/// `Last-Modified` younger than that as weak, because the file could change
/// again within the same second without the date moving. Anything
/// unparseable mismatches, and a mismatch means the caller must ignore the
/// range and serve the whole resource, the safe outcome.
pub fn if_range_allows(
    if_range: Option<&str>,
    etag: Option<&str>,
    modified: Option<SystemTime>,
    now: SystemTime,
) -> bool {
    let Some(validator) = if_range else {
        return true;
    };
    let validator = validator.trim();

    if validator.starts_with("W/") {
        return false;
    }
    if validator.starts_with('"') {
        return etag == Some(validator);
    }

    let Some(modified) = modified else {
        return false;
    };
    let Ok(sent) = httpdate::parse_http_date(validator) else {
        return false;
    };
    let (Ok(modified_since_epoch), Ok(sent_since_epoch)) = (
        modified.duration_since(UNIX_EPOCH),
        sent.duration_since(UNIX_EPOCH),
    ) else {
        return false;
    };

    let modified_is_strong = now
        .duration_since(modified)
        .is_ok_and(|age| age >= Duration::from_secs(1));

    modified_is_strong && modified_since_epoch.as_secs() == sent_since_epoch.as_secs()
}

/// Decides whether a conditional GET/HEAD may be answered with 304 Not
/// Modified, applying RFC 9110 §13.2.2 precedence: a present `If-None-Match`
/// decides alone (`If-Modified-Since` is then ignored, §13.1.3), otherwise
/// `If-Modified-Since` decides, and with neither header the request is
/// unconditional. Callers only reach this for GET and HEAD; every other
/// method is rejected with a 405 before file serving begins.
pub fn not_modified(
    if_none_match: Option<&str>,
    if_modified_since: Option<&str>,
    etag: Option<&str>,
    modified: Option<SystemTime>,
) -> bool {
    if let Some(candidates) = if_none_match {
        return if_none_match_matches(candidates, etag);
    }
    if_modified_since.is_some_and(|since| unmodified_since(since, modified))
}

/// Evaluates an `If-None-Match` field against the stored strong entity tag
/// with the weak comparison RFC 9110 §13.1.2 mandates: a candidate's `W/`
/// prefix is ignored and the quoted opaque tags are compared byte-for-byte.
/// `*` counts only as the entire field value (per the §8.8.3 grammar) and
/// always matches here, because a representation exists whenever a file is
/// being served. This server's tags are hex digits, dots, and a dash, so no
/// matchable candidate contains a comma and splitting the list on commas
/// cannot split one; a client tag that does carry a comma splits into
/// fragments missing a DQUOTE, which only ever fail toward "no match" and a
/// full response.
fn if_none_match_matches(field: &str, etag: Option<&str>) -> bool {
    if field.trim_ascii() == "*" {
        return true;
    }
    let Some(etag) = etag else {
        return false;
    };
    field
        .split(',')
        .map(str::trim_ascii)
        .any(|candidate| candidate.strip_prefix("W/").unwrap_or(candidate) == etag)
}

/// Evaluates an `If-Modified-Since` date against the modification time,
/// truncated to whole seconds to mirror what `Last-Modified` advertised, so
/// a client echoing that header back revalidates successfully. An
/// unparseable date means the header is ignored (RFC 9110 §13.1.3), and a
/// missing or pre-epoch modification time counts as modified.
fn unmodified_since(since: &str, modified: Option<SystemTime>) -> bool {
    let Some(modified) = modified else {
        return false;
    };
    let Ok(sent) = httpdate::parse_http_date(since) else {
        return false;
    };
    let (Ok(modified_since_epoch), Ok(sent_since_epoch)) = (
        modified.duration_since(UNIX_EPOCH),
        sent.duration_since(UNIX_EPOCH),
    ) else {
        return false;
    };

    modified_since_epoch.as_secs() <= sent_since_epoch.as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A modification time comfortably in the past, with sub-second
    /// precision to exercise the truncation in date comparisons. The
    /// nanoseconds are a multiple of 100 so the value survives Windows'
    /// 100 ns `SystemTime` resolution intact.
    fn old_mtime() -> SystemTime {
        UNIX_EPOCH + Duration::new(1_700_000_000, 123_456_700)
    }

    fn now() -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_800_000_000)
    }

    #[test]
    fn strong_etag_is_quoted_hex_of_mtime_and_len() {
        let etag = strong_etag(old_mtime(), 0x2ab).unwrap();

        assert_eq!(etag, "\"6553f100.75bccbc-2ab\"");
    }

    #[test]
    fn strong_etag_for_pre_epoch_mtime_is_none() {
        let pre_epoch = UNIX_EPOCH - Duration::from_secs(1);

        assert!(strong_etag(pre_epoch, 10).is_none());
    }

    #[test]
    fn absent_if_range_always_allows() {
        assert!(if_range_allows(None, None, None, now()));
    }

    #[test]
    fn matching_strong_etag_allows() {
        let etag = strong_etag(old_mtime(), 10).unwrap();

        assert!(if_range_allows(
            Some(&etag),
            Some(&etag),
            Some(old_mtime()),
            now()
        ));
    }

    #[test]
    fn mismatched_etag_denies() {
        let etag = strong_etag(old_mtime(), 10).unwrap();

        assert!(!if_range_allows(
            Some("\"deadbeef.0-a\""),
            Some(&etag),
            Some(old_mtime()),
            now()
        ));
    }

    #[test]
    fn weak_etag_never_matches() {
        let etag = strong_etag(old_mtime(), 10).unwrap();
        let weak = format!("W/{etag}");

        assert!(!if_range_allows(
            Some(&weak),
            Some(&etag),
            Some(old_mtime()),
            now()
        ));
    }

    #[test]
    fn etag_form_without_a_stored_etag_denies() {
        assert!(!if_range_allows(
            Some("\"6553f100.0-a\""),
            None,
            Some(old_mtime()),
            now()
        ));
    }

    #[test]
    fn matching_date_for_an_old_mtime_allows() {
        let sent = httpdate::fmt_http_date(old_mtime());

        assert!(if_range_allows(Some(&sent), None, Some(old_mtime()), now()));
    }

    #[test]
    fn mismatched_date_denies() {
        let other = UNIX_EPOCH + Duration::from_secs(1_600_000_000);
        let sent = httpdate::fmt_http_date(other);

        assert!(!if_range_allows(
            Some(&sent),
            None,
            Some(old_mtime()),
            now()
        ));
    }

    #[test]
    fn matching_date_for_an_mtime_younger_than_a_second_denies() {
        let mtime = old_mtime();
        let sent = httpdate::fmt_http_date(mtime);
        let barely_later = mtime + Duration::from_millis(500);

        assert!(!if_range_allows(
            Some(&sent),
            None,
            Some(mtime),
            barely_later
        ));
    }

    #[test]
    fn date_form_without_an_mtime_denies() {
        let sent = httpdate::fmt_http_date(old_mtime());

        assert!(!if_range_allows(Some(&sent), None, None, now()));
    }

    #[test]
    fn garbage_validator_denies() {
        assert!(!if_range_allows(
            Some("not-a-validator"),
            Some("\"6553f100.0-a\""),
            Some(old_mtime()),
            now()
        ));
    }

    #[test]
    fn no_conditional_headers_means_modified() {
        let etag = strong_etag(old_mtime(), 10).unwrap();

        assert!(!not_modified(None, None, Some(&etag), Some(old_mtime())));
    }

    #[test]
    fn matching_if_none_match_is_not_modified() {
        let etag = strong_etag(old_mtime(), 10).unwrap();

        assert!(not_modified(
            Some(&etag),
            None,
            Some(&etag),
            Some(old_mtime())
        ));
    }

    #[test]
    fn weak_if_none_match_candidate_matches_the_strong_etag() {
        let etag = strong_etag(old_mtime(), 10).unwrap();
        let weak = format!("W/{etag}");

        assert!(not_modified(
            Some(&weak),
            None,
            Some(&etag),
            Some(old_mtime())
        ));
    }

    #[test]
    fn if_none_match_list_with_spaces_finds_the_match() {
        let etag = strong_etag(old_mtime(), 10).unwrap();
        let list = format!("\"other.0-a\", {etag} , W/\"third.0-b\"");

        assert!(not_modified(
            Some(&list),
            None,
            Some(&etag),
            Some(old_mtime())
        ));
    }

    #[test]
    fn mismatched_if_none_match_list_is_modified() {
        let etag = strong_etag(old_mtime(), 10).unwrap();

        assert!(!not_modified(
            Some("\"other.0-a\", \"third.0-b\""),
            None,
            Some(&etag),
            Some(old_mtime())
        ));
    }

    #[test]
    fn if_none_match_star_matches_as_the_whole_field() {
        assert!(not_modified(Some(" * "), None, None, None));
    }

    #[test]
    fn if_none_match_star_inside_a_list_is_not_the_star_form() {
        let etag = strong_etag(old_mtime(), 10).unwrap();

        assert!(!not_modified(
            Some("\"other.0-a\", *"),
            None,
            Some(&etag),
            Some(old_mtime())
        ));
    }

    #[test]
    fn if_none_match_without_a_stored_etag_is_modified() {
        assert!(!not_modified(
            Some("\"6553f100.0-a\""),
            None,
            None,
            Some(old_mtime())
        ));
    }

    #[test]
    fn if_modified_since_equal_to_the_mtime_second_is_not_modified() {
        let sent = httpdate::fmt_http_date(old_mtime());

        assert!(not_modified(None, Some(&sent), None, Some(old_mtime())));
    }

    #[test]
    fn if_modified_since_later_than_the_mtime_is_not_modified() {
        let later = old_mtime() + Duration::from_secs(3600);
        let sent = httpdate::fmt_http_date(later);

        assert!(not_modified(None, Some(&sent), None, Some(old_mtime())));
    }

    #[test]
    fn if_modified_since_earlier_than_the_mtime_is_modified() {
        let earlier = UNIX_EPOCH + Duration::from_secs(1_600_000_000);
        let sent = httpdate::fmt_http_date(earlier);

        assert!(!not_modified(None, Some(&sent), None, Some(old_mtime())));
    }

    #[test]
    fn unparseable_if_modified_since_is_ignored() {
        assert!(!not_modified(
            None,
            Some("not-a-date"),
            None,
            Some(old_mtime())
        ));
    }

    #[test]
    fn if_modified_since_without_an_mtime_is_modified() {
        let sent = httpdate::fmt_http_date(old_mtime());

        assert!(!not_modified(None, Some(&sent), None, None));
    }

    #[test]
    fn if_modified_since_with_a_pre_epoch_mtime_is_modified() {
        let sent = httpdate::fmt_http_date(old_mtime());
        let pre_epoch = UNIX_EPOCH - Duration::from_secs(1);

        assert!(!not_modified(None, Some(&sent), None, Some(pre_epoch)));
    }

    #[test]
    fn present_if_none_match_makes_if_modified_since_ignored() {
        let etag = strong_etag(old_mtime(), 10).unwrap();
        let matching_date = httpdate::fmt_http_date(old_mtime());

        assert!(!not_modified(
            Some("\"other.0-a\""),
            Some(&matching_date),
            Some(&etag),
            Some(old_mtime())
        ));
    }

    #[test]
    fn matching_if_none_match_wins_over_a_stale_if_modified_since() {
        let etag = strong_etag(old_mtime(), 10).unwrap();
        let earlier = UNIX_EPOCH + Duration::from_secs(1_600_000_000);
        let stale_date = httpdate::fmt_http_date(earlier);

        assert!(not_modified(
            Some(&etag),
            Some(&stale_date),
            Some(&etag),
            Some(old_mtime())
        ));
    }
}

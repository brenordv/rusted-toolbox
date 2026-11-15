//! Capture filters: time windows over the broker's enqueue time and text
//! matching over the message body.

use crate::models::CapturedMessage;
use anyhow::Context;
use chrono::{DateTime, Duration, NaiveDate, NaiveTime, SubsecRound, Timelike, Utc};
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

/// A condition on a message's broker-assigned enqueue time.
///
/// Every variant fails for a message without an enqueue time: guessing a
/// time would silently include or drop messages.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TimeFilter {
    OlderThan(DateTime<Utc>),
    OlderOrEqual(DateTime<Utc>),
    NewerThan(DateTime<Utc>),
    NewerOrEqual(DateTime<Utc>),
    /// The whole second, minute, or hour named by the timestamp; see
    /// [`during_window`] for how the granularity is derived.
    During(DateTime<Utc>),
    /// Inclusive of both `start` and `end`.
    Between {
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    },
}

/// A condition on a message's body.
///
/// Serializes as the pattern text (`{"Regex": "..."}`) and deserializes
/// through [`TextFilter::regex`], so an invalid pattern in a saved filter is
/// a load error rather than a runtime failure. Equality compares the pattern
/// text, because compiled regexes have no equality of their own.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "TextFilterRepr", into = "TextFilterRepr")]
pub enum TextFilter {
    /// Matches when the body contains any of the strings, ignoring case. An
    /// empty list matches nothing.
    Contains(Vec<String>),
    /// Matches when the compiled regex finds a match in the body. Build it with
    /// [`TextFilter::regex`] to get the case-insensitive default.
    Regex(Regex),
}

impl TextFilter {
    /// Compiles `pattern` into a [`TextFilter::Regex`] that matches case
    /// insensitively; a pattern can opt back into case sensitivity with
    /// `(?-i)`. The regex engine guarantees linear-time matching, so any
    /// user-supplied pattern is safe to run against every message.
    ///
    /// # Errors
    /// Fails when `pattern` is not a valid regex or compiles past the regex
    /// crate's size limit.
    pub fn regex(pattern: &str) -> anyhow::Result<Self> {
        let regex = RegexBuilder::new(pattern)
            .case_insensitive(true)
            .build()
            .context("Invalid text filter regex")?;

        Ok(Self::Regex(regex))
    }

    /// Returns `true` when `text` satisfies this filter.
    pub fn is_match(&self, text: &str) -> bool {
        match self {
            Self::Contains(needles) => contains_any_ci(text, needles),
            Self::Regex(regex) => regex.is_match(text),
        }
    }
}

impl PartialEq for TextFilter {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Contains(left), Self::Contains(right)) => left == right,
            (Self::Regex(left), Self::Regex(right)) => left.as_str() == right.as_str(),
            _ => false,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
enum TextFilterRepr {
    Contains(Vec<String>),
    Regex(String),
}

impl TryFrom<TextFilterRepr> for TextFilter {
    type Error = anyhow::Error;

    fn try_from(repr: TextFilterRepr) -> anyhow::Result<Self> {
        match repr {
            TextFilterRepr::Contains(needles) => Ok(Self::Contains(needles)),
            TextFilterRepr::Regex(pattern) => Self::regex(&pattern),
        }
    }
}

impl From<TextFilter> for TextFilterRepr {
    fn from(filter: TextFilter) -> Self {
        match filter {
            TextFilter::Contains(needles) => Self::Contains(needles),
            TextFilter::Regex(regex) => Self::Regex(regex.as_str().to_owned()),
        }
    }
}

/// The time and text conditions a captured message must meet to be shown.
/// An unset condition always passes; when both are set, both must pass.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CaptureFilters {
    pub time: Option<TimeFilter>,
    pub text: Option<TextFilter>,
}

impl CaptureFilters {
    /// Returns `true` when `msg` passes every configured condition.
    pub fn matches(&self, msg: &CapturedMessage) -> bool {
        self.matches_time(msg) && self.matches_text(msg)
    }

    /// Returns `true` when `msg` passes the time condition, or none is set.
    pub fn matches_time(&self, msg: &CapturedMessage) -> bool {
        let Some(filter) = &self.time else {
            return true;
        };
        let Some(enqueued) = msg.enqueued_time else {
            return false;
        };

        match *filter {
            TimeFilter::OlderThan(bound) => enqueued < bound,
            TimeFilter::OlderOrEqual(bound) => enqueued <= bound,
            TimeFilter::NewerThan(bound) => enqueued > bound,
            TimeFilter::NewerOrEqual(bound) => enqueued >= bound,
            TimeFilter::During(timestamp) => {
                let (start, end) = during_window(timestamp);
                enqueued >= start && enqueued < end
            }
            TimeFilter::Between { start, end } => enqueued >= start && enqueued <= end,
        }
    }

    /// Returns `true` when `msg` passes the text condition, or none is set.
    pub fn matches_text(&self, msg: &CapturedMessage) -> bool {
        self.text
            .as_ref()
            .is_none_or(|filter| filter.is_match(&msg.body))
    }

    /// The earliest enqueue time the time condition can accept, if it has a
    /// lower bound. For `NewerThan` the bound itself is excluded by
    /// [`matches`](Self::matches), not by this value.
    pub fn lower_time_bound(&self) -> Option<DateTime<Utc>> {
        match self.time.as_ref()? {
            TimeFilter::NewerThan(bound) | TimeFilter::NewerOrEqual(bound) => Some(*bound),
            TimeFilter::During(timestamp) => Some(during_window(*timestamp).0),
            TimeFilter::Between { start, .. } => Some(*start),
            TimeFilter::OlderThan(_) | TimeFilter::OlderOrEqual(_) => None,
        }
    }

    /// The latest enqueue time the time condition can accept, if it has an
    /// upper bound. For `During` this is the exclusive end of its window.
    pub fn upper_time_bound(&self) -> Option<DateTime<Utc>> {
        match self.time.as_ref()? {
            TimeFilter::OlderThan(bound) | TimeFilter::OlderOrEqual(bound) => Some(*bound),
            TimeFilter::During(timestamp) => Some(during_window(*timestamp).1),
            TimeFilter::Between { end, .. } => Some(*end),
            TimeFilter::NewerThan(_) | TimeFilter::NewerOrEqual(_) => None,
        }
    }
}

/// Returns the half-open window `[start, end)` a `During` filter covers.
///
/// The granularity comes from the finest non-zero component of `t`: a
/// non-zero second (or sub-second) selects that whole second, a zero second
/// with a non-zero minute selects that minute, and a zero minute and second
/// select that hour. The rule cannot express the minute `12:00` or the second
/// `12:34:00`; use `Between` for those.
pub fn during_window(t: DateTime<Utc>) -> (DateTime<Utc>, DateTime<Utc>) {
    let length = if t.second() != 0 || t.nanosecond() != 0 {
        Duration::seconds(1)
    } else if t.minute() != 0 {
        Duration::minutes(1)
    } else {
        Duration::hours(1)
    };
    let start = t.trunc_subsecs(0);
    let end = start
        .checked_add_signed(length)
        .unwrap_or(DateTime::<Utc>::MAX_UTC);

    (start, end)
}

/// Returns `true` when `haystack` contains at least one of `needles`,
/// ignoring case. An empty `needles` slice matches nothing.
pub fn contains_any_ci(haystack: &str, needles: &[String]) -> bool {
    let haystack = haystack.to_lowercase();

    needles
        .iter()
        .any(|needle| haystack.contains(&needle.to_lowercase()))
}

/// The timestamp shapes [`parse_time`] accepts, for error messages and help text.
pub const TIME_SHAPES: &str = "an RFC3339 timestamp such as 2026-07-02T12:34:56Z, or a date such \
                               as 2026-07-02 (midnight UTC)";

/// An inclusive enqueue-time window whose start is not after its end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimeWindow {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

impl TimeWindow {
    /// The window from `start` to `end`.
    ///
    /// # Errors
    /// Fails when `start` is after `end`.
    pub fn new(start: DateTime<Utc>, end: DateTime<Utc>) -> anyhow::Result<Self> {
        if start > end {
            anyhow::bail!("START is after END");
        }
        Ok(Self { start, end })
    }
}

impl From<TimeWindow> for TimeFilter {
    fn from(window: TimeWindow) -> Self {
        Self::Between {
            start: window.start,
            end: window.end,
        }
    }
}

/// Parses an RFC3339 timestamp, or a `YYYY-MM-DD` date as midnight UTC.
///
/// # Errors
/// Fails for any other shape, naming the accepted ones.
pub fn parse_time(value: &str) -> anyhow::Result<DateTime<Utc>> {
    match DateTime::parse_from_rfc3339(value) {
        Ok(time) => Ok(time.with_timezone(&Utc)),
        Err(_) => NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .map(|date| date.and_time(NaiveTime::MIN).and_utc())
            .map_err(|_| anyhow::anyhow!("expected {TIME_SHAPES}")),
    }
}

/// Parses the full RFC3339 timestamp a `During` filter needs: its granularity comes from the
/// timestamp (see [`during_window`]), so a date alone, which would select its first hour, is
/// refused.
///
/// # Errors
/// Fails for a date alone (suggesting the whole-day window) and for any non-RFC3339 value.
pub fn parse_time_with_granularity(value: &str) -> anyhow::Result<DateTime<Utc>> {
    match DateTime::parse_from_rfc3339(value) {
        Ok(time) => Ok(time.with_timezone(&Utc)),
        Err(_) if NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok() => anyhow::bail!(
            "a date alone would select its first hour only; for the whole day use a window \
             from {value}T00:00:00Z to {value}T23:59:59.999Z"
        ),
        Err(_) => anyhow::bail!(
            "expected an RFC3339 timestamp such as 2026-07-02T12:34:00Z (that minute)"
        ),
    }
}

/// Parses `START..END`, two [`parse_time`] values with `START` not after `END`.
///
/// # Errors
/// Fails without the `..` separator, when either side does not parse, or when `START` is
/// after `END`.
pub fn parse_window(value: &str) -> anyhow::Result<TimeWindow> {
    let Some((start, end)) = value.split_once("..") else {
        anyhow::bail!("expected START..END, each {TIME_SHAPES}");
    };
    let start = parse_time(start).map_err(|error| anyhow::anyhow!("START: {error}"))?;
    let end = parse_time(end).map_err(|error| anyhow::anyhow!("END: {error}"))?;
    TimeWindow::new(start, end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{BodyKind, MessageKey};
    use chrono::TimeZone;
    use rstest::rstest;
    use std::collections::BTreeMap;

    fn at(hour: u32, minute: u32, second: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 2, hour, minute, second)
            .unwrap()
    }

    fn message(enqueued_time: Option<DateTime<Utc>>, body: &str) -> CapturedMessage {
        CapturedMessage {
            key: MessageKey {
                partition_id: "0".to_owned(),
                sequence_number: 1,
            },
            offset: None,
            partition_key: None,
            enqueued_time,
            processed_at: Utc::now(),
            message_id: None,
            correlation_id: None,
            content_type: None,
            properties: BTreeMap::new(),
            body: body.to_owned(),
            body_kind: BodyKind::Utf8,
        }
    }

    fn time_only(filter: TimeFilter) -> CaptureFilters {
        CaptureFilters {
            time: Some(filter),
            text: None,
        }
    }

    #[rstest]
    #[case::minute(at(12, 34, 0), at(12, 34, 0), at(12, 35, 0))]
    #[case::hour(at(12, 0, 0), at(12, 0, 0), at(13, 0, 0))]
    #[case::second(at(12, 34, 56), at(12, 34, 56), at(12, 34, 57))]
    #[case::midnight_is_an_hour(at(0, 0, 0), at(0, 0, 0), at(1, 0, 0))]
    #[case::minute_at_day_end(
        at(23, 59, 0),
        at(23, 59, 0),
        Utc.with_ymd_and_hms(2026, 7, 3, 0, 0, 0).unwrap()
    )]
    #[case::subsecond_is_its_second(
        at(12, 34, 0) + Duration::milliseconds(250),
        at(12, 34, 0),
        at(12, 34, 1)
    )]
    fn during_window_derives_granularity(
        #[case] t: DateTime<Utc>,
        #[case] start: DateTime<Utc>,
        #[case] end: DateTime<Utc>,
    ) {
        assert_eq!(during_window(t), (start, end));
    }

    #[test]
    fn during_window_saturates_at_the_end_of_representable_time() {
        let last_hour = DateTime::<Utc>::MAX_UTC
            .with_minute(0)
            .unwrap()
            .with_second(0)
            .unwrap()
            .with_nanosecond(0)
            .unwrap();

        let (start, end) = during_window(last_hour);

        assert_eq!(start, last_hour);
        assert_eq!(end, DateTime::<Utc>::MAX_UTC);
    }

    #[rstest]
    #[case::older_than_below(TimeFilter::OlderThan(at(12, 0, 0)), at(11, 59, 59), true)]
    #[case::older_than_at(TimeFilter::OlderThan(at(12, 0, 0)), at(12, 0, 0), false)]
    #[case::older_or_equal_at(TimeFilter::OlderOrEqual(at(12, 0, 0)), at(12, 0, 0), true)]
    #[case::older_or_equal_above(TimeFilter::OlderOrEqual(at(12, 0, 0)), at(12, 0, 1), false)]
    #[case::newer_than_at(TimeFilter::NewerThan(at(12, 0, 0)), at(12, 0, 0), false)]
    #[case::newer_than_above(TimeFilter::NewerThan(at(12, 0, 0)), at(12, 0, 1), true)]
    #[case::newer_or_equal_at(TimeFilter::NewerOrEqual(at(12, 0, 0)), at(12, 0, 0), true)]
    #[case::newer_or_equal_below(TimeFilter::NewerOrEqual(at(12, 0, 0)), at(11, 59, 59), false)]
    #[case::during_start(TimeFilter::During(at(12, 34, 0)), at(12, 34, 0), true)]
    #[case::during_last_second(TimeFilter::During(at(12, 34, 0)), at(12, 34, 59), true)]
    #[case::during_end_excluded(TimeFilter::During(at(12, 34, 0)), at(12, 35, 0), false)]
    #[case::during_before(TimeFilter::During(at(12, 34, 0)), at(12, 33, 59), false)]
    #[case::between_start(
        TimeFilter::Between { start: at(12, 0, 0), end: at(13, 0, 0) },
        at(12, 0, 0),
        true
    )]
    #[case::between_end_included(
        TimeFilter::Between { start: at(12, 0, 0), end: at(13, 0, 0) },
        at(13, 0, 0),
        true
    )]
    #[case::between_after(
        TimeFilter::Between { start: at(12, 0, 0), end: at(13, 0, 0) },
        at(13, 0, 1),
        false
    )]
    #[case::between_before(
        TimeFilter::Between { start: at(12, 0, 0), end: at(13, 0, 0) },
        at(11, 59, 59),
        false
    )]
    fn time_filters_pin_their_boundaries(
        #[case] filter: TimeFilter,
        #[case] enqueued: DateTime<Utc>,
        #[case] expected: bool,
    ) {
        let filters = time_only(filter);

        assert_eq!(filters.matches(&message(Some(enqueued), "")), expected);
    }

    #[rstest]
    #[case::older_than(TimeFilter::OlderThan(at(12, 0, 0)))]
    #[case::newer_than(TimeFilter::NewerThan(at(12, 0, 0)))]
    #[case::during(TimeFilter::During(at(12, 34, 0)))]
    #[case::between(TimeFilter::Between { start: at(12, 0, 0), end: at(13, 0, 0) })]
    fn every_time_filter_rejects_a_message_without_enqueued_time(#[case] filter: TimeFilter) {
        assert!(!time_only(filter).matches(&message(None, "")));
    }

    #[test]
    fn no_filters_match_everything() {
        assert!(CaptureFilters::default().matches(&message(None, "anything")));
    }

    #[rstest]
    #[case("Hello World", vec!["hello"], true)]
    #[case("Hello World", vec!["HELLO"], true)]
    #[case("Hello World", vec!["HeLLo"], true)]
    #[case("Hello World", vec!["world"], true)]
    #[case("Hello World", vec!["ell"], true)]
    #[case("Hello World", vec!["He"], true)]
    #[case("Hello World", vec!["xyz"], false)]
    #[case("Hello World", vec!["WORLD!"], false)]
    #[case("Hello World", vec!["Hello World Extra"], false)]
    #[case("", vec!["hello"], false)]
    #[case("Hello World", vec!["xyz", "hello"], true)]
    #[case("Hello World", vec!["hello", "world"], true)]
    #[case("Hello World", vec!["HeLLo", "WoRLD"], true)]
    #[case("Hello World", vec!["abc", "def", "ghi"], false)]
    #[case("Test message", vec!["test", "message", "extra"], true)]
    #[case("", vec![], false)]
    #[case("Hello World", vec![], false)]
    #[case("", vec![""], true)]
    #[case("Hello", vec![""], true)]
    #[case("Café München", vec!["café"], true)]
    #[case("Hello 世界", vec!["世界"], true)]
    #[case("Emoji test 🚀", vec!["🚀"], true)]
    #[case("Mixed café 世界 🚀", vec!["CAFÉ"], true)]
    #[case("Line1\nLine2\nLine3", vec!["line2"], true)]
    #[case("Tab\tseparated\tvalues", vec!["separated"], true)]
    #[case("Spaces   everywhere", vec!["   "], true)]
    #[case("Special!@#$%^&*()chars", vec!["!@#"], true)]
    #[case("JSON: {\"key\": \"value\"}", vec!["json"], true)]
    #[case("XML: <root><child>value</child></root>", vec!["<root>"], true)]
    #[case("CSV: name,age,city", vec!["name,age"], true)]
    #[case("Error: Connection timeout", vec!["error"], true)]
    #[case("INFO: Process completed successfully", vec!["info"], true)]
    fn contains_any_ci_keeps_message_matches_filter_parity(
        #[case] haystack: &str,
        #[case] needles: Vec<&str>,
        #[case] expected: bool,
    ) {
        let needles: Vec<String> = needles.into_iter().map(String::from).collect();

        assert_eq!(contains_any_ci(haystack, &needles), expected);
    }

    #[test]
    fn contains_filter_matches_through_capture_filters() {
        let filters = CaptureFilters {
            time: None,
            text: Some(TextFilter::Contains(vec!["bacon".to_owned()])),
        };

        assert!(filters.matches(&message(None, "Crispy BACON strips")));
        assert!(!filters.matches(&message(None, "eggs")));
    }

    #[test]
    fn regex_filter_ignores_case_by_default() {
        let filter = TextFilter::regex("order-\\d+").unwrap();

        assert!(filter.is_match("ORDER-42 shipped"));
    }

    #[test]
    fn regex_filter_honours_an_inline_case_sensitive_flag() {
        let filter = TextFilter::regex("(?-i)order").unwrap();

        assert!(!filter.is_match("ORDER"));
    }

    #[test]
    fn regex_filter_rejects_an_invalid_pattern() {
        let error = TextFilter::regex("(unclosed").unwrap_err();

        assert!(
            error.to_string().contains("Invalid text filter regex"),
            "{error:#}"
        );
    }

    #[test]
    fn text_filter_equality_compares_regex_patterns() {
        assert_eq!(
            TextFilter::regex("a+").unwrap(),
            TextFilter::regex("a+").unwrap()
        );
        assert_ne!(
            TextFilter::regex("a+").unwrap(),
            TextFilter::regex("b+").unwrap()
        );
        assert_ne!(
            TextFilter::regex("a").unwrap(),
            TextFilter::Contains(vec!["a".to_owned()])
        );
    }

    #[test]
    fn text_filter_serde_round_trips_the_pattern_string() {
        let filter = TextFilter::regex("order-\\d+").unwrap();

        let json = serde_json::to_string(&filter).unwrap();
        let restored: TextFilter = serde_json::from_str(&json).unwrap();

        assert_eq!(json, r#"{"Regex":"order-\\d+"}"#);
        assert_eq!(restored, filter);
        assert!(restored.is_match("ORDER-7"));
    }

    #[test]
    fn text_filter_deserialization_rejects_an_invalid_pattern() {
        let result = serde_json::from_str::<TextFilter>(r#"{"Regex":"(unclosed"}"#);

        assert!(result.is_err());
    }

    #[test]
    fn time_and_text_must_both_pass() {
        let filters = CaptureFilters {
            time: Some(TimeFilter::NewerOrEqual(at(12, 0, 0))),
            text: Some(TextFilter::Contains(vec!["match".to_owned()])),
        };

        assert!(filters.matches(&message(Some(at(12, 0, 0)), "a match")));
        assert!(!filters.matches(&message(Some(at(11, 0, 0)), "a match")));
        assert!(!filters.matches(&message(Some(at(12, 0, 0)), "nothing")));
    }

    #[rstest]
    #[case::newer_than(TimeFilter::NewerThan(at(12, 0, 0)), Some(at(12, 0, 0)), None)]
    #[case::older_than(TimeFilter::OlderThan(at(12, 0, 0)), None, Some(at(12, 0, 0)))]
    #[case::during(
        TimeFilter::During(at(12, 34, 0)),
        Some(at(12, 34, 0)),
        Some(at(12, 35, 0))
    )]
    #[case::between(
        TimeFilter::Between { start: at(12, 0, 0), end: at(13, 0, 0) },
        Some(at(12, 0, 0)),
        Some(at(13, 0, 0))
    )]
    fn time_bounds_follow_the_filter(
        #[case] filter: TimeFilter,
        #[case] lower: Option<DateTime<Utc>>,
        #[case] upper: Option<DateTime<Utc>>,
    ) {
        let filters = time_only(filter);

        assert_eq!(filters.lower_time_bound(), lower);
        assert_eq!(filters.upper_time_bound(), upper);
    }

    #[rstest]
    #[case::utc("2026-07-02T12:00:00Z", at(12, 0, 0))]
    #[case::offset("2026-07-02T14:00:00+02:00", at(12, 0, 0))]
    #[case::date_only("2026-07-02", at(0, 0, 0))]
    fn parse_time_accepts_rfc3339_and_dates(#[case] value: &str, #[case] expected: DateTime<Utc>) {
        assert_eq!(parse_time(value).unwrap(), expected);
    }

    #[rstest]
    #[case::garbage("yesterday")]
    #[case::no_zone("2026-07-02T12:00:00")]
    fn parse_time_rejects_other_shapes_naming_the_accepted_ones(#[case] value: &str) {
        let error = format!("{:#}", parse_time(value).unwrap_err());

        assert!(error.contains("RFC3339"), "{error}");
    }

    #[test]
    fn parse_time_with_granularity_rejects_a_date_and_suggests_the_whole_day() {
        let error = format!(
            "{:#}",
            parse_time_with_granularity("2026-07-02").unwrap_err()
        );

        assert!(
            error.contains("from 2026-07-02T00:00:00Z to 2026-07-02T23:59:59.999Z"),
            "{error}"
        );
    }

    #[test]
    fn parse_time_with_granularity_accepts_a_timestamp() {
        assert_eq!(
            parse_time_with_granularity("2026-07-02T12:00:00Z").unwrap(),
            at(12, 0, 0)
        );
    }

    #[test]
    fn parse_time_with_granularity_rejects_garbage() {
        assert!(parse_time_with_granularity("noon").is_err());
    }

    #[test]
    fn parse_window_splits_start_and_end() {
        let window = parse_window("2026-07-02..2026-07-02T01:00:00Z").unwrap();

        assert_eq!(
            window,
            TimeWindow {
                start: at(0, 0, 0),
                end: at(1, 0, 0)
            }
        );
    }

    #[rstest]
    #[case::no_separator("2026-07-02T01:00:00Z", "START..END")]
    #[case::bad_start("x..2026-07-02T01:00:00Z", "START:")]
    #[case::bad_end("2026-07-02T01:00:00Z..x", "END:")]
    #[case::reversed("2026-07-02T02:00:00Z..2026-07-02T01:00:00Z", "START is after END")]
    fn parse_window_rejects_bad_windows(#[case] value: &str, #[case] expected: &str) {
        let error = format!("{:#}", parse_window(value).unwrap_err());

        assert!(error.contains(expected), "{error}");
    }

    #[test]
    fn a_time_window_becomes_an_inclusive_between_filter() {
        let window = TimeWindow::new(at(1, 0, 0), at(2, 0, 0)).unwrap();

        assert_eq!(
            TimeFilter::from(window),
            TimeFilter::Between {
                start: at(1, 0, 0),
                end: at(2, 0, 0)
            }
        );
    }
}

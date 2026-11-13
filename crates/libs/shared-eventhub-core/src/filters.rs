use chrono::{DateTime, Duration, Timelike, Utc};
use serde::{Deserialize, Serialize};
use crate::models::CapturedMessage;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TimeFilter {
    OlderThan(DateTime<Utc>),
    OlderOrEqual(DateTime<Utc>),
    NewerThan(DateTime<Utc>),
    NewerOrEqual(DateTime<Utc>),
    During(DateTime<Utc>),                       // granularity derived, see below
    Between { start: DateTime<Utc>, end: DateTime<Utc> },   // inclusive both ends
}

#[derive(Clone, Debug)]
pub enum TextFilter {
    Contains(Vec<String>),       // case-insensitive substring, any-of (old dump_filter parity)
    Regex(regex::Regex),         // compiled at construction; see notes below
}

#[derive(Clone, Debug, Default)]
pub struct CaptureFilters {
    pub time: Option<TimeFilter>,
    pub text: Option<TextFilter>,
}

impl CaptureFilters {
    pub fn matches(&self, msg: &CapturedMessage) -> bool {
        // A message must pass the time filter, if configured.
        let time_matches = match (&self.time, &msg.enqueued_time) {
            (None, _) => true,

            // Missing enqueued time fails every active time filter.
            (Some(_), None) => false,

            (Some(TimeFilter::OlderThan(bound)), Some(t)) => {
                t < bound
            }
            (Some(TimeFilter::OlderOrEqual(bound)), Some(t)) => {
                t <= bound
            }
            (Some(TimeFilter::NewerThan(bound)), Some(t)) => {
                t > bound
            }
            (Some(TimeFilter::NewerOrEqual(bound)), Some(t)) => {
                t >= bound
            }
            (Some(TimeFilter::During(timestamp)), Some(t)) => {
                let (start, end) = during_window(timestamp.clone());
                t >= &start && t < &end
            }
            (Some(TimeFilter::Between { start, end }), Some(t)) => {
                t >= start && t <= end
            }
        };

        if !time_matches {
            return false;
        }

        // A message must also pass the text filter, if configured.
        match &self.text {
            None => true,

            Some(TextFilter::Contains(needles)) => {
                contains_any_ci(&msg.body, needles)
            }

            Some(TextFilter::Regex(regex)) => {
                regex.is_match(&msg.body)
            }
        }
    }

    pub fn lower_time_bound(&self) -> Option<DateTime<Utc>> {
        match &self.time {
            Some(TimeFilter::NewerThan(t))
            | Some(TimeFilter::NewerOrEqual(t)) => Some(t.clone()),

            Some(TimeFilter::During(t)) => {
                Some(during_window(t.clone()).0)
            }

            Some(TimeFilter::Between { start, .. }) => {
                Some(start.clone())
            }

            _ => None,
        }
    }

    pub fn upper_time_bound(&self) -> Option<DateTime<Utc>> {
        match &self.time {
            Some(TimeFilter::OlderThan(t))
            | Some(TimeFilter::OlderOrEqual(t)) => Some(t.clone()),

            Some(TimeFilter::During(t)) => {
                Some(during_window(t.clone()).1)
            }

            Some(TimeFilter::Between { end, .. }) => {
                Some(end.clone())
            }

            _ => None,
        }
    }
}

/// Returns the half-open time window [start, end)
/// represented by a During filter.
pub fn during_window(
    t: DateTime<Utc>,
) -> (DateTime<Utc>, DateTime<Utc>) {
    let (start, duration) =
        if t.second() != 0 || t.nanosecond() != 0 {
            (
                t.with_nanosecond(0).expect("valid nanosecond"),
                Duration::seconds(1),
            )
        } else if t.minute() != 0 {
            (t, Duration::minutes(1))
        } else {
            (t, Duration::hours(1))
        };

    let end = start.clone() + duration;
    (start, end)
}

/// Case-insensitive substring matching.
/// Returns true if at least one needle is found.
pub fn contains_any_ci(
    haystack: &str,
    needles: &[String],
) -> bool {
    let haystack_lower = haystack.to_lowercase();

    needles.iter().any(|needle| {
        haystack_lower.contains(&needle.to_lowercase())
    })
}
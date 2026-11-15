//! The capture-rules form: a draft of time and text filters that becomes the engine's
//! `CaptureFilters` when applied.

use crate::read::rows::sentence;
use crate::read::search::MAX_PATTERN_CHARS;
use chrono::{DateTime, SecondsFormat, Utc};
use rune::TextEdit;
use shared_eventhub_core::filters::{
    CaptureFilters, TextFilter, TimeFilter, TimeWindow, parse_time, parse_time_with_granularity,
};
use std::cell::RefCell;

/// The time condition the form builds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TimeMode {
    #[default]
    None,
    OlderThan,
    OlderOrEqual,
    NewerThan,
    NewerOrEqual,
    During,
    Between,
}

impl TimeMode {
    pub const ALL: [Self; 7] = [
        Self::None,
        Self::OlderThan,
        Self::OlderOrEqual,
        Self::NewerThan,
        Self::NewerOrEqual,
        Self::During,
        Self::Between,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::OlderThan => "Older than",
            Self::OlderOrEqual => "Older or equal",
            Self::NewerThan => "Newer than",
            Self::NewerOrEqual => "Newer or equal",
            Self::During => "During",
            Self::Between => "Between",
        }
    }

    pub fn from_label(label: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.label() == label)
    }
}

/// The text condition the form builds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextMode {
    #[default]
    None,
    Contains,
    Regex,
}

impl TextMode {
    pub const ALL: [Self; 3] = [Self::None, Self::Contains, Self::Regex];

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Contains => "Contains",
            Self::Regex => "Regex",
        }
    }

    pub fn from_label(label: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.label() == label)
    }
}

/// What is wrong with each field of a draft.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RulesIssues {
    pub time_start: Option<String>,
    pub time_end: Option<String>,
    pub text: Option<String>,
}

type Parsed = Result<CaptureFilters, RulesIssues>;

/// The inputs a parse ran on, so an unchanged draft is not parsed again.
#[derive(Clone, Debug, PartialEq, Eq)]
struct DraftKey {
    time_mode: TimeMode,
    start: String,
    end: String,
    text_mode: TextMode,
    text: String,
}

/// The sidebar's draft rules.
pub struct RulesForm {
    pub time_mode: TimeMode,
    pub time_start: TextEdit,
    pub time_end: TextEdit,
    pub text_mode: TextMode,
    pub text: TextEdit,
    /// Whether each mode picker's list is open.
    pub time_open: bool,
    pub text_open: bool,
    parsed: RefCell<Option<(DraftKey, Parsed)>>,
}

impl std::fmt::Debug for RulesForm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RulesForm")
            .field("time_mode", &self.time_mode)
            .field("text_mode", &self.text_mode)
            .finish_non_exhaustive()
    }
}

impl Default for RulesForm {
    fn default() -> Self {
        Self {
            time_mode: TimeMode::None,
            time_start: TextEdit::new(""),
            time_end: TextEdit::new(""),
            text_mode: TextMode::None,
            text: TextEdit::new(""),
            time_open: false,
            text_open: false,
            parsed: RefCell::new(None),
        }
    }
}

impl RulesForm {
    /// The draft showing `filters`, as a recalled config holds them.
    pub fn from_filters(filters: &CaptureFilters) -> Self {
        let mut form = Self::default();
        let (mode, start, end) = match &filters.time {
            None => (TimeMode::None, None, None),
            Some(TimeFilter::OlderThan(at)) => (TimeMode::OlderThan, Some(at), None),
            Some(TimeFilter::OlderOrEqual(at)) => (TimeMode::OlderOrEqual, Some(at), None),
            Some(TimeFilter::NewerThan(at)) => (TimeMode::NewerThan, Some(at), None),
            Some(TimeFilter::NewerOrEqual(at)) => (TimeMode::NewerOrEqual, Some(at), None),
            Some(TimeFilter::During(at)) => (TimeMode::During, Some(at), None),
            Some(TimeFilter::Between { start, end }) => (TimeMode::Between, Some(start), Some(end)),
        };
        form.time_mode = mode;
        if let Some(start) = start {
            form.time_start = TextEdit::new(stamp(start));
        }
        if let Some(end) = end {
            form.time_end = TextEdit::new(stamp(end));
        }
        match &filters.text {
            None => {}
            Some(TextFilter::Contains(needles)) => {
                form.text_mode = TextMode::Contains;
                form.text = TextEdit::new(needles.join(", "));
            }
            Some(TextFilter::Regex(regex)) => {
                form.text_mode = TextMode::Regex;
                form.text = TextEdit::new(regex.as_str());
            }
        }
        form
    }

    /// The filters the draft describes, or what is wrong with it. Parsed once per change.
    pub fn parsed(&self) -> Parsed {
        let key = DraftKey {
            time_mode: self.time_mode,
            start: self.time_start.text.clone(),
            end: self.time_end.text.clone(),
            text_mode: self.text_mode,
            text: self.text.text.clone(),
        };
        let mut cache = self.parsed.borrow_mut();
        if let Some((cached, parsed)) = cache.as_ref()
            && *cached == key
        {
            return parsed.clone();
        }
        let parsed = parse(&key);
        *cache = Some((key, parsed.clone()));
        parsed
    }
}

fn parse(draft: &DraftKey) -> Parsed {
    let mut issues = RulesIssues::default();
    let time = time_filter(draft, &mut issues);
    let text = text_filter(draft, &mut issues);
    if issues != RulesIssues::default() {
        return Err(issues);
    }
    Ok(CaptureFilters { time, text })
}

/// `value` parsed by `parse`, or `None` with the field's issue set.
fn field(
    value: &str,
    parse: fn(&str) -> anyhow::Result<DateTime<Utc>>,
    issue: &mut Option<String>,
) -> Option<DateTime<Utc>> {
    let value = value.trim();
    if value.is_empty() {
        *issue = Some("Enter a time".to_owned());
        return None;
    }
    match parse(value) {
        Ok(time) => Some(time),
        Err(error) => {
            *issue = Some(sentence(&format!("{error:#}")));
            None
        }
    }
}

fn time_filter(draft: &DraftKey, issues: &mut RulesIssues) -> Option<TimeFilter> {
    let start = |issues: &mut RulesIssues| field(&draft.start, parse_time, &mut issues.time_start);
    match draft.time_mode {
        TimeMode::None => None,
        TimeMode::OlderThan => start(issues).map(TimeFilter::OlderThan),
        TimeMode::OlderOrEqual => start(issues).map(TimeFilter::OlderOrEqual),
        TimeMode::NewerThan => start(issues).map(TimeFilter::NewerThan),
        TimeMode::NewerOrEqual => start(issues).map(TimeFilter::NewerOrEqual),
        TimeMode::During => field(
            &draft.start,
            parse_time_with_granularity,
            &mut issues.time_start,
        )
        .map(TimeFilter::During),
        TimeMode::Between => {
            let from = start(issues);
            let to = field(&draft.end, parse_time, &mut issues.time_end);
            let (from, to) = (from?, to?);
            match TimeWindow::new(from, to) {
                Ok(window) => Some(window.into()),
                Err(_) => {
                    issues.time_end = Some("The end is before the start".to_owned());
                    None
                }
            }
        }
    }
}

fn text_filter(draft: &DraftKey, issues: &mut RulesIssues) -> Option<TextFilter> {
    let text = draft.text.trim();
    match draft.text_mode {
        TextMode::None => None,
        TextMode::Contains => {
            let needles: Vec<String> = text
                .split(',')
                .map(str::trim)
                .filter(|needle| !needle.is_empty())
                .map(str::to_owned)
                .collect();
            if needles.is_empty() {
                issues.text = Some("Enter one or more keywords, separated by commas".to_owned());
                return None;
            }
            Some(TextFilter::Contains(needles))
        }
        TextMode::Regex => {
            if text.is_empty() {
                issues.text = Some("Enter a regular expression".to_owned());
                return None;
            }
            if text.chars().count() > MAX_PATTERN_CHARS {
                issues.text = Some(format!(
                    "A pattern is limited to {MAX_PATTERN_CHARS} characters"
                ));
                return None;
            }
            match TextFilter::regex(text) {
                Ok(filter) => Some(filter),
                Err(error) => {
                    issues.text = Some(format!("{error:#}"));
                    None
                }
            }
        }
    }
}

/// `time` as the rules fields show it: RFC3339 UTC with milliseconds.
pub fn stamp(time: &DateTime<Utc>) -> String {
    time.to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// How many conditions `filters` sets (0 to 2), for the sidebar's badge.
pub fn rule_count(filters: &CaptureFilters) -> usize {
    usize::from(filters.time.is_some()) + usize::from(filters.text.is_some())
}

#[cfg(test)]
mod tests;

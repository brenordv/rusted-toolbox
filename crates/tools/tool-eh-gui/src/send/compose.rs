//! The composer: the event body, its metadata, how many to send, where they go, and how fast,
//! checked through core's own constructors.

use super::template::TemplateState;
use crate::read::rows::display_line;
use common_utils::string_utils::{format_thousands, non_blank};
use mock_data_utils::template::Template;
use rune::TextEdit;
use shared_eventhub_core::producer::{
    MAX_SEND_COUNT, MAX_SEND_INTERVAL_MS, OutgoingEvent, Pacing, Routing, SendPlan,
    insert_property, parse_property,
};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

/// The largest body the editor sends, in bytes.
pub const BODY_CAP: usize = 1024 * 1024;
/// The most properties text the composer parses, in bytes; it is parsed on every view build.
pub const PROPERTIES_CAP: usize = 4 * 1024;
const PROPERTIES_ISSUE: &str = "Properties are over 4 KB; remove some";
/// The count a new composer starts with.
const DEFAULT_COUNT: &str = "1";
/// The interval a new composer starts with, in milliseconds.
const DEFAULT_INTERVAL_MS: &str = "1000";
const TARGET_ISSUE: &str = "Set up the connection first";
const KEY_ISSUE: &str = "Enter a partition key, or pick Automatic";
const BODY_ISSUE: &str = "Bodies over 1 MB don't fit this editor; use eh send --body-file";

/// Where the events go inside the event hub.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RoutingChoice {
    #[default]
    Automatic,
    PartitionKey,
}

impl RoutingChoice {
    pub const ALL: [RoutingChoice; 2] = [Self::Automatic, Self::PartitionKey];

    pub fn label(self) -> &'static str {
        match self {
            Self::Automatic => "Automatic",
            Self::PartitionKey => "Partition key",
        }
    }

    pub fn from_label(label: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|choice| choice.label() == label)
    }
}

/// How fast the events go.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PacingChoice {
    #[default]
    Batched,
    Paced,
}

impl PacingChoice {
    pub const ALL: [PacingChoice; 2] = [Self::Batched, Self::Paced];

    pub fn label(self) -> &'static str {
        match self {
            Self::Batched => "Batched",
            Self::Paced => "Paced",
        }
    }

    pub fn from_label(label: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|choice| choice.label() == label)
    }
}

/// A part of the Send section that can hold an issue, in the order focus visits them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendField {
    Target,
    PartitionKey,
    Count,
    Interval,
    Properties,
    Body,
}

/// What is wrong with each part, for the parts the current choices show.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SendIssues {
    pub target: Option<String>,
    pub partition_key: Option<String>,
    pub count: Option<String>,
    pub interval: Option<String>,
    /// Properties text over its cap, or the first line that does not parse or repeats a key,
    /// as typed (escape and cut it for display).
    pub properties: Option<String>,
    /// A body over the editor's cap, or, under Template, a body that does not compile (an
    /// over-cap body is never compiled, so the two never meet).
    pub body: Option<String>,
}

impl SendIssues {
    fn in_order(&self) -> [(SendField, &Option<String>); 6] {
        let SendIssues {
            target,
            partition_key,
            count,
            interval,
            properties,
            body,
        } = self;
        [
            (SendField::Target, target),
            (SendField::PartitionKey, partition_key),
            (SendField::Count, count),
            (SendField::Interval, interval),
            (SendField::Properties, properties),
            (SendField::Body, body),
        ]
    }

    /// How many parts have an issue.
    pub fn len(&self) -> usize {
        self.in_order()
            .iter()
            .filter(|(_, issue)| issue.is_some())
            .count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The first part with an issue, in focus order.
    pub fn first(&self) -> Option<SendField> {
        self.in_order()
            .into_iter()
            .find_map(|(field, issue)| issue.as_ref().map(|_| field))
    }
}

/// The fields of one send: body, content type, metadata, count, routing, and pacing. `Debug`
/// prints no field text.
pub struct Composer {
    pub body: TextEdit,
    pub content_type: TextEdit,
    pub correlation_id: TextEdit,
    /// One `key=value` per line; blank lines are skipped.
    pub properties: TextEdit,
    pub count: TextEdit,
    pub routing: RoutingChoice,
    pub partition_key: TextEdit,
    pub pacing: PacingChoice,
    pub interval: TextEdit,
    /// Whether the routing picker's list shows.
    pub routing_open: bool,
    /// Whether the pacing picker's list shows.
    pub pacing_open: bool,
    /// The Template switch and what the body compiled to; written by
    /// [`Composer::refresh_template`], [`Composer::set_template`], and
    /// [`Composer::regenerate`] only.
    template: TemplateState,
    /// Whether the placeholder list shows.
    pub placeholders_open: bool,
}

impl std::fmt::Debug for Composer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let correlation_id = match non_blank(&self.correlation_id.text) {
            Some(_) => "set",
            None => "unset",
        };
        let property_lines = self
            .properties
            .text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count();
        f.debug_struct("Composer")
            .field("body_len", &self.body.text.len())
            .field("correlation_id", &correlation_id)
            .field("property_lines", &property_lines)
            .field("routing", &self.routing)
            .field("pacing", &self.pacing)
            .field("template", &self.template)
            .finish_non_exhaustive()
    }
}

impl Default for Composer {
    fn default() -> Self {
        Self {
            body: TextEdit::new(""),
            content_type: TextEdit::new(""),
            correlation_id: TextEdit::new(""),
            properties: TextEdit::new(""),
            count: TextEdit::new(DEFAULT_COUNT),
            routing: RoutingChoice::Automatic,
            partition_key: TextEdit::new(""),
            pacing: PacingChoice::Batched,
            interval: TextEdit::new(DEFAULT_INTERVAL_MS),
            routing_open: false,
            pacing_open: false,
            template: TemplateState::Off,
            placeholders_open: false,
        }
    }
}

/// What a valid composer sends: the plan, the event (the body as typed, which is the template's
/// source when the switch is on), and the compiled template when there is one.
#[derive(Debug)]
pub struct Composed {
    pub plan: SendPlan,
    pub event: OutgoingEvent,
    pub template: Option<Arc<Template>>,
}

/// What a valid composer's checks produced: the plan and the parsed properties. `Debug` shows
/// the plan and the property count, never a key or value.
pub struct Validated {
    pub plan: SendPlan,
    pub properties: BTreeMap<String, String>,
}

impl std::fmt::Debug for Validated {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Validated")
            .field("plan", &self.plan)
            .field("property_count", &self.properties.len())
            .finish()
    }
}

/// The properties `text` holds, one `key=value` per line with blank lines skipped, or the issue:
/// the cap, or "Line N: ..." for the first line that does not parse or repeats a key (lines
/// counted as typed, blank ones included).
fn parse_properties(text: &str) -> Result<BTreeMap<String, String>, String> {
    if text.len() > PROPERTIES_CAP {
        return Err(PROPERTIES_ISSUE.to_owned());
    }
    let mut properties = BTreeMap::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let issue = |error: anyhow::Error| format!("Line {}: {error:#}", index + 1);
        let (key, value) = parse_property(line).map_err(issue)?;
        insert_property(&mut properties, key, value).map_err(issue)?;
    }
    Ok(properties)
}

/// A whole number typed as ASCII digits, spaces around it allowed; `None` for anything else,
/// including a sign and a value past `u64`.
fn digits(text: &str) -> Option<u64> {
    let text = text.trim();
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

fn count_issue() -> String {
    format!(
        "Enter a number from 1 to {}",
        format_thousands(MAX_SEND_COUNT)
    )
}

fn interval_issue() -> String {
    format!(
        "Enter an interval from 1 to {} ms (one hour)",
        format_thousands(MAX_SEND_INTERVAL_MS)
    )
}

impl Composer {
    /// The Template switch and what the body compiled to.
    pub fn template(&self) -> &TemplateState {
        &self.template
    }

    /// Recompiles the body after it changed, keeping the switch where it is.
    pub fn refresh_template(&mut self) {
        self.template = TemplateState::for_body(self.template.is_on(), &self.body.text);
    }

    /// Turns the Template switch on or off; turning it off closes the placeholder list.
    pub fn set_template(&mut self, on: bool) {
        self.template = TemplateState::for_body(on, &self.body.text);
        if !on {
            self.placeholders_open = false;
        }
    }

    /// Renders a new sample of a compiled template; true when there was one.
    pub fn regenerate(&mut self) -> bool {
        self.template.regenerate()
    }

    fn routing(&self) -> Option<Routing> {
        match self.routing {
            RoutingChoice::Automatic => Some(Routing::automatic()),
            RoutingChoice::PartitionKey => {
                Routing::partition_key(self.partition_key.text.clone()).ok()
            }
        }
    }

    fn pacing(&self) -> Option<Pacing> {
        match self.pacing {
            PacingChoice::Batched => Some(Pacing::batched()),
            PacingChoice::Paced => digits(&self.interval.text)
                .and_then(|ms| Pacing::interval(Duration::from_millis(ms)).ok()),
        }
    }

    /// The plan and the event the composer describes, or what is wrong with it; `target_ready`
    /// says whether the connection form names a usable event hub.
    ///
    /// # Errors
    /// Returns the issues of every part that is wrong.
    pub fn compose(&self, target_ready: bool) -> Result<Composed, Box<SendIssues>> {
        let Validated { plan, properties } = self.validate(target_ready)?;
        Ok(Composed {
            plan,
            event: self.event(properties),
            template: self.template.template().cloned(),
        })
    }

    /// The plan and properties the composer describes, or what is wrong with it, without
    /// copying the body. Validity is decided by core's constructors and parser; `target_ready`
    /// says whether the connection form names a usable event hub.
    ///
    /// # Errors
    /// Returns the issues of every part that is wrong.
    pub fn validate(&self, target_ready: bool) -> Result<Validated, Box<SendIssues>> {
        let count = digits(&self.count.text);
        let routing = self.routing();
        let pacing = self.pacing();
        let properties = parse_properties(&self.properties.text);
        let plan = match (count, &routing, pacing) {
            (Some(count), Some(routing), Some(pacing)) => {
                Some(SendPlan::new(count, routing.clone(), pacing))
            }
            (None, _, _) | (Some(_), None, _) | (Some(_), Some(_), None) => None,
        };
        let count_ok = match (count, &plan) {
            (Some(_), Some(built)) => built.is_ok(),
            (Some(count), None) => {
                SendPlan::new(count, Routing::automatic(), Pacing::batched()).is_ok()
            }
            (None, _) => false,
        };
        let issues = SendIssues {
            target: (!target_ready).then(|| TARGET_ISSUE.to_owned()),
            partition_key: routing.is_none().then(|| KEY_ISSUE.to_owned()),
            count: (!count_ok).then(count_issue),
            interval: pacing.is_none().then(interval_issue),
            properties: properties.as_ref().err().cloned(),
            body: (self.body.text.len() > BODY_CAP)
                .then(|| BODY_ISSUE.to_owned())
                .or_else(|| self.template.issue().map(str::to_owned)),
        };
        match (plan, properties) {
            (Some(Ok(plan)), Ok(properties)) if issues.is_empty() => {
                Ok(Validated { plan, properties })
            }
            (Some(Ok(_)) | Some(Err(_)) | None, Ok(_) | Err(_)) => Err(Box::new(issues)),
        }
    }

    /// What is wrong with the composer, or no issues.
    pub fn issues(&self, target_ready: bool) -> SendIssues {
        self.validate(target_ready)
            .map_or_else(|issues| *issues, |_| SendIssues::default())
    }

    fn event(&self, properties: BTreeMap<String, String>) -> OutgoingEvent {
        OutgoingEvent {
            body: self.body.text.as_bytes().to_vec(),
            content_type: non_blank(&self.content_type.text),
            correlation_id: non_blank(&self.correlation_id.text),
            properties,
        }
    }

    /// The idle line: how many events go to `hub`, how they are routed and paced, naming only
    /// what is valid.
    pub fn summary(&self, hub: Option<&str>) -> String {
        let mut line = digits(&self.count.text)
            .filter(|count| SendPlan::new(*count, Routing::automatic(), Pacing::batched()).is_ok())
            .map_or_else(|| "Send".to_owned(), events);
        if let Some(hub) = hub {
            line.push_str(&format!(" to {}", display_line(hub.chars())));
        }
        if let Some(key) = self.routing().as_ref().and_then(Routing::key) {
            line.push_str(&format!(", partition key {}", display_line(key.chars())));
        }
        if let Some(interval) = self.pacing().and_then(Pacing::get) {
            line.push_str(&pacing_words(interval));
        }
        line
    }
}

/// "1 event" or "1,000 events".
pub fn events(count: u64) -> String {
    match count {
        1 => "1 event".to_owned(),
        count => format!("{} events", format_thousands(count)),
    }
}

/// ", one every 1,000 ms" for a paced interval.
pub fn pacing_words(interval: Duration) -> String {
    // `Pacing::interval` caps an interval at an hour, so its milliseconds always fit.
    let ms = u64::try_from(interval.as_millis()).unwrap_or(MAX_SEND_INTERVAL_MS);
    format!(", one every {} ms", format_thousands(ms))
}

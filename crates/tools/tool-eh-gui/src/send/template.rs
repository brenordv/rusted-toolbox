//! The Template switch: whether the body is a template, what it compiled to, and one sample.

use super::compose::BODY_CAP;
use crate::read::rows::{cut_at_char_boundary, sentence};
use chrono::Utc;
use mock_data_utils::template::Template;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// The most bytes of a sample the Send section shows.
pub const SAMPLE_SHOWN_BYTES: usize = 16 * 1024;

/// What the Template switch and the body come to.
#[derive(Default)]
pub enum TemplateState {
    /// The switch is off; the body is sent as written.
    #[default]
    Off,
    /// On, but the body is over the editor's cap, so nothing is compiled.
    OverCap,
    /// On, and the body does not compile; the text says where and why.
    Invalid(String),
    /// On and compiled, with one sample render (or the render's error) and how many times
    /// the sample was rendered again.
    Ready {
        template: Arc<Template>,
        sample: Result<String, String>,
        generation: u64,
    },
}

impl fmt::Debug for TemplateState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Off => f.write_str("Off"),
            Self::OverCap => f.write_str("OverCap"),
            Self::Invalid(_) => f.write_str("Invalid"),
            Self::Ready {
                template,
                generation,
                ..
            } => f
                .debug_struct("Ready")
                .field("placeholders", &template.placeholder_count())
                .field("generation", generation)
                .finish_non_exhaustive(),
        }
    }
}

impl TemplateState {
    /// The state for `body` with the switch `on`: compiled, and rendered once when it compiles,
    /// with `next` as the number its `{{sequence}}` shows.
    pub fn for_body(on: bool, body: &str, next: u64) -> Self {
        if !on {
            return Self::Off;
        }
        if body.len() > BODY_CAP {
            return Self::OverCap;
        }
        match Template::compile(body) {
            Ok(template) => Self::Ready {
                sample: sample_of(&template, next),
                template: Arc::new(template),
                generation: 0,
            },
            Err(error) => Self::Invalid(sentence(&format!("{error:#}"))),
        }
    }

    /// Whether the switch is on.
    pub fn is_on(&self) -> bool {
        match self {
            Self::Off => false,
            Self::OverCap | Self::Invalid(_) | Self::Ready { .. } => true,
        }
    }

    /// Renders a new sample with `next` as the number its `{{sequence}}` shows; true when
    /// there was a compiled template to render.
    pub fn regenerate(&mut self, next: u64) -> bool {
        match self {
            Self::Ready {
                template,
                sample,
                generation,
            } => {
                *sample = sample_of(template, next);
                *generation += 1;
                true
            }
            Self::Off | Self::OverCap | Self::Invalid(_) => false,
        }
    }

    /// The compiled template, when the body is one.
    pub fn template(&self) -> Option<&Arc<Template>> {
        match self {
            Self::Ready { template, .. } => Some(template),
            Self::Off | Self::OverCap | Self::Invalid(_) => None,
        }
    }

    /// Why the body cannot be sent as a template, when it cannot; an over-cap body already has
    /// the body's own issue.
    pub fn issue(&self) -> Option<&str> {
        match self {
            Self::Invalid(text) => Some(text),
            Self::Off | Self::OverCap | Self::Ready { .. } => None,
        }
    }
}

fn sample_of(template: &Template, next: u64) -> Result<String, String> {
    template
        .render(BODY_CAP, Utc::now(), next)
        .map_err(|error| format!("{error:#}"))
}

/// The number an event built from `template` shows: for a template that uses `{{sequence}}`,
/// `counter`'s value, which then moves on by one; otherwise 0, with `counter` left alone.
pub fn take_sequence(template: Option<&Template>, counter: &AtomicU64) -> u64 {
    match template {
        // The add alone keeps the numbers unique; a reader that sees an older value only
        // shows a lower next number.
        Some(template) if template.uses_sequence() => counter.fetch_add(1, Ordering::Relaxed),
        Some(_) | None => 0,
    }
}

/// The part of `sample` the Send section shows: at most [`SAMPLE_SHOWN_BYTES`], cut at a
/// character boundary, and whether it was cut.
pub fn shown_sample(sample: &str) -> (&str, bool) {
    let shown = cut_at_char_boundary(sample, SAMPLE_SHOWN_BYTES);
    (shown, shown.len() < sample.len())
}

#[cfg(test)]
mod tests;

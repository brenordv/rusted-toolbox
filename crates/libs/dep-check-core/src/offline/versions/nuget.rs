//! NuGet's version order. `NuGetVersion` "only requires the major segment to be defined. All
//! others are optional, and are equivalent to zero"; NuGet "chooses a version without a suffix
//! first, then applies precedence to pre-release versions in reverse alphabetical order and
//! treats dot notation numbers with numerical order"; it "uses case insensitive string
//! comparisons for pre-release components"; build metadata is removed. No crate implements
//! this, so it is written here from that page.

use super::Number;
use std::cmp::Ordering;

/// The most numbers a NuGet version has: major, minor, patch, revision.
const MAX_PARTS: usize = 4;

/// One NuGet version, build metadata removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NuGetVersion {
    /// Always four numbers; a part not written is zero.
    release: [Number; MAX_PARTS],
    /// The pre-release labels; empty for a release.
    pre: Vec<Label>,
}

/// One dot-separated pre-release label.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Label {
    Numeric(Number),
    /// Lowercased, since NuGet compares labels without regard to case.
    Text(String),
}

impl NuGetVersion {
    /// `text` as a NuGet version: one to four dot-separated numbers, an optional `-` and
    /// dot-separated labels of ASCII letters, digits, and `-`, and optional `+` build metadata.
    pub(crate) fn parse(text: &str) -> Option<NuGetVersion> {
        let without_build = text.split_once('+').map_or(text, |(version, _)| version);
        let (release_text, pre_text) = match without_build.split_once('-') {
            Some((release, pre)) => (release, Some(pre)),
            None => (without_build, None),
        };
        let parts: Vec<&str> = release_text.split('.').collect();
        if parts.len() > MAX_PARTS {
            return None;
        }
        let mut release = [
            Number::zero(),
            Number::zero(),
            Number::zero(),
            Number::zero(),
        ];
        for (slot, part) in release.iter_mut().zip(&parts) {
            *slot = Number::of(part)?;
        }
        let pre = match pre_text {
            Some(labels) => labels.split('.').map(Label::parse).collect::<Option<_>>()?,
            None => Vec::new(),
        };
        Some(NuGetVersion { release, pre })
    }
}

impl Label {
    fn parse(text: &str) -> Option<Label> {
        if text.is_empty() || !text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
            return None;
        }
        Some(match Number::of(text) {
            Some(number) => Label::Numeric(number),
            None => Label::Text(text.to_ascii_lowercase()),
        })
    }
}

impl PartialOrd for Label {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Label {
    /// Numbers by value, text by its letters, and a number below any text.
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Label::Numeric(a), Label::Numeric(b)) => a.cmp(b),
            (Label::Text(a), Label::Text(b)) => a.cmp(b),
            (Label::Numeric(_), Label::Text(_)) => Ordering::Less,
            (Label::Text(_), Label::Numeric(_)) => Ordering::Greater,
        }
    }
}

impl PartialOrd for NuGetVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for NuGetVersion {
    /// The numbers first; at equal numbers a release above any pre-release, and pre-releases
    /// label by label, the one that runs out first lower.
    fn cmp(&self, other: &Self) -> Ordering {
        self.release.cmp(&other.release).then_with(|| {
            match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => self.pre.cmp(&other.pre),
            }
        })
    }
}

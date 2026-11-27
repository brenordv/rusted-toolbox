//! RubyGems' version order, ported from `Gem::Version` in rubygems v4.1.1
//! (`lib/rubygems/version.rb`): a version must match `VERSION_PATTERN`, every `-` reads as
//! `.pre.`, the canonical segments drop trailing zeros and the zeros before the first letter,
//! and `<=>` compares segment by segment with a letter segment below a number. The class doc
//! says it this way: "Versions with a prerelease part in the Nth part sort less than versions
//! with N-1 parts." The sort-key shortcut `<=>` takes for releases is left out; it orders the
//! same versions the same way.

use super::Number;
use std::cmp::Ordering;

/// One RubyGems version, as its canonical segments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GemVersion {
    segments: Vec<Segment>,
}

/// A run of digits or a run of letters.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    Number(Number),
    /// As written: Ruby compares strings byte by byte, so case counts.
    Text(String),
}

impl GemVersion {
    /// `text` as a RubyGems version. Only text that `VERSION_PATTERN` matches whole parses;
    /// the blank string upstream reads as `0` does not, so it fails closed.
    pub(crate) fn parse(text: &str) -> Option<GemVersion> {
        if !matches_version_pattern(text) {
            return None;
        }
        let version = text.replace('-', ".pre.");
        let prerelease = version.bytes().any(|b| b.is_ascii_alphabetic());
        let mut canonical = drop_trailing_zeros(&version);
        if prerelease {
            canonical = drop_zeros_before_first_letter(&canonical);
        }
        Some(GemVersion {
            segments: segments_of(&canonical),
        })
    }
}

/// `[0-9]+(?>\.[0-9a-zA-Z]+)*(-[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?`, matched against all of
/// `text`.
fn matches_version_pattern(text: &str) -> bool {
    let (head, tail) = match text.split_once('-') {
        Some((head, tail)) => (head, Some(tail)),
        None => (text, None),
    };
    let mut parts = head.split('.');
    let first_is_number = parts
        .next()
        .is_some_and(|first| !first.is_empty() && first.bytes().all(|b| b.is_ascii_digit()));
    let rest_are_words =
        parts.all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric()));
    let tail_is_words = tail.is_none_or(|tail| {
        tail.split('.').all(|part| {
            !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
    });
    first_is_number && rest_are_words && tail_is_words
}

/// `version.sub(/(?<=[a-zA-Z.])[.0]+\z/, "")`: the leftmost run of `.` and `0` that reaches
/// the end and follows a letter or a dot is removed.
fn drop_trailing_zeros(version: &str) -> String {
    let bytes = version.as_bytes();
    let run = bytes
        .iter()
        .rev()
        .take_while(|b| matches!(b, b'.' | b'0'))
        .count();
    let start = bytes.len() - run;
    let cut = (start.max(1)..bytes.len())
        .find(|&position| bytes[position - 1].is_ascii_alphabetic() || bytes[position - 1] == b'.');
    match cut {
        Some(position) => version[..position].to_owned(),
        None => version.to_owned(),
    }
}

/// `version.sub(/(?<=\.|\A)[0.]+(?=[a-zA-Z])/, "")`: the leftmost run of `0` and `.` that
/// starts the text or follows a dot, and is followed by a letter, is removed.
fn drop_zeros_before_first_letter(version: &str) -> String {
    let bytes = version.as_bytes();
    for start in 0..bytes.len() {
        let anchored = start == 0 || bytes[start - 1] == b'.';
        if !anchored || !matches!(bytes[start], b'0' | b'.') {
            continue;
        }
        let end = start
            + bytes[start..]
                .iter()
                .take_while(|b| matches!(b, b'0' | b'.'))
                .count();
        if end < bytes.len() && bytes[end].is_ascii_alphabetic() {
            return format!("{}{}", &version[..start], &version[end..]);
        }
    }
    version.to_owned()
}

/// `ver.scan(/\d+|[a-z]+/i)`, each run of digits a number.
fn segments_of(version: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let bytes = version.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let digits = bytes[index].is_ascii_digit();
        let letters = bytes[index].is_ascii_alphabetic();
        if !digits && !letters {
            index += 1;
            continue;
        }
        let end = index
            + bytes[index..]
                .iter()
                .take_while(|b| {
                    if digits {
                        b.is_ascii_digit()
                    } else {
                        b.is_ascii_alphabetic()
                    }
                })
                .count();
        let run = &version[index..end];
        segments.push(match Number::of(run) {
            Some(number) => Segment::Number(number),
            None => Segment::Text(run.to_owned()),
        });
        index = end;
    }
    segments
}

impl PartialOrd for GemVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for GemVersion {
    /// `Gem::Version#<=>`: segment by segment up to the shorter one, a letter segment below a
    /// number; then the longer one's remaining segments decide: a letter segment makes it
    /// lower, a non-zero number higher, and zeros are skipped.
    fn cmp(&self, other: &Self) -> Ordering {
        if self.segments == other.segments {
            return Ordering::Equal;
        }
        let shared = self.segments.len().min(other.segments.len());
        for (left, right) in self.segments.iter().zip(&other.segments).take(shared) {
            let order = match (left, right) {
                (Segment::Number(a), Segment::Number(b)) => a.cmp(b),
                (Segment::Text(a), Segment::Text(b)) => a.cmp(b),
                (Segment::Text(_), Segment::Number(_)) => Ordering::Less,
                (Segment::Number(_), Segment::Text(_)) => Ordering::Greater,
            };
            if order != Ordering::Equal {
                return order;
            }
        }
        if self.segments.len() <= other.segments.len() {
            remaining_order(&other.segments[shared..]).reverse()
        } else {
            remaining_order(&self.segments[shared..])
        }
    }
}

/// How the longer version's `rest` orders it against the shorter one: lower at its first letter
/// segment, higher at its first non-zero number, equal when every one is zero.
fn remaining_order(rest: &[Segment]) -> Ordering {
    for segment in rest {
        match segment {
            Segment::Text(_) => return Ordering::Less,
            Segment::Number(number) if !number.is_zero() => return Ordering::Greater,
            Segment::Number(_) => {}
        }
    }
    Ordering::Equal
}

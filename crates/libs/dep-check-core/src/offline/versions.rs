//! Version orderings for offline matching: SemVer 2.0 precedence, NuGet's, PEP 440's, and
//! RubyGems'. Each ordering parses a version into a key with a total order; a version that does
//! not parse has no key, so a caller can fail closed instead of guessing.

use crate::ecosystem::Ecosystem;
use std::cmp::Ordering as CmpOrdering;
use std::str::FromStr;

mod nuget;
mod rubygems;

pub(crate) use nuget::NuGetVersion;
pub(crate) use rubygems::GemVersion;

/// The rules one ecosystem's versions sort by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ordering {
    /// SemVer 2.0 precedence: crates.io, npm, and every `SEMVER` range.
    SemVer,
    /// NuGet's: one to four numbers, case-insensitive pre-release labels.
    NuGet,
    /// PEP 440's: PyPI.
    Pep440,
    /// RubyGems' `Gem::Version`.
    RubyGems,
}

/// How an OSV range's events are ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RangeOrdering {
    By(Ordering),
    /// A `GIT` range: commit hashes, which a package version cannot be placed among.
    Skip,
    /// A range type this tool does not know.
    Unknown,
}

/// The ordering for an OSV range of `range_type` in `ecosystem`: `SEMVER` by SemVer 2.0,
/// `ECOSYSTEM` by the ecosystem's own rules.
pub(crate) fn ordering_for(ecosystem: Ecosystem, range_type: &str) -> RangeOrdering {
    match range_type {
        "SEMVER" => RangeOrdering::By(Ordering::SemVer),
        "ECOSYSTEM" => RangeOrdering::By(ecosystem_ordering(ecosystem)),
        "GIT" => RangeOrdering::Skip,
        _ => RangeOrdering::Unknown,
    }
}

/// The ordering `ecosystem`'s own version strings sort by, as in an `ECOSYSTEM` range or a
/// `versions` list. Cargo "bakes in the concept of Semantic Versioning", and npm versions "must
/// be parseable by node-semver".
pub(crate) fn ecosystem_ordering(ecosystem: Ecosystem) -> Ordering {
    match ecosystem {
        Ecosystem::Cargo | Ecosystem::Npm => Ordering::SemVer,
        Ecosystem::Nuget => Ordering::NuGet,
        Ecosystem::Pypi => Ordering::Pep440,
        Ecosystem::Rubygems => Ordering::RubyGems,
    }
}

/// A SemVer 2.0 version ordered by precedence, where build metadata does not count: "Versions
/// that differ only in build metadata are considered equal".
#[derive(Debug, Clone)]
pub(crate) struct Precedence(semver::Version);

impl PartialEq for Precedence {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == CmpOrdering::Equal
    }
}

impl Eq for Precedence {}

impl PartialOrd for Precedence {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.cmp(other))
    }
}

impl Ord for Precedence {
    fn cmp(&self, other: &Self) -> CmpOrdering {
        self.0.cmp_precedence(&other.0)
    }
}

/// `text` as a SemVer 2.0 version. A leading `v`, four numbers, or a leading zero do not parse.
pub(crate) fn semver_key(text: &str) -> Option<Precedence> {
    semver::Version::parse(text).ok().map(Precedence)
}

/// `text` as a NuGet version.
pub(crate) fn nuget_key(text: &str) -> Option<NuGetVersion> {
    NuGetVersion::parse(text)
}

/// `text` as a PEP 440 version. A legacy version string does not parse.
pub(crate) fn pep440_key(text: &str) -> Option<pep440_rs::Version> {
    pep440_rs::Version::from_str(text).ok()
}

/// `text` as a RubyGems version.
pub(crate) fn rubygems_key(text: &str) -> Option<GemVersion> {
    GemVersion::parse(text)
}

/// `a` against `b` under `ordering`; `None` when either does not parse.
#[cfg(test)]
pub(crate) fn compare(ordering: Ordering, a: &str, b: &str) -> Option<CmpOrdering> {
    fn with<K: Ord>(parse: fn(&str) -> Option<K>, a: &str, b: &str) -> Option<CmpOrdering> {
        Some(parse(a)?.cmp(&parse(b)?))
    }
    match ordering {
        Ordering::SemVer => with(semver_key, a, b),
        Ordering::NuGet => with(nuget_key, a, b),
        Ordering::Pep440 => with(pep440_key, a, b),
        Ordering::RubyGems => with(rubygems_key, a, b),
    }
}

/// A run of ASCII digits as a number of any size: leading zeros dropped, compared by length
/// and then digit by digit, so no value overflows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Number(String);

impl Number {
    /// `digits`, which must be one or more ASCII digits.
    fn of(digits: &str) -> Option<Number> {
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let trimmed = digits.trim_start_matches('0');
        Some(Number(if trimmed.is_empty() {
            "0".to_owned()
        } else {
            trimmed.to_owned()
        }))
    }

    fn zero() -> Number {
        Number("0".to_owned())
    }

    fn is_zero(&self) -> bool {
        self.0 == "0"
    }
}

impl PartialOrd for Number {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.cmp(other))
    }
}

impl Ord for Number {
    fn cmp(&self, other: &Self) -> CmpOrdering {
        self.0
            .len()
            .cmp(&other.0.len())
            .then_with(|| self.0.cmp(&other.0))
    }
}

#[cfg(test)]
mod tests;

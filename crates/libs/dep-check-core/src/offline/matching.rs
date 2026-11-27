//! Whether an OSV record affects one package version, evaluated the way the OSV schema says
//! (1.9.2, `#evaluation`): a version is affected "if it lies within any one of the ranges or is
//! listed in the versions list". Anything the code cannot read with certainty (a version that
//! does not parse, a range type it does not know, an entry it cannot place the version in)
//! gives `Undecided`, never `NotAffected`.

use crate::ecosystem::Ecosystem;
use crate::offline::versions::{
    Ordering, RangeOrdering, ecosystem_ordering, nuget_key, ordering_for, pep440_key, rubygems_key,
    semver_key,
};
use crate::sources::osv::{OsvAffected, OsvEvent, OsvRange, is_this_package};
use std::cmp::Ordering as CmpOrdering;

/// What one record says about one package version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    Affected,
    NotAffected,
    /// The record could not be read with certainty for this version.
    Undecided,
}

/// What the `affected` entries of one record say about `name` at `version` in `ecosystem`.
/// A record with no entry for the package is `Undecided`: the index that returned it and the
/// record disagree.
pub(crate) fn evaluate(
    ecosystem: Ecosystem,
    name: &str,
    version: &str,
    affected: &[OsvAffected],
) -> Verdict {
    let mut entries = affected
        .iter()
        .filter(|entry| is_this_package(entry, ecosystem, name))
        .peekable();
    if entries.peek().is_none() {
        return Verdict::Undecided;
    }
    combine(entries.map(|entry| entry_verdict(ecosystem, version, entry)))
}

/// `Affected` when any verdict is; else `Undecided` when any is; else `NotAffected`.
fn combine(verdicts: impl Iterator<Item = Verdict>) -> Verdict {
    let mut undecided = false;
    for verdict in verdicts {
        match verdict {
            Verdict::Affected => return Verdict::Affected,
            Verdict::Undecided => undecided = true,
            Verdict::NotAffected => {}
        }
    }
    if undecided {
        Verdict::Undecided
    } else {
        Verdict::NotAffected
    }
}

/// One entry's verdict from its `versions` list and its ranges. An entry with neither a list
/// nor a range this tool can order (only `GIT` ranges, or nothing) is `Undecided`.
fn entry_verdict(ecosystem: Ecosystem, version: &str, entry: &OsvAffected) -> Verdict {
    let mut verdicts = Vec::new();
    if !entry.versions.is_empty() {
        verdicts.push(listed_verdict(ecosystem, version, &entry.versions));
    }
    for range in &entry.ranges {
        match range_kind(ecosystem, range) {
            RangeOrdering::By(ordering) => verdicts.push(range_verdict(ordering, version, range)),
            RangeOrdering::Skip => {}
            RangeOrdering::Unknown => verdicts.push(Verdict::Undecided),
        }
    }
    if verdicts.is_empty() {
        return Verdict::Undecided;
    }
    combine(verdicts.into_iter())
}

fn range_kind(ecosystem: Ecosystem, range: &OsvRange) -> RangeOrdering {
    range
        .kind
        .as_deref()
        .map_or(RangeOrdering::Unknown, |kind| ordering_for(ecosystem, kind))
}

/// The `versions` list: the version written the same way, or equal under the ecosystem's
/// ordering (NuGet's `1.0` and `1.0.0`).
fn listed_verdict(ecosystem: Ecosystem, version: &str, listed: &[String]) -> Verdict {
    if listed.iter().any(|entry| entry == version) {
        return Verdict::Affected;
    }
    match ecosystem_ordering(ecosystem) {
        Ordering::SemVer => listed_with(semver_key, version, listed),
        Ordering::NuGet => listed_with(nuget_key, version, listed),
        Ordering::Pep440 => listed_with(pep440_key, version, listed),
        Ordering::RubyGems => listed_with(rubygems_key, version, listed),
    }
}

fn listed_with<K: Ord>(parse: fn(&str) -> Option<K>, version: &str, listed: &[String]) -> Verdict {
    let Some(asked) = parse(version) else {
        return Verdict::Undecided;
    };
    let mut undecided = false;
    for entry in listed {
        match parse(entry) {
            Some(key) if key.cmp(&asked) == CmpOrdering::Equal => return Verdict::Affected,
            Some(_) => {}
            None => undecided = true,
        }
    }
    if undecided {
        Verdict::Undecided
    } else {
        Verdict::NotAffected
    }
}

fn range_verdict(ordering: Ordering, version: &str, range: &OsvRange) -> Verdict {
    match ordering {
        Ordering::SemVer => events_verdict(semver_key, version, &range.events),
        Ordering::NuGet => events_verdict(nuget_key, version, &range.events),
        Ordering::Pep440 => events_verdict(pep440_key, version, &range.events),
        Ordering::RubyGems => events_verdict(rubygems_key, version, &range.events),
    }
}

/// Where an event sits: `introduced: "0"` "sorts before any other version", and `limit: "*"`
/// is above every version.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Bound<K> {
    Bottom,
    At(K),
    Top,
}

/// The event kinds in the order the tool sorts them at an equal version. The schema gives no
/// order for a tie; with `last_affected` before `introduced`, every later version would read
/// affected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Introduced,
    LastAffected,
    Fixed,
    Limit,
}

/// The schema's walk over one range's events, sorted by version, inside its limits. "If no
/// limit events are provided, an implicit { "limit": "*" } is assumed."
fn events_verdict<K: Ord>(
    parse: fn(&str) -> Option<K>,
    version: &str,
    events: &[OsvEvent],
) -> Verdict {
    let Some(asked) = parse(version) else {
        return Verdict::Undecided;
    };
    let mut parsed: Vec<(Bound<K>, Kind)> = Vec::with_capacity(events.len());
    for event in events {
        match parse_event(parse, event) {
            Some(bound) => parsed.push(bound),
            None => return Verdict::Undecided,
        }
    }
    // The schema requires every range to have an introduced event; a range without one says
    // nothing a version can be placed in.
    if !parsed.iter().any(|(_, kind)| *kind == Kind::Introduced) {
        return Verdict::Undecided;
    }
    let asked = Bound::At(asked);
    let mut limits = parsed
        .iter()
        .filter(|(_, kind)| *kind == Kind::Limit)
        .peekable();
    if limits.peek().is_some() && !limits.any(|(limit, _)| asked < *limit) {
        return Verdict::NotAffected;
    }
    parsed.sort();
    let mut affected = false;
    for (bound, kind) in &parsed {
        match kind {
            Kind::Introduced if asked >= *bound => affected = true,
            Kind::Fixed if asked >= *bound => affected = false,
            Kind::LastAffected if asked > *bound => affected = false,
            Kind::Introduced | Kind::Fixed | Kind::LastAffected | Kind::Limit => {}
        }
    }
    if affected {
        Verdict::Affected
    } else {
        Verdict::NotAffected
    }
}

/// One event as its bound and kind; `None` when it does not hold exactly one field or its
/// version does not parse.
fn parse_event<K>(parse: fn(&str) -> Option<K>, event: &OsvEvent) -> Option<(Bound<K>, Kind)> {
    let fields = [
        (event.introduced.as_deref(), Kind::Introduced),
        (event.fixed.as_deref(), Kind::Fixed),
        (event.last_affected.as_deref(), Kind::LastAffected),
        (event.limit.as_deref(), Kind::Limit),
    ];
    let mut set = fields
        .into_iter()
        .filter_map(|(value, kind)| value.map(|value| (value, kind)));
    let (value, kind) = set.next()?;
    if set.next().is_some() {
        return None;
    }
    let bound = match (kind, value) {
        (Kind::Introduced, "0") => Bound::Bottom,
        (Kind::Limit, "*") => Bound::Top,
        _ => Bound::At(parse(value)?),
    };
    Some((bound, kind))
}

#[cfg(test)]
mod tests;

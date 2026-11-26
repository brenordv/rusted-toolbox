//! What the dep-check window holds, and how its inputs change it.

use crate::services::Services;
use chrono::{DateTime, Utc};
use common_gui_rune::Theme;
use common_gui_rune::parts::Tone;
use common_utils::string_utils::format_thousands;
use dep_check_core::ecosystem::Ecosystem;
use dep_check_core::engine::CheckRequest;
use dep_check_core::package::{InputIssue, IssueKind, MAX_PACKAGES, parse_specs};
use dep_check_core::render::{count_phrase, source_phrase};
use dep_check_core::report::{Outcome, Report, SourceReport, SourceStatus, display_time};
use rune::TextEdit;
use std::fmt;
use std::rc::Rc;

/// The Packages editor's key; a refused press moves focus there.
pub const PACKAGES_KEY: &str = "dep-check-packages";

/// The window's whole state.
pub struct DepCheckState {
    /// The ecosystem the next check covers; it lasts the session.
    pub ecosystem: Ecosystem,
    /// Whether the ecosystem picker's list is open. A lone toggle the picker's own handlers set:
    /// it disables no control and gates no request.
    pub picker_open: bool,
    /// The Packages editor's buffer, one package per line.
    pub packages: TextEdit,
    /// What the last refused press found wrong in Packages, until the next edit or pick.
    pub packages_issue: Option<InputIssue>,
    pub run: Run,
    /// How many answers a check has brought, so the results list measures each new set of
    /// rows instead of reusing the last one's heights.
    pub answers: u64,
    pub theme: Theme,
    pub services: Option<Rc<dyn Services>>,
}

/// Where the window's one check is.
pub enum Run {
    /// No check has run yet.
    Idle,
    /// A check of `packages` packages of `ecosystem` is running.
    Checking {
        ecosystem: Ecosystem,
        packages: usize,
    },
    /// The last check's report; `line_shown` is whether the status line still says how it went.
    Done {
        report: Box<Report>,
        line_shown: bool,
    },
    /// The last check found no service that answered, or stopped; `message` is the whole text.
    Failed { message: String, line_shown: bool },
}

/// Everything that can happen to the window.
pub enum Input {
    /// An ecosystem was picked.
    EcosystemPicked(Ecosystem),
    /// The Packages text changed.
    PackagesEdited,
    /// Check was pressed, or `mod+enter` asked for it.
    CheckPressed,
    /// A check's one answer: its report, or why there is none.
    Checked(Result<Box<Report>, String>),
}

impl fmt::Debug for Input {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Input::EcosystemPicked(ecosystem) => write!(f, "EcosystemPicked({})", ecosystem.name()),
            Input::PackagesEdited => f.write_str("PackagesEdited"),
            Input::CheckPressed => f.write_str("CheckPressed"),
            Input::Checked(Ok(report)) => {
                write!(f, "Checked(Ok({} packages))", report.packages.len())
            }
            Input::Checked(Err(text)) => write!(f, "Checked(Err({} chars))", text.chars().count()),
        }
    }
}

/// Work for the services.
pub enum Effect {
    /// Run this check.
    Check(CheckRequest),
}

impl fmt::Debug for Effect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Effect::Check(request) => write!(
                f,
                "Check({}, {} packages)",
                request.ecosystem.name(),
                request.packages.len()
            ),
        }
    }
}

/// What one input did: whether the view must run again, the work to start, and where focus
/// goes.
#[derive(Debug)]
pub struct Transition {
    pub changed: bool,
    pub effects: Vec<Effect>,
    pub focus: Option<&'static str>,
}

impl Transition {
    fn unchanged() -> Self {
        Self::of(false)
    }

    fn of(changed: bool) -> Self {
        Self {
            changed,
            effects: Vec::new(),
            focus: None,
        }
    }
}

/// The status line's words: a kind that keys its live region, its tone, and its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusWords {
    pub kind: &'static str,
    pub tone: Tone,
    pub text: String,
}

impl DepCheckState {
    /// A state with no services, for the window spec before the real start.
    pub fn detached() -> Self {
        Self::new(None)
    }

    /// The state the window starts in.
    pub fn new(services: Option<Rc<dyn Services>>) -> Self {
        Self {
            ecosystem: Ecosystem::Cargo,
            picker_open: false,
            packages: TextEdit::new(""),
            packages_issue: None,
            run: Run::Idle,
            answers: 0,
            theme: Theme::new(),
            services,
        }
    }

    /// Applies `input`. Pure: no clock, file, or network.
    pub fn apply(&mut self, input: Input) -> Transition {
        match input {
            Input::EcosystemPicked(ecosystem) => self.picked(ecosystem),
            Input::PackagesEdited => self.edited(),
            Input::CheckPressed => self.pressed(),
            Input::Checked(answer) => self.checked(answer),
        }
    }

    fn picked(&mut self, ecosystem: Ecosystem) -> Transition {
        self.picker_open = false;
        if ecosystem == self.ecosystem {
            // Nothing the input rule reads changed, so the issue and the line still hold.
            return Transition::of(true);
        }
        self.ecosystem = ecosystem;
        // Both the ecosystem and the text feed the input rule, so a pick clears the issue too.
        self.packages_issue = None;
        self.hide_line();
        Transition::of(true)
    }

    fn edited(&mut self) -> Transition {
        let had_issue = self.packages_issue.take().is_some();
        let hid_line = self.hide_line();
        Transition::of(had_issue || hid_line)
    }

    fn pressed(&mut self) -> Transition {
        match self.run {
            // `mod+enter` reaches here whatever the button shows.
            Run::Checking { .. } => return Transition::unchanged(),
            Run::Idle | Run::Done { .. } | Run::Failed { .. } => {}
        }
        match parse_specs(self.ecosystem, self.packages.text.lines()) {
            Ok(packages) => {
                self.packages_issue = None;
                self.run = Run::Checking {
                    ecosystem: self.ecosystem,
                    packages: packages.len(),
                };
                Transition {
                    changed: true,
                    effects: vec![Effect::Check(CheckRequest {
                        ecosystem: self.ecosystem,
                        packages,
                    })],
                    focus: None,
                }
            }
            Err(issue) => {
                self.packages_issue = Some(issue);
                self.hide_line();
                Transition {
                    changed: true,
                    effects: Vec::new(),
                    focus: Some(PACKAGES_KEY),
                }
            }
        }
    }

    fn checked(&mut self, answer: Result<Box<Report>, String>) -> Transition {
        match self.run {
            Run::Checking { .. } => {}
            Run::Idle | Run::Done { .. } | Run::Failed { .. } => return Transition::unchanged(),
        }
        self.answers += 1;
        self.run = match answer {
            Ok(report) => match report.outcome {
                Outcome::Failed => Run::Failed {
                    message: no_service_answered(&report),
                    line_shown: true,
                },
                Outcome::Clean | Outcome::Vulnerable | Outcome::Incomplete => Run::Done {
                    report,
                    line_shown: true,
                },
            },
            Err(text) => Run::Failed {
                message: format!("The check stopped. Press Check again. {text}"),
                line_shown: true,
            },
        };
        Transition::of(true)
    }

    /// Hides the outcome line of a finished check; true when one was showing.
    fn hide_line(&mut self) -> bool {
        match &mut self.run {
            Run::Done { line_shown, .. } | Run::Failed { line_shown, .. } => {
                std::mem::replace(line_shown, false)
            }
            Run::Idle | Run::Checking { .. } => false,
        }
    }

    /// The report on screen, when there is one.
    pub fn report(&self) -> Option<&Report> {
        match &self.run {
            Run::Done { report, .. } => Some(report),
            Run::Idle | Run::Checking { .. } | Run::Failed { .. } => None,
        }
    }

    /// The ecosystem the Results box is about: the running check's, else the report's.
    pub fn results_ecosystem(&self) -> Option<Ecosystem> {
        match &self.run {
            Run::Checking { ecosystem, .. } => Some(*ecosystem),
            Run::Done { report, .. } => Some(report.ecosystem),
            Run::Idle | Run::Failed { .. } => None,
        }
    }

    /// Whether a check is running.
    pub fn is_checking(&self) -> bool {
        match self.run {
            Run::Checking { .. } => true,
            Run::Idle | Run::Done { .. } | Run::Failed { .. } => false,
        }
    }

    /// What the status line says, highest first: a refused press; the running check; the last
    /// outcome until the next input; nothing.
    pub fn status(&self) -> Option<StatusWords> {
        if let Some(issue) = &self.packages_issue {
            return Some(StatusWords {
                kind: "refused",
                tone: Tone::Danger,
                text: refused_words(issue),
            });
        }
        match &self.run {
            Run::Checking { packages, .. } => Some(StatusWords {
                kind: "checking",
                tone: Tone::Muted,
                text: format!(
                    "Checking {}…",
                    count_phrase(*packages as u64, "package", "packages")
                ),
            }),
            Run::Done {
                report,
                line_shown: true,
            } => Some(StatusWords {
                kind: "done",
                tone: Tone::Muted,
                text: done_words(report),
            }),
            Run::Failed {
                message,
                line_shown: true,
            } => Some(StatusWords {
                kind: "failed",
                tone: Tone::Danger,
                text: message.clone(),
            }),
            Run::Idle
            | Run::Done {
                line_shown: false, ..
            }
            | Run::Failed {
                line_shown: false, ..
            } => None,
        }
    }
}

/// The issue caption under Packages.
pub fn issue_words(issue: &InputIssue) -> String {
    match issue.kind {
        IssueKind::BadLine => format!(
            "Line {}: {}",
            format_thousands(issue.line as u64),
            issue.message
        ),
        IssueKind::TooMany => format!(
            "More than {}: check them in smaller lists.",
            count_phrase(MAX_PACKAGES as u64, "package", "packages")
        ),
        IssueKind::NoPackages => "Packages is empty: type one package per line.".to_owned(),
    }
}

/// The status line after a refused press.
fn refused_words(issue: &InputIssue) -> String {
    match issue.kind {
        IssueKind::BadLine => format!(
            "Fix line {} of Packages to check.",
            format_thousands(issue.line as u64)
        ),
        IssueKind::TooMany => format!(
            "Check at most {} at a time.",
            count_phrase(MAX_PACKAGES as u64, "package", "packages")
        ),
        IssueKind::NoPackages => "Type a package in Packages to check.".to_owned(),
    }
}

/// The status line after a check: the counts by status (zero counts left out), and a pointer
/// to Results when a source did not fully answer.
pub fn done_words(report: &Report) -> String {
    let summary = &report.summary;
    let counts: Vec<String> = [
        (summary.vulnerable, "vulnerable"),
        (summary.clean, "clean"),
        (summary.not_found, "not found"),
        (summary.not_checked, "not checked"),
    ]
    .into_iter()
    .filter(|(count, _)| *count > 0)
    .map(|(count, word)| count_phrase(count as u64, word, word))
    .collect();
    let mut words = format!(
        "{}: {}.",
        count_phrase(summary.packages as u64, "package", "packages"),
        counts.join(", ")
    );
    if !every_source_answered(report) {
        words.push_str(" Not every source answered; see Results.");
    }
    words
}

/// Whether every source answered every package it was asked about, with nothing missing.
pub fn every_source_answered(report: &Report) -> bool {
    report.sources.iter().all(SourceReport::fully_answered)
}

/// The message of a check no service answered: what to do next, then each source's reason.
fn no_service_answered(report: &Report) -> String {
    let asked: Vec<&SourceReport> = report.sources.iter().filter(|s| was_asked(s)).collect();
    // Every source that was asked is blocked: the earliest end is when one may answer.
    let ends: Option<Vec<DateTime<Utc>>> = asked.iter().map(|source| block_end(source)).collect();
    let earliest = ends.and_then(|ends| ends.into_iter().min());
    let step = match earliest {
        Some(end) => format!("Try again after {}.", display_time(end)),
        None => "Check the connection, then press Check again.".to_owned(),
    };
    let reasons: Vec<String> = asked.iter().map(|source| source_phrase(source)).collect();
    format!("No service answered. {step} {}.", reasons.join("; "))
}

fn was_asked(source: &SourceReport) -> bool {
    match source.status {
        SourceStatus::NotAsked => false,
        SourceStatus::Answered { .. }
        | SourceStatus::Partial { .. }
        | SourceStatus::Failed { .. }
        | SourceStatus::RateLimited { .. } => true,
    }
}

/// When a source that answered nothing because of a block may be asked again.
fn block_end(source: &SourceReport) -> Option<DateTime<Utc>> {
    match &source.status {
        SourceStatus::RateLimited { until, .. } => Some(*until),
        SourceStatus::Answered { .. }
        | SourceStatus::Partial { .. }
        | SourceStatus::Failed { .. }
        | SourceStatus::NotAsked => None,
    }
}

#[cfg(test)]
mod tests;

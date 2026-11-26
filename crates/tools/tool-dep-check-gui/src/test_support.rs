//! Test support for the window: services that keep each request whole, and fixture reports.

use crate::services::Services;
use crate::state::DepCheckState;
use chrono::{DateTime, TimeDelta, Utc};
use dep_check_core::advisory::{AdvisoryKind, Severity, SourceName};
use dep_check_core::ecosystem::Ecosystem;
use dep_check_core::engine::CheckRequest;
use dep_check_core::http::SourceFailure;
use dep_check_core::report::{
    Attestations, BlockOrigin, Finding, Outcome, PackageReport, PackageStatus, REPORT_VERSION,
    Report, SourceReport, SourceStatus, Summary, SupplyChain, VersionFrom,
};
use rune::TextEdit;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

/// Services that keep every request whole and run none.
#[derive(Default)]
pub struct RecordingServices {
    requests: RefCell<Vec<CheckRequest>>,
}

impl RecordingServices {
    /// Every request so far, oldest first.
    pub fn requests(&self) -> Vec<CheckRequest> {
        self.requests.borrow().clone()
    }
}

impl Services for RecordingServices {
    fn check(&self, request: CheckRequest) {
        self.requests.borrow_mut().push(request);
    }
}

/// A window with recording services and `packages` in the editor.
pub fn recording(packages: &str) -> (DepCheckState, Rc<RecordingServices>) {
    let services = Rc::new(RecordingServices::default());
    let mut state = DepCheckState::new(Some(services.clone()));
    state.packages = TextEdit::new(packages);
    (state, services)
}

/// When every fixture report was made: 2026-11-26 14:00:00 UTC.
pub fn checked_at() -> DateTime<Utc> {
    DateTime::from_timestamp(1_795_701_600, 0).expect("a fixed instant")
}

/// `minutes` after [`checked_at`].
pub fn later(minutes: i64) -> DateTime<Utc> {
    checked_at() + TimeDelta::minutes(minutes)
}

pub fn source(source: SourceName, status: SourceStatus) -> SourceReport {
    SourceReport {
        source,
        status,
        requests: 1,
        failures: Vec::new(),
        details_missing: 0,
    }
}

pub fn answered(name: SourceName) -> SourceReport {
    source(
        name,
        SourceStatus::Answered {
            blocked_until: None,
        },
    )
}

pub fn rate_limited(name: SourceName, until: DateTime<Utc>) -> SourceReport {
    source(
        name,
        SourceStatus::RateLimited {
            until,
            origin: BlockOrigin::ThisRun,
        },
    )
}

pub fn failed(name: SourceName, failure: SourceFailure) -> SourceReport {
    source(name, SourceStatus::Failed { failure })
}

/// Every source answered everything.
pub fn all_answered() -> Vec<SourceReport> {
    vec![
        answered(SourceName::Osv),
        answered(SourceName::DepsDev),
        answered(SourceName::GitHub),
    ]
}

pub fn package(input: &str, version: Option<&str>, status: PackageStatus) -> PackageReport {
    let name = input.split('@').next().unwrap_or(input);
    PackageReport {
        input: input.to_owned(),
        name: name.to_owned(),
        version: version.map(str::to_owned),
        version_from: None,
        status,
        reason: None,
        findings: Vec::new(),
        supply_chain: None,
        answers: BTreeMap::new(),
    }
}

pub fn finding(id: &str, kind: AdvisoryKind) -> Finding {
    Finding {
        id: id.to_owned(),
        aliases: vec![format!("CVE-{id}")],
        kind,
        severity: Severity::Medium,
        summary: Some(format!("summary of {id}")),
        cvss: Vec::new(),
        epss: None,
        fixed_in: vec!["1.23.1".to_owned()],
        urls: Vec::new(),
        sources: vec![SourceName::Osv],
    }
}

pub fn supply_chain(license: &str) -> SupplyChain {
    SupplyChain {
        latest: true,
        deprecated: None,
        licenses: vec![license.to_owned()],
        published_at: None,
        source_repo: None,
        scorecard: None,
        attestations: Attestations {
            total: 0,
            verified: 0,
        },
    }
}

/// A report of `packages` from `sources`, its outcome and summary from the packages.
pub fn report(
    ecosystem: Ecosystem,
    sources: Vec<SourceReport>,
    packages: Vec<PackageReport>,
) -> Report {
    let statuses: Vec<PackageStatus> = packages.iter().map(|package| package.status).collect();
    Report {
        report_version: REPORT_VERSION,
        tool: "dep-check-gui 1.0.0".to_owned(),
        checked_at: checked_at(),
        ecosystem,
        outcome: Outcome::from_statuses(&statuses),
        summary: Summary::of(&packages),
        sources,
        packages,
    }
}

/// tokio 1.20.0 vulnerable, serde clean at its latest version, left-padd not found; every
/// source answered.
pub fn sample_report() -> Report {
    let mut tokio = package("tokio@1.20.0", Some("1.20.0"), PackageStatus::Vulnerable);
    tokio.findings = vec![finding("RUSTSEC-2023-0005", AdvisoryKind::Vulnerability)];
    tokio.supply_chain = Some(supply_chain("MIT"));
    let mut serde = package("serde", Some("1.0.215"), PackageStatus::Clean);
    serde.version_from = Some(VersionFrom::DepsDevLatest);
    serde.supply_chain = Some(supply_chain("MIT OR Apache-2.0"));
    let left_padd = package("left-padd", None, PackageStatus::NotFound);
    report(
        Ecosystem::Cargo,
        all_answered(),
        vec![tokio, serde, left_padd],
    )
}

/// A report in which no package was checked: every source rate limited.
pub fn all_blocked_report() -> Report {
    let mut tokio = package("tokio@1.20.0", Some("1.20.0"), PackageStatus::NotChecked);
    tokio.reason = Some("every source is rate limited".to_owned());
    report(
        Ecosystem::Cargo,
        vec![
            rate_limited(SourceName::Osv, later(60)),
            rate_limited(SourceName::DepsDev, later(30)),
            rate_limited(SourceName::GitHub, later(90)),
        ],
        vec![tokio],
    )
}

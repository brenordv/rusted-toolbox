//! Live checks the owner runs (`cargo test -p dep-check-core -- --ignored live_offline`): L9
//! installs every ecosystem's real dump and prints what it took; L10 compares offline verdicts
//! with OSV.dev's for real packages.

use crate::ecosystem::Ecosystem;
use crate::engine::{PackageAnswer, PackageKey, Resolved, RunContext, system_clock};
use crate::http::{HttpLimits, build_client};
use crate::offline::store::{self, DataStatus, StoreFiles};
use crate::offline::testing::{USER_AGENT, not_cancelled, quiet};
use crate::offline::update::{OSV_DATA_BASE, Trigger, UpdateLimits, UpdateOutcome, Updater};
use crate::sources::offline::{OfflineRead, OfflineSource};
use crate::sources::osv::{OSV_BASE, OsvClient};
use chrono::{TimeDelta, Utc};
use reqwest::header::HeaderMap;
use std::collections::BTreeSet;
use std::path::Path;
use std::time::Instant;
use url::Url;

/// Pairs taken from the stored records' `versions` lists per ecosystem.
const PAIRS_PER_ECOSYSTEM: usize = 50;

/// The GitHub live check's packages at its vulnerable and fixed versions, the NuGet and PyPI
/// names also re-cased, and a RubyGems package at two versions.
const SHAPES: [(Ecosystem, &str, &str); 12] = [
    (Ecosystem::Cargo, "tokio", "1.20.0"),
    (Ecosystem::Cargo, "tokio", "1.23.1"),
    (Ecosystem::Npm, "@babel/traverse", "7.22.0"),
    (Ecosystem::Npm, "@babel/traverse", "7.23.2"),
    (Ecosystem::Pypi, "Django", "3.2.0"),
    (Ecosystem::Pypi, "Django", "4.2.24"),
    (Ecosystem::Pypi, "dJANGO", "3.2.0"),
    (Ecosystem::Nuget, "Newtonsoft.Json", "12.0.1"),
    (Ecosystem::Nuget, "Newtonsoft.Json", "13.0.1"),
    (Ecosystem::Nuget, "NEWTONSOFT.JSON", "12.0.1"),
    (Ecosystem::Rubygems, "rails", "5.2.0"),
    (Ecosystem::Rubygems, "rails", "7.1.3"),
];

fn real_updater(dir: &Path) -> Updater {
    Updater::with_parts(
        USER_AGENT,
        Url::parse(OSV_DATA_BASE).unwrap(),
        dir.to_path_buf(),
        system_clock(),
        UpdateLimits::default(),
    )
    .unwrap()
}

#[tokio::test]
#[ignore = "downloads OSV.dev's five dumps (about 270 MB); run with --ignored live_offline"]
async fn live_offline_install_every_ecosystem() {
    let dir = tempfile::tempdir().unwrap();
    let updater = real_updater(dir.path());

    for ecosystem in Ecosystem::ALL {
        let started = Instant::now();

        let outcome = updater
            .update(ecosystem, Trigger::Manual, quiet(), not_cancelled())
            .await;

        let DataStatus::Ready { meta, file_bytes } = store::status(dir.path(), ecosystem) else {
            panic!("{ecosystem}: not ready after {outcome:?}");
        };
        eprintln!(
            "{ecosystem}: {outcome:?} in {:.1} s; archive {} bytes, store {file_bytes} bytes, {} records",
            started.elapsed().as_secs_f64(),
            meta.archive_bytes,
            meta.records,
        );
        assert!(matches!(outcome, UpdateOutcome::Installed { .. }));
    }
}

/// Up to [`PAIRS_PER_ECOSYSTEM`] (name, version) pairs from the stored records' lists.
fn sampled_pairs(dir: &Path, ecosystem: Ecosystem) -> Vec<(String, String)> {
    let conn = rusqlite::Connection::open(StoreFiles::of(dir, ecosystem).db).unwrap();
    let mut statement = conn
        .prepare("SELECT record FROM records ORDER BY id LIMIT 2000")
        .unwrap();
    let records: Vec<String> = statement
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let mut pairs = BTreeSet::new();
    for record in records {
        let value: serde_json::Value = serde_json::from_str(&record).unwrap();
        for entry in value["affected"].as_array().into_iter().flatten() {
            let Some(name) = entry["package"]["name"].as_str() else {
                continue;
            };
            for version in entry["versions"].as_array().into_iter().flatten().take(1) {
                if let Some(version) = version.as_str() {
                    pairs.insert((name.to_owned(), version.to_owned()));
                }
            }
        }
        if pairs.len() >= PAIRS_PER_ECOSYSTEM {
            break;
        }
    }
    pairs.into_iter().collect()
}

#[tokio::test]
#[ignore = "downloads OSV.dev's five dumps and asks the real OSV.dev; run with --ignored live_offline"]
async fn live_offline_verdicts_match_osv_dev() {
    let dir = tempfile::tempdir().unwrap();
    let updater = real_updater(dir.path());
    let limits = HttpLimits::default();
    let client = build_client(&limits, USER_AGENT, HeaderMap::new()).unwrap();
    let osv = OsvClient::new(client, Url::parse(OSV_BASE).unwrap(), limits).unwrap();
    let context = RunContext {
        clock: system_clock(),
        strikes: 0,
    };
    let mut mismatches = Vec::new();

    for ecosystem in Ecosystem::ALL {
        updater
            .update(ecosystem, Trigger::Manual, quiet(), not_cancelled())
            .await;
        let mut pairs = sampled_pairs(dir.path(), ecosystem);
        pairs.extend(
            SHAPES
                .iter()
                .filter(|(shape, _, _)| *shape == ecosystem)
                .map(|(_, name, version)| ((*name).to_owned(), (*version).to_owned())),
        );
        let packages: Vec<Resolved> = pairs
            .iter()
            .enumerate()
            .map(|(index, (name, version))| Resolved {
                key: PackageKey(index),
                name: name.clone(),
                version: version.clone(),
            })
            .collect();
        let OfflineRead::Run(offline) = OfflineSource::new(Some(dir.path().to_path_buf())).check(
            ecosystem,
            &packages,
            TimeDelta::days(365),
            Utc::now(),
        ) else {
            panic!("{ecosystem}: no offline data");
        };
        let online = osv.check(ecosystem, &packages, &context).await;
        let mut undecided = 0;
        for package in &packages {
            let found_online = match &online.answers[&package.key] {
                PackageAnswer::Answered { advisories } => !advisories.is_empty(),
                other => panic!(
                    "{ecosystem} {}: OSV.dev did not answer: {other:?}",
                    package.name
                ),
            };
            let found_offline = match &offline.answers[&package.key] {
                PackageAnswer::Answered { advisories } => !advisories.is_empty(),
                PackageAnswer::Failed { .. } => {
                    undecided += 1;
                    continue;
                }
                PackageAnswer::NotFound => continue,
            };
            if found_online != found_offline {
                mismatches.push(format!(
                    "{ecosystem} {}@{}: online {found_online}, offline {found_offline}",
                    package.name, package.version
                ));
            }
        }
        eprintln!(
            "{ecosystem}: {} packages compared, {undecided} undecided offline",
            packages.len()
        );
    }

    assert!(mismatches.is_empty(), "{mismatches:#?}");
}

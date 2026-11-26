//! What the engine writes to the log: one start, one end, and one line per source, with no
//! package name, address, query, or text from an answer (a `Link` header included); and a warn
//! for a rate-limit file that cannot be read or written. This is the only test in its binary,
//! so a global subscriber sees every event.

use chrono::{TimeDelta, Utc};
use dep_check_core::advisory::SourceName;
use dep_check_core::ecosystem::Ecosystem;
use dep_check_core::engine::{Bases, CheckRequest, Checker};
use dep_check_core::package::parse_specs;
use dep_check_core::rate_limits::{Change, save_changes};
use dep_check_core::test_support::{MockServer, Reply, fixed_clock};
use serde_json::json;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const MARKER: &str = "marker-48151";
const PACKAGES: [&str; 4] = [
    "zebra-pkg-one@1.0.0",
    "zebra-pkg-two@2.0.0",
    "zebra-pkg-three@3.0.0",
    "zebra-pkg-four",
];

#[derive(Clone, Default)]
struct SharedBuffer(Arc<Mutex<Vec<u8>>>);

impl Write for SharedBuffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl SharedBuffer {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

fn request() -> CheckRequest {
    CheckRequest {
        ecosystem: Ecosystem::Cargo,
        packages: parse_specs(Ecosystem::Cargo, PACKAGES).unwrap(),
    }
}

async fn run(server: &MockServer, limits_path: Option<PathBuf>) {
    Checker::with_bases(
        "dep-check 1.0.0",
        "dep-check-test/1.0",
        Bases {
            osv: server.base(),
            deps_dev: server.base(),
            github: server.base(),
        },
        limits_path,
    )
    .unwrap()
    .check(request(), fixed_clock(Utc::now()))
    .await;
}

fn engine_lines(log: &str) -> Vec<&str> {
    log.lines()
        .filter(|line| line.contains("dep_check_core"))
        .collect()
}

#[tokio::test]
async fn the_log_holds_one_line_per_source_and_no_package_or_answer_text() {
    let logs = SharedBuffer::default();
    let writer = logs.clone();
    tracing::subscriber::set_global_default(
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish(),
    )
    .unwrap();
    let server = MockServer::start();
    server.route(
        "POST",
        "/v1/querybatch",
        Reply::json(
            200,
            &json!({"results": [{"vulns": [{"id": "ID-A"}]}, {"vulns": [{"id": "ID-B"}]}, {}, {}]}),
        ),
    );
    server.route(
        "GET",
        "/v3/systems/cargo/packages/zebra-pkg-four",
        Reply::json(
            200,
            &json!({"versions": [{"versionKey": {"version": MARKER}, "isDefault": true}]}),
        ),
    );
    server.route(
        "GET",
        &format!("/v3/systems/cargo/packages/zebra-pkg-four/versions/{MARKER}"),
        Reply::json(
            200,
            &json!({
                "advisoryKeys": [{"id": format!("GHSA-{MARKER}")}],
                "relatedProjects": [{"projectKey": {"id": format!("github.com/{MARKER}/x")}, "relationType": "SOURCE_REPO"}]
            }),
        ),
    );
    server.route(
        "GET",
        &format!("/v3/advisories/GHSA-{MARKER}"),
        Reply::json(200, &json!({"title": MARKER})),
    );
    server.route(
        "GET",
        &format!("/v3/projects/github.com%2F{MARKER}%2Fx"),
        Reply::new(500, MARKER),
    );
    server.route(
        "GET",
        "/v1/vulns/ID-A",
        Reply::json(200, &json!({"id": "ID-A", "summary": MARKER})),
    );
    server.route("GET", "/v1/vulns/ID-B", Reply::new(503, MARKER));
    server.route(
        "GET",
        "/advisories",
        Reply::new(403, MARKER).with_header("retry-after", "30"),
    );
    for (name, version) in [("zebra-pkg-one", "1.0.0"), ("zebra-pkg-two", "2.0.0")] {
        server.route(
            "GET",
            &format!("/v3/systems/cargo/packages/{name}/versions/{version}"),
            Reply::json(200, &json!({"deprecatedReason": MARKER})),
        );
    }
    server.route(
        "GET",
        "/v3/systems/cargo/packages/zebra-pkg-three/versions/3.0.0",
        Reply::new(503, MARKER),
    );
    let dir = tempfile::tempdir().unwrap();
    let unreadable = dir.path().join("unreadable").join("rate-limits.json");
    std::fs::create_dir_all(unreadable.parent().unwrap()).unwrap();
    std::fs::write(&unreadable, MARKER).unwrap();
    let blocker = dir.path().join("unwritable");
    std::fs::write(&blocker, b"a file").unwrap();
    let limiting = MockServer::start();
    limiting.route(
        "POST",
        "/v1/querybatch",
        Reply::new(429, MARKER).with_header("retry-after", "30"),
    );
    limiting.route(
        "GET",
        "/advisories",
        Reply::json(200, &json!([])).with_header(
            "link",
            &format!(
                "<https://{MARKER}.example/advisories?affects=zebra-pkg-one%401.0.0>; rel=\"next\""
            ),
        ),
    );

    let saved = dir.path().join("saved").join("rate-limits.json");
    let now = Utc::now();
    save_changes(
        Some(&saved),
        &[(
            SourceName::Osv,
            Change::Blocked {
                since: now,
                until: now + TimeDelta::minutes(10),
            },
        )],
        now,
    )
    .await;

    run(&server, None).await;
    let first_run = logs.text();
    run(&server, Some(unreadable)).await;
    run(&limiting, Some(blocker.join("rate-limits.json"))).await;
    let before_saved_run = logs.text().len();
    run(&server, Some(saved)).await;
    let log = logs.text();

    let saved_run_lines: Vec<&str> = engine_lines(&log[before_saved_run..])
        .into_iter()
        .filter(|line| line.contains("source finished") && line.contains("source=\"osv\""))
        .collect();
    assert_eq!(saved_run_lines.len(), 1, "{log}");
    assert!(
        saved_run_lines[0].contains("WARN"),
        "{}",
        saved_run_lines[0]
    );
    assert!(
        saved_run_lines[0].contains("status=\"rate_limited\""),
        "{}",
        saved_run_lines[0]
    );
    assert!(
        saved_run_lines[0].contains("origin=\"saved\""),
        "{}",
        saved_run_lines[0]
    );
    assert!(
        saved_run_lines[0].contains("until=2"),
        "{}",
        saved_run_lines[0]
    );

    let first = engine_lines(&first_run);
    let count = |words: &str| first.iter().filter(|line| line.contains(words)).count();
    assert_eq!(count("check started"), 1, "{first_run}");
    assert_eq!(count("check finished"), 1, "{first_run}");
    let source_line = |source: &str| -> Vec<&&str> {
        first
            .iter()
            .filter(|line| {
                line.contains("source finished") && line.contains(&format!("source=\"{source}\""))
            })
            .collect()
    };
    assert_eq!(count("source finished"), 3, "{first_run}");
    let github_lines = source_line("github");
    assert_eq!(github_lines.len(), 1, "{first_run}");
    let github_line = github_lines[0];
    assert!(github_line.contains("WARN"), "{github_line}");
    assert!(
        github_line.contains("status=\"rate_limited\""),
        "{github_line}"
    );
    assert!(github_line.contains("until=2"), "{github_line}");
    assert!(github_line.contains("strikes=1"), "{github_line}");
    assert!(
        engine_lines(&log)
            .iter()
            .any(|line| line.contains("DEBUG")
                && line.contains("a next-page link to another address")),
        "{log}"
    );
    assert!(
        engine_lines(&log)
            .iter()
            .all(|line| !line.contains("affects=")),
        "the query in the log"
    );
    let osv_lines = source_line("osv");
    assert_eq!(osv_lines.len(), 1, "{first_run}");
    let osv_line = osv_lines[0];
    assert!(osv_line.contains("WARN"), "{osv_line}");
    assert!(osv_line.contains("status=\"answered\""), "{osv_line}");
    assert!(osv_line.contains("failed=0"), "{osv_line}");
    assert!(osv_line.contains("details_missing=1"), "{osv_line}");
    let deps_dev_lines = source_line("deps_dev");
    assert_eq!(deps_dev_lines.len(), 1, "{first_run}");
    let deps_dev_line = deps_dev_lines[0];
    assert!(deps_dev_line.contains("WARN"), "{deps_dev_line}");
    assert!(
        deps_dev_line.contains("status=\"partial\""),
        "{deps_dev_line}"
    );
    assert!(
        deps_dev_line.contains("reason=answered 503 (1 package)"),
        "{deps_dev_line}"
    );
    assert!(deps_dev_line.contains("failed=1"), "{deps_dev_line}");
    assert!(
        first
            .iter()
            .any(|line| line.contains("DEBUG") && line.contains("path_kind=\"querybatch\"")),
        "{first_run}"
    );

    let ports = [
        format!(":{}", server.addr.port()),
        format!(":{}", limiting.addr.port()),
    ];
    for line in engine_lines(&log) {
        assert!(!line.contains(MARKER), "answer text in: {line}");
        if line.contains("DEBUG") || line.contains("TRACE") {
            continue;
        }
        assert!(!line.contains("zebra-pkg"), "a package name in: {line}");
        assert!(
            !ports.iter().any(|port| line.contains(port.as_str())),
            "the address in: {line}"
        );
        assert!(
            !line.contains("v1/") && !line.contains("v3/"),
            "a request path in: {line}"
        );
    }
    let warns = |words: &str| {
        engine_lines(&log)
            .iter()
            .filter(|line| line.contains("WARN") && line.contains(words))
            .count()
    };
    assert_eq!(warns("the rate-limit file could not be read"), 1, "{log}");
    assert_eq!(
        warns("the rate-limit file could not be written"),
        1,
        "{log}"
    );
}

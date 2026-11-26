use super::*;
use chrono::Utc;
use common_cli::test_writers::{ClosedPipe, FailingDisk};
use dep_check_core::engine::Bases;
use dep_check_core::test_support::{MockServer, Reply, closed_port, fixed_clock};
use serde_json::json;
use std::io::Cursor;

const QUERYBATCH: &str = "/v1/querybatch";

/// A stdin that fails the test when read.
struct UnreadStdin;

impl Read for UnreadStdin {
    fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
        panic!("stdin was read");
    }
}

/// A stdin whose read fails.
struct FailingStdin;

impl Read for FailingStdin {
    fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("the pipe broke"))
    }
}

fn args(format: Format, packages: &[&str]) -> DepCheckArgs {
    DepCheckArgs {
        ecosystem: Ecosystem::Cargo,
        format,
        packages: packages.iter().map(|text| (*text).to_owned()).collect(),
    }
}

fn checker(osv: url::Url, deps_dev: url::Url, dir: &tempfile::TempDir) -> Checker {
    Checker::with_bases(
        "dep-check 1.0.0",
        "dep-check-test/1.0",
        Bases { osv, deps_dev },
        Some(dir.path().join("rate-limits.json")),
    )
    .unwrap()
}

fn deps<R: Read, W: Write>(stdin: R, stdout: W, checker: Checker) -> Deps<R, W> {
    Deps {
        stdin_terminal: false,
        stdin,
        stdout,
        checker,
        clock: fixed_clock(Utc::now()),
    }
}

/// A server whose OSV.dev answers every batch with `results`.
fn osv_answering(results: serde_json::Value) -> MockServer {
    let server = MockServer::start();
    server.route(
        "POST",
        QUERYBATCH,
        Reply::json(200, &json!({ "results": results })),
    );
    server
}

async fn run_text(server: &MockServer, packages: &[&str]) -> (Result<RunEnd>, String) {
    let dir = tempfile::tempdir().unwrap();
    let mut stdout = Vec::new();
    let end = run(
        args(Format::Text, packages),
        deps(
            UnreadStdin,
            &mut stdout,
            checker(server.base(), closed_port(), &dir),
        ),
    )
    .await;
    (end, String::from_utf8(stdout).unwrap())
}

async fn run_stdin(stdin: Vec<u8>) -> (Result<RunEnd>, MockServer) {
    let server = osv_answering(json!([{}]));
    let dir = tempfile::tempdir().unwrap();
    let end = run(
        args(Format::Text, &[]),
        deps(
            Cursor::new(stdin),
            Vec::new(),
            checker(server.base(), closed_port(), &dir),
        ),
    )
    .await;
    (end, server)
}

#[tokio::test]
async fn arguments_are_used_and_stdin_is_not_read() {
    let server = osv_answering(json!([{}, {}]));

    let (end, _) = run_text(&server, &["tokio@1.38.0", "serde@1.0.215"]).await;

    assert_eq!(end.unwrap(), RunEnd::Reported(ExitCode::Clean));
    let queries = server.received_at(QUERYBATCH)[0].json()["queries"].clone();
    assert_eq!(queries.as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn no_arguments_and_a_terminal_stdin_is_refused_naming_both_ways() {
    let server = MockServer::start();
    let dir = tempfile::tempdir().unwrap();
    let deps = Deps {
        stdin_terminal: true,
        ..deps(
            UnreadStdin,
            Vec::new(),
            checker(server.base(), server.base(), &dir),
        )
    };

    let end = run(args(Format::Text, &[]), deps).await;

    assert_eq!(end.unwrap(), RunEnd::Refused(NO_PACKAGES.to_owned()));
    assert!(server.received().is_empty());
}

#[tokio::test]
async fn piped_stdin_with_crlf_a_bom_and_blank_lines_parses_to_its_packages() {
    let (end, server) = run_stdin(b"\xEF\xBB\xBFtokio@1.38.0\r\n\r\n  \r\n".to_vec()).await;

    assert_eq!(end.unwrap(), RunEnd::Reported(ExitCode::Clean));
    let queries = server.received_at(QUERYBATCH)[0].json()["queries"].clone();
    assert_eq!(queries[0]["package"]["name"], "tokio");
    assert_eq!(queries[0]["version"], "1.38.0");
}

fn padded_to(size: usize) -> Vec<u8> {
    let mut stdin = b"tokio@1.38.0\n".to_vec();
    stdin.resize(size, b' ');
    stdin
}

#[tokio::test]
async fn one_mib_of_stdin_is_read() {
    let (end, _) = run_stdin(padded_to(MAX_STDIN_BYTES)).await;

    assert_eq!(end.unwrap(), RunEnd::Reported(ExitCode::Clean));
}

#[tokio::test]
async fn one_byte_past_a_mib_of_stdin_is_refused() {
    let (end, server) = run_stdin(padded_to(MAX_STDIN_BYTES + 1)).await;

    assert_eq!(
        end.unwrap(),
        RunEnd::Refused("stdin holds more than 1 MiB".to_owned())
    );
    assert!(server.received().is_empty());
}

#[tokio::test]
async fn stdin_that_is_not_utf8_is_refused() {
    let (end, _) = run_stdin(vec![b't', 0xFF, b'\n']).await;

    assert_eq!(
        end.unwrap(),
        RunEnd::Refused("stdin is not UTF-8 text".to_owned())
    );
}

#[tokio::test]
async fn a_bad_line_is_refused_with_its_number_and_nothing_is_sent() {
    let (end, server) = run_stdin(b"tokio@1.0.0\nserde\nbad name\n".to_vec()).await;

    let RunEnd::Refused(line) = end.unwrap() else {
        panic!("not refused");
    };
    assert!(line.starts_with("line 3: \"bad name\""), "{line}");
    assert!(server.received().is_empty());
}

#[tokio::test]
async fn a_bad_argument_is_refused_with_its_number() {
    let server = MockServer::start();

    let (end, _) = run_text(&server, &["serde", "bad name"]).await;

    let RunEnd::Refused(line) = end.unwrap() else {
        panic!("not refused");
    };
    assert!(line.starts_with("argument 2: "), "{line}");
}

#[tokio::test]
async fn an_empty_piped_stdin_is_refused_with_the_no_packages_line() {
    let (end, _) = run_stdin(Vec::new()).await;

    assert_eq!(end.unwrap(), RunEnd::Refused(NO_PACKAGES.to_owned()));
}

#[tokio::test]
async fn a_stdin_that_fails_to_read_fails_the_run_and_sends_nothing() {
    let server = MockServer::start();
    let dir = tempfile::tempdir().unwrap();

    let end = run(
        args(Format::Text, &[]),
        deps(
            FailingStdin,
            Vec::new(),
            checker(server.base(), server.base(), &dir),
        ),
    )
    .await;

    assert!(format!("{:#}", end.unwrap_err()).contains("Failed to read stdin"));
    assert!(server.received().is_empty());
}

#[tokio::test]
async fn every_package_clean_ends_clean_with_the_text_report() {
    let server = osv_answering(json!([{}]));

    let (end, text) = run_text(&server, &["tokio@1.38.0"]).await;

    assert_eq!(end.unwrap(), RunEnd::Reported(ExitCode::Clean));
    assert!(
        text.starts_with("dep-check 1.0.0  Cargo  1 package: 1 clean"),
        "{text}"
    );
}

fn with_one_vulnerability() -> MockServer {
    let server = osv_answering(json!([{"vulns": [{"id": "RUSTSEC-1"}]}, {}]));
    server.route(
        "GET",
        "/v1/vulns/RUSTSEC-1",
        Reply::json(200, &json!({"id": "RUSTSEC-1"})),
    );
    server
}

#[tokio::test]
async fn one_vulnerable_package_ends_vulnerable() {
    let server = with_one_vulnerability();

    let (end, _) = run_text(&server, &["tokio@1.20.0", "serde@1.0.215"]).await;

    assert_eq!(end.unwrap(), RunEnd::Reported(ExitCode::Vulnerable));
}

#[tokio::test]
async fn a_version_deps_dev_does_not_know_ends_incomplete() {
    let server = osv_answering(json!([{}, {}]));
    server.route(
        "GET",
        "/v3/systems/cargo/packages/tokio/versions/1.38.0",
        Reply::json(200, &json!({})),
    );
    server.route(
        "GET",
        "/v3/systems/cargo/packages/serde/versions/0.0.1",
        Reply::new(404, ""),
    );
    let dir = tempfile::tempdir().unwrap();

    let end = run(
        args(Format::Text, &["tokio@1.38.0", "serde@0.0.1"]),
        deps(
            UnreadStdin,
            Vec::new(),
            checker(server.base(), server.base(), &dir),
        ),
    )
    .await;

    assert_eq!(end.unwrap(), RunEnd::Reported(ExitCode::Incomplete));
}

#[tokio::test]
async fn a_name_no_service_knows_never_ends_clean() {
    let server = osv_answering(json!([{}]));
    server.route(
        "GET",
        "/v3/systems/cargo/packages/left-padd/versions/1.0.0",
        Reply::new(404, ""),
    );
    let dir = tempfile::tempdir().unwrap();

    let end = run(
        args(Format::Text, &["left-padd@1.0.0"]),
        deps(
            UnreadStdin,
            Vec::new(),
            checker(server.base(), server.base(), &dir),
        ),
    )
    .await;

    assert_eq!(end.unwrap(), RunEnd::Reported(ExitCode::Incomplete));
}

#[test]
fn a_reported_run_ends_with_the_report_s_code() {
    let result: Result<RunEnd> = Ok(RunEnd::Reported(ExitCode::Vulnerable));

    assert_eq!(exit_code_of(&result).code(), 1);
}

#[test]
fn a_refused_run_ends_with_2() {
    let result: Result<RunEnd> = Ok(RunEnd::Refused("line 1: bad".to_owned()));

    assert_eq!(exit_code_of(&result).code(), 2);
}

#[test]
fn a_failed_run_ends_with_4() {
    let result: Result<RunEnd> = Err(anyhow::anyhow!("no source answered"));

    assert_eq!(exit_code_of(&result).code(), 4);
}

#[tokio::test]
async fn one_vulnerable_and_one_not_checked_ends_vulnerable() {
    let server = osv_answering(json!([{"vulns": [{"id": "RUSTSEC-1"}]}]));
    server.route(
        "GET",
        "/v1/vulns/RUSTSEC-1",
        Reply::json(200, &json!({"id": "RUSTSEC-1"})),
    );
    server.route(
        "GET",
        "/v3/systems/cargo/packages/serde",
        Reply::new(503, ""),
    );
    let dir = tempfile::tempdir().unwrap();

    let end = run(
        args(Format::Text, &["tokio@1.20.0", "serde"]),
        deps(
            UnreadStdin,
            Vec::new(),
            checker(server.base(), server.base(), &dir),
        ),
    )
    .await;

    assert_eq!(end.unwrap(), RunEnd::Reported(ExitCode::Vulnerable));
}

#[tokio::test]
async fn both_services_down_fails_the_run_naming_both_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut stdout = Vec::new();

    let end = run(
        args(Format::Text, &["tokio@1.38.0"]),
        deps(
            UnreadStdin,
            &mut stdout,
            checker(closed_port(), closed_port(), &dir),
        ),
    )
    .await;

    let error = format!("{:#}", end.unwrap_err());
    assert!(
        error.starts_with("no source answered: OSV.dev could not be reached"),
        "{error}"
    );
    assert!(error.contains("; deps.dev could not be reached"), "{error}");
    assert!(stdout.is_empty());
}

#[tokio::test]
async fn json_writes_one_document_and_nothing_else() {
    let server = with_one_vulnerability();
    let dir = tempfile::tempdir().unwrap();
    let mut stdout = Vec::new();

    let end = run(
        args(Format::Json, &["tokio@1.20.0", "serde@1.0.215"]),
        deps(
            UnreadStdin,
            &mut stdout,
            checker(server.base(), closed_port(), &dir),
        ),
    )
    .await;

    assert_eq!(end.unwrap(), RunEnd::Reported(ExitCode::Vulnerable));
    let text = String::from_utf8(stdout).unwrap();
    assert!(text.ends_with("}\n"));
    let report: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(report["outcome"], "vulnerable");
    assert_eq!(report["packages"][0]["findings"][0]["id"], "RUSTSEC-1");
}

#[tokio::test]
async fn a_closed_pipe_on_stdout_ends_with_the_report_s_code() {
    let server = with_one_vulnerability();
    let dir = tempfile::tempdir().unwrap();

    let end = run(
        args(Format::Text, &["tokio@1.20.0", "serde@1.0.215"]),
        deps(
            UnreadStdin,
            ClosedPipe,
            checker(server.base(), closed_port(), &dir),
        ),
    )
    .await;

    assert_eq!(end.unwrap(), RunEnd::Reported(ExitCode::Vulnerable));
}

#[tokio::test]
async fn a_stdout_that_fails_another_way_fails_the_run() {
    let server = osv_answering(json!([{}]));
    let dir = tempfile::tempdir().unwrap();

    let end = run(
        args(Format::Text, &["tokio@1.38.0"]),
        deps(
            UnreadStdin,
            FailingDisk,
            checker(server.base(), closed_port(), &dir),
        ),
    )
    .await;

    assert!(format!("{:#}", end.unwrap_err()).contains("Failed to write the report"));
}

#[tokio::test]
async fn a_failed_lookup_for_the_only_package_fails_the_run_naming_deps_dev() {
    let server = MockServer::start();
    server.route(
        "GET",
        "/v3/systems/cargo/packages/serde",
        Reply::new(503, ""),
    );
    let dir = tempfile::tempdir().unwrap();

    let end = run(
        args(Format::Text, &["serde"]),
        deps(
            UnreadStdin,
            Vec::new(),
            checker(server.base(), server.base(), &dir),
        ),
    )
    .await;

    let error = format!("{:#}", end.unwrap_err());
    assert!(error.contains("deps.dev answered 503"), "{error}");
}

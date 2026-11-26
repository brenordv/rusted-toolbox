use super::*;
use crate::state::Run;
use crate::views::{CHECK_KEY, view, window_key};
use crate::{TOOL, USER_AGENT};
use common_gui_rune::test_harness::{press, runtime_with_keys};
use dep_check_core::ecosystem::Ecosystem;
use dep_check_core::engine::Bases;
use dep_check_core::package::parse_specs;
use dep_check_core::report::PackageStatus;
use dep_check_core::test_support::{MockServer, Reply};
use rune::TextEdit;
use serde_json::json;

fn breaks() -> Report {
    panic!("the check broke")
}

#[tokio::test]
async fn a_check_task_that_dies_answers_one_failure() {
    let joined = tokio::spawn(async { breaks() }).await;

    let answer = answer_of(joined);

    match answer {
        Input::Checked(Err(text)) => {
            assert!(
                text.starts_with("The check task stopped unexpectedly: "),
                "{text}"
            );
        }
        other => panic!("not a failure: {other:?}"),
    }
}

#[tokio::test]
async fn a_real_check_answers_with_its_report() {
    let server = MockServer::start();
    server.route(
        "POST",
        "/v1/querybatch",
        Reply::json(200, &json!({"results": [{}]})),
    );
    server.route(
        "GET",
        "/v3/systems/cargo/packages/serde/versions/1.0.215",
        Reply::json(200, &json!({})),
    );
    server.route("GET", "/advisories", Reply::json(200, &json!([])));
    let bases = Bases {
        osv: server.base(),
        deps_dev: server.base(),
        github: server.base(),
    };
    let checker = Arc::new(Checker::with_bases(TOOL, USER_AGENT, bases, None).unwrap());
    let request = CheckRequest {
        ecosystem: Ecosystem::Cargo,
        packages: parse_specs(Ecosystem::Cargo, ["serde@1.0.215"]).unwrap(),
    };

    let answer = answer_of(tokio::spawn(run_check(checker, request)).await);

    match answer {
        Input::Checked(Ok(report)) => {
            assert_eq!(report.packages[0].status, PackageStatus::Clean);
            assert_eq!(report.tool, TOOL);
        }
        other => panic!("not a report: {other:?}"),
    }
}

#[test]
fn a_window_with_no_services_answers_its_check_with_a_failure_at_once() {
    let mut state = DepCheckState::detached();
    state.packages = TextEdit::new("serde");
    let mut rt = runtime_with_keys(state, view, window_key, (760.0, 720.0));

    press(&mut rt, CHECK_KEY);

    match &rt.state.run {
        Run::Failed { message, .. } => {
            assert_eq!(
                message,
                &format!("The check stopped. Press Check again. {NOT_SET_UP}")
            );
        }
        Run::Idle | Run::Checking { .. } | Run::Done { .. } => panic!("the check got no answer"),
    }
}

//! What dep-check-gui writes to its log around a check: the engine's start and end lines for a
//! check that finishes, an end line with `outcome="stopped"` for a check task that dies, and a
//! line when the window closes during a check; no package name at info or above. This is the
//! only test in its binary, so a global subscriber sees every event.

use common_gui_rune::test_harness::runtime_at;
use dep_check_core::ecosystem::Ecosystem;
use dep_check_core::engine::{Bases, CheckRequest, Checker};
use dep_check_core::offline::OfflineUse;
use dep_check_core::package::parse_specs;
use dep_check_core::report::Report;
use dep_check_core::test_support::{MockServer, Reply};
use dep_check_gui::design::WINDOW_SIZE;
use dep_check_gui::services::{answer_of, run_check};
use dep_check_gui::state::{DepCheckState, Input};
use dep_check_gui::views::{on_close, view};
use dep_check_gui::{TOOL, USER_AGENT};
use rune::TextEdit;
use serde_json::json;
use std::io::Write;
use std::sync::{Arc, Mutex};

const PACKAGE: &str = "zebra-gui-pkg";

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

fn breaks() -> Report {
    panic!("the check broke")
}

#[tokio::test]
async fn a_check_logs_its_start_and_end_a_dead_one_its_stop_and_a_close_its_drop() {
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
        Reply::json(200, &json!({"results": [{}]})),
    );
    server.route(
        "GET",
        &format!("/v3/systems/cargo/packages/{PACKAGE}/versions/1.0.0"),
        Reply::json(200, &json!({})),
    );
    server.route("GET", "/advisories", Reply::json(200, &json!([])));
    let bases = Bases {
        osv: server.base(),
        deps_dev: server.base(),
        github: server.base(),
    };
    let checker = Arc::new(Checker::with_bases(TOOL, USER_AGENT, bases, None, None).unwrap());
    let request = CheckRequest {
        ecosystem: Ecosystem::Cargo,
        packages: parse_specs(Ecosystem::Cargo, [format!("{PACKAGE}@1.0.0").as_str()]).unwrap(),
        offline: OfflineUse::Off,
    };

    let finished = answer_of(tokio::spawn(run_check(checker, request)).await);
    let stopped = answer_of(tokio::spawn(async { breaks() }).await);
    let mut state = DepCheckState::detached();
    state.packages = TextEdit::new(format!("{PACKAGE}@1.0.0"));
    state.apply(Input::CheckPressed);
    let mut rt = runtime_at(state, view, WINDOW_SIZE);
    rt.update_if(|s, tx| {
        on_close(s, tx);
        false
    });
    let log = logs.text();

    assert!(matches!(finished, Input::Checked(Ok(_))), "{finished:?}");
    assert!(matches!(stopped, Input::Checked(Err(_))), "{stopped:?}");
    let lines: Vec<&str> = log.lines().collect();
    let count = |words: &str| lines.iter().filter(|line| line.contains(words)).count();
    assert_eq!(count("check started"), 1, "{log}");
    assert_eq!(count("check finished"), 2, "{log}");
    assert_eq!(count("outcome=\"stopped\""), 1, "{log}");
    assert_eq!(count("check dropped: the window closed"), 1, "{log}");
    for line in &lines {
        if line.contains(" DEBUG ") || line.contains(" TRACE ") {
            continue;
        }
        assert!(!line.contains(PACKAGE), "a package name in: {line}");
    }
}

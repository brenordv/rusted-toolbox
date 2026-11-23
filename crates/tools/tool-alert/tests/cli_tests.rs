use std::process::{Command, Output, Stdio};

const EXE: &str = env!("CARGO_BIN_EXE_alert");

fn alert(args: &[&str]) -> Output {
    Command::new(EXE)
        .args(args)
        .args(["--log-level", "error"])
        .env_remove("RUST_LOG")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("failed to run alert")
}

#[test]
fn help_prints_clap_usage_and_exits_zero() {
    let output = alert(&["--help"]);

    assert!(output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("Usage: alert"));
}

#[test]
fn no_body_and_no_gui_is_a_usage_error() {
    let output = alert(&["--title", "Done"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("<BODY>"),
        "{output:?}"
    );
}

#[test]
fn a_blank_body_exits_one_with_its_message_on_stderr_and_an_empty_stdout() {
    let output = alert(&["   "]);

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("The alert's body is blank"),
        "{output:?}"
    );
    assert!(output.stdout.is_empty(), "{output:?}");
}

#[test]
fn a_rune_tooling_flag_is_a_usage_error() {
    let output = alert(&["--tree", "tree.json", "Build finished"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("unexpected argument '--tree'"),
        "{output:?}"
    );
}

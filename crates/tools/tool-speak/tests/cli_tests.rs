use std::io::Write;
use std::process::{Command, Output, Stdio};

const EXE: &str = env!("CARGO_BIN_EXE_speak");

fn speak(args: &[&str], piped: &str) -> Output {
    let mut child = Command::new(EXE)
        .args(args)
        .env_remove("RUST_LOG")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to run speak");
    child
        .stdin
        .take()
        .expect("speak's stdin")
        .write_all(piped.as_bytes())
        .expect("failed to write speak's stdin");
    child.wait_with_output().expect("failed to wait for speak")
}

#[test]
fn help_prints_clap_usage_and_exits_zero() {
    let output = speak(&["--help"], "");

    assert!(output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("Usage: speak"));
}

#[test]
fn an_empty_piped_stdin_and_no_argument_exits_one_with_the_blank_text_message() {
    let output = speak(&[], "");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("The text to speak is blank"),
        "{output:?}"
    );
}

#[test]
fn a_blank_argument_exits_one_with_its_message_on_stderr_and_an_empty_stdout() {
    let output = speak(&["   "], "");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("The text to speak is blank"),
        "{output:?}"
    );
    assert!(output.stdout.is_empty(), "{output:?}");
}

#[test]
fn a_rune_tooling_flag_is_a_usage_error() {
    let output = speak(&["--tree", "tree.json"], "");

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("unexpected argument '--tree'"),
        "{output:?}"
    );
}

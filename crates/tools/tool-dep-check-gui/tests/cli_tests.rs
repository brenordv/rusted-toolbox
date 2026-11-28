use std::process::Command;

const EXE: &str = env!("CARGO_BIN_EXE_dep-check-gui");

fn run_tool(args: &[&str]) -> std::process::Output {
    Command::new(EXE)
        .args(args)
        .output()
        .expect("failed to spawn dep-check-gui")
}

#[test]
fn help_exits_zero_and_lists_the_shared_logging_flags() {
    let output = run_tool(&["--help"]);

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--log-level"));
    assert!(stdout.contains("--log-to-file"));
    assert!(!stdout.contains("--verbose"));
}

#[test]
fn version_exits_zero_and_prints_the_version() {
    let output = run_tool(&["--version"]);

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn a_rune_flag_is_a_usage_error() {
    let output = run_tool(&["--tree", "x"]);

    assert_eq!(output.status.code(), Some(2));
}

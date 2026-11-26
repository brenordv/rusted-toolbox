//! The dep-check binary's command line: help, usage errors, and refused input. None of these
//! runs reaches a check, so nothing is sent anywhere.

use std::process::{Command, Output, Stdio};

fn dep_check(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_dep-check"))
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("dep-check runs")
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn help_lists_the_ecosystems_and_the_exit_codes() {
    let output = dep_check(&["--help"]);

    let help = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(0));
    for word in ["cargo", "nuget", "pypi", "npm", "rubygems"] {
        assert!(help.contains(word), "{word} missing from: {help}");
    }
    for code in [
        "0 nothing found",
        "1 vulnerabilities found",
        "2 usage or input error",
        "3 some",
        "4 no service answered",
    ] {
        assert!(help.contains(code), "{code} missing from: {help}");
    }
}

#[test]
fn no_arguments_at_all_is_a_usage_error() {
    let output = dep_check(&[]);

    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn a_bad_package_argument_exits_2_with_one_line_naming_it() {
    let output = dep_check(&["-e", "cargo", "bad name"]);

    let stderr = stderr_of(&output);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(stderr.lines().count(), 1, "{stderr}");
    assert!(stderr.contains("argument 1"), "{stderr}");
    assert!(output.stdout.is_empty());
}

#[test]
fn an_empty_stdin_exits_2() {
    let output = dep_check(&["-e", "cargo"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(stderr_of(&output).contains("no packages"));
}

#[test]
fn no_flag_points_a_query_elsewhere() {
    for flag in ["--osv-url", "--base-url", "--url", "--server"] {
        let output = dep_check(&["-e", "cargo", flag, "http://127.0.0.1:9/", "serde"]);

        assert_eq!(output.status.code(), Some(2), "{flag}");
    }
}

#[test]
fn version_alone_prints_it_and_exits_0() {
    let output = dep_check(&["--version"]);

    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).starts_with("dep-check "));
}

#[test]
fn version_in_a_package_list_exits_2_and_prints_nothing_on_stdout() {
    let output = dep_check(&["-e", "cargo", "tokio@1.20.0", "--version"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(stderr_of(&output).contains("must stand alone"));
}

#[test]
fn a_dash_name_after_the_separator_is_refused_as_a_package() {
    let output = dep_check(&["-e", "cargo", "--", "--help"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr_of(&output).contains("argument 1"),
        "{}",
        stderr_of(&output)
    );
}

#[test]
fn json_with_log_lines_on_stdout_exits_2() {
    let output = dep_check(&[
        "-e",
        "cargo",
        "--format",
        "json",
        "--log-to-console",
        "serde",
    ]);

    assert_eq!(output.status.code(), Some(2));
    assert!(stderr_of(&output).starts_with("--log-to-console"));
    assert!(output.stdout.is_empty());
}

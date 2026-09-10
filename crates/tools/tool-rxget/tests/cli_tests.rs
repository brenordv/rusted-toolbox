use std::path::PathBuf;
use std::process::{Command, Output};

const EXE: &str = env!("CARGO_BIN_EXE_rxget");

fn fixture(dir: &tempfile::TempDir, name: &str, content: &[u8]) -> PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, content).unwrap();
    path
}

fn run_tool(args: &[&str]) -> Output {
    Command::new(EXE)
        .args(args)
        .output()
        .expect("failed to spawn rxget")
}

#[test]
fn glob_target_extracts_values_from_both_files() {
    let dir = tempfile::tempdir().unwrap();
    fixture(&dir, "a.log", b"user id=42 done\n");
    fixture(&dir, "b.log", b"id=7 and id=9\n");

    let pattern = format!("{}/*.log", dir.path().display());
    let output = run_tool(&["-p", r"id=(\d+)", &pattern]);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"42\n7\n9\n");
}

#[test]
fn with_filename_prefixes_each_value() {
    let dir = tempfile::tempdir().unwrap();
    let file = fixture(&dir, "a.log", b"id=42\n");

    let output = run_tool(&["-p", r"id=(\d+)", "-H", file.to_str().unwrap()]);

    assert_eq!(output.status.code(), Some(0));
    let expected = format!("{}: 42\n", file.display());
    assert_eq!(output.stdout, expected.as_bytes());
}

#[test]
fn unique_per_run_suppresses_across_files() {
    let dir = tempfile::tempdir().unwrap();
    let a = fixture(&dir, "a.log", b"id=1 id=1\n");
    let b = fixture(&dir, "b.log", b"id=1 id=2\n");

    let output = run_tool(&[
        "-p",
        r"id=(\d+)",
        "-m",
        "unique-per-run",
        a.to_str().unwrap(),
        b.to_str().unwrap(),
    ]);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"1\n2\n");
}

#[test]
fn missing_file_exits_one_but_good_values_still_print() {
    let dir = tempfile::tempdir().unwrap();
    let good = fixture(&dir, "good.log", b"id=5\n");
    let missing = dir.path().join("missing.log");

    let output = run_tool(&[
        "-p",
        r"id=(\d+)",
        missing.to_str().unwrap(),
        good.to_str().unwrap(),
    ]);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, b"5\n");
    assert!(!output.stderr.is_empty());
}

#[test]
fn invalid_pattern_exits_one_before_reading_anything() {
    let dir = tempfile::tempdir().unwrap();
    let file = fixture(&dir, "a.log", b"id=1\n");

    let output = run_tool(&["-p", r"id=(\d+", file.to_str().unwrap()]);

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
}

#[test]
fn help_exits_zero() {
    let output = run_tool(&["--help"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(!output.stdout.is_empty());
}

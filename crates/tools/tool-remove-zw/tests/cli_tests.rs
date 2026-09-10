use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

const EXE: &str = env!("CARGO_BIN_EXE_remove-zw");

fn fixture(dir: &tempfile::TempDir, name: &str, content: &[u8]) -> PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, content).unwrap();
    path
}

fn run_tool(args: &[&str]) -> Output {
    Command::new(EXE)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("failed to spawn remove-zw")
}

fn run_tool_with_stdin(args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(EXE)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn remove-zw");
    child
        .stdin
        .take()
        .expect("stdin was piped")
        .write_all(input)
        .expect("failed to write stdin");
    child.wait_with_output().expect("failed to wait")
}

#[test]
fn check_dirty_file_exits_one_with_report() {
    let dir = tempfile::tempdir().unwrap();
    let dirty = fixture(&dir, "dirty.txt", "a\u{200B}b".as_bytes());

    let output = run_tool(&["--check", dirty.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("would modify"), "stdout: {stdout}");
    assert!(stdout.contains("Check: 1 would modify"), "stdout: {stdout}");
    // Nothing written.
    assert!(!dir.path().join("dirty.cleaned.txt").exists());
}

#[test]
fn check_clean_file_exits_zero_with_summary() {
    let dir = tempfile::tempdir().unwrap();
    let clean = fixture(&dir, "clean.txt", b"nothing to strip");

    let output = run_tool(&["--check", clean.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Check: 0 would modify"), "stdout: {stdout}");
}

#[test]
fn check_with_in_place_is_a_usage_error() {
    let output = run_tool(&["--check", "--in-place", "whatever.txt"]);
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn check_missing_input_exits_two_with_diagnostic() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("absent.txt");

    let output = run_tool(&["--check", missing.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Invalid arguments"), "stderr: {stderr}");
    assert!(stderr.contains("absent.txt"), "stderr: {stderr}");
}

#[test]
fn missing_input_without_check_keeps_exit_one() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("absent.txt");

    let output = run_tool(&[missing.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Invalid arguments"), "stderr: {stderr}");
}

#[test]
fn check_bom_only_file_respects_keep_bom() {
    let dir = tempfile::tempdir().unwrap();
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(b"text");
    let bom_file = fixture(&dir, "bom.txt", &bytes);
    let path = bom_file.to_str().unwrap();

    let without = run_tool(&["--check", path]);
    assert_eq!(without.status.code(), Some(1));

    let with = run_tool(&["--check", "--keep-bom", path]);
    assert_eq!(with.status.code(), Some(0));
}

#[test]
fn check_dirty_stdin_reports_instead_of_cleaning() {
    let output = run_tool_with_stdin(&["--check"], "he\u{200B}llo".as_bytes());
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("would modify: stdin"), "stdout: {stdout}");
    assert!(!stdout.contains("hello"), "stdout: {stdout}");
}

#[test]
fn check_invalid_utf8_stdin_exits_two() {
    let output = run_tool_with_stdin(&["--check"], &[0xFF, 0x28, 0x80]);
    assert_eq!(output.status.code(), Some(2));
    assert!(!output.stderr.is_empty());
}

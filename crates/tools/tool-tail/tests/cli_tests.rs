use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

const EXE: &str = env!("CARGO_BIN_EXE_tail");

fn fixture(dir: &tempfile::TempDir, name: &str, content: &[u8]) -> PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, content).unwrap();
    path
}

fn run_tool(args: &[&str]) -> Output {
    Command::new(EXE)
        .args(args)
        .output()
        .expect("failed to spawn tail")
}

fn run_tool_with_stdin(args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(EXE)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn tail");
    child
        .stdin
        .take()
        .expect("stdin was piped")
        .write_all(input)
        .expect("failed to write stdin");
    child.wait_with_output().expect("failed to wait")
}

#[test]
fn prints_the_last_lines() {
    let dir = tempfile::tempdir().unwrap();
    let input = fixture(&dir, "in.txt", b"1\n2\n3\n4\n5\n");

    let output = run_tool(&["-n", "2", input.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"4\n5\n");
}

#[test]
fn missing_file_exits_one() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("absent.txt");

    let output = run_tool(&[missing.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(!output.stderr.is_empty());
}

#[test]
fn follow_with_only_stdin_ends_at_eof_with_success() {
    // Stdin is read once and never polls; with nothing left to follow the
    // run warns and ends with exit 0 instead of waiting forever.
    let output = run_tool_with_stdin(&["-f", "-"], b"a\nb\n");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"a\nb\n");
    assert!(!output.stderr.is_empty());
}

#[test]
fn follow_with_stdin_and_missing_file_fails_with_no_files_remaining() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("absent.log");

    let output = run_tool_with_stdin(&["-f", "-", missing.to_str().unwrap()], b"x\n");
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("x\n"), "stdout: {stdout}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no files remaining"), "stderr: {stderr}");
}

#[test]
fn quiet_and_verbose_are_positionally_last_wins() {
    let dir = tempfile::tempdir().unwrap();
    let input = fixture(&dir, "one.txt", b"x\n");
    let path = input.to_str().unwrap();

    let forced = run_tool(&["-q", "--verbose", path]);
    let stdout = String::from_utf8_lossy(&forced.stdout);
    assert!(stdout.contains("==>"), "stdout: {stdout}");

    let suppressed = run_tool(&["--verbose", "-q", path]);
    let stdout = String::from_utf8_lossy(&suppressed.stdout);
    assert!(!stdout.contains("==>"), "stdout: {stdout}");
}

#[test]
fn gnu_suffix_forms_parse_through_the_real_cli() {
    let dir = tempfile::tempdir().unwrap();
    let input = fixture(&dir, "in.txt", b"1\n2\n3\n4\n5\n");
    let path = input.to_str().unwrap();

    let kib_form = run_tool(&["-c", "4kiB", path]);
    assert_eq!(kib_form.status.code(), Some(0));
    assert_eq!(kib_form.stdout, b"1\n2\n3\n4\n5\n");

    let from_line = run_tool(&["-n", "+4", path]);
    assert_eq!(from_line.status.code(), Some(0));
    assert_eq!(from_line.stdout, b"4\n5\n");
}

#[test]
fn obsolete_minus_count_prints_the_last_lines() {
    let dir = tempfile::tempdir().unwrap();
    let input = fixture(&dir, "in.txt", b"1\n2\n3\n4\n5\n");

    let output = run_tool(&["-2", input.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"4\n5\n");
}

#[test]
fn obsolete_plus_count_starts_at_the_given_line() {
    let dir = tempfile::tempdir().unwrap();
    let input = fixture(&dir, "in.txt", b"1\n2\n3\n4\n5\n");

    let output = run_tool(&["+4", input.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"4\n5\n");
}

#[test]
fn obsolete_count_with_two_operands_stays_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let a = fixture(&dir, "a.txt", b"x\n");
    let b = fixture(&dir, "b.txt", b"y\n");

    let output = run_tool(&["-2", a.to_str().unwrap(), b.to_str().unwrap()]);
    assert_ne!(output.status.code(), Some(0));
    assert!(!output.stderr.is_empty());
}

#[test]
fn help_exits_zero() {
    let output = run_tool(&["--help"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(!output.stdout.is_empty());
}

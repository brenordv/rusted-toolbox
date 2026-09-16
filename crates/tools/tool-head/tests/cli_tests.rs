use std::path::PathBuf;
use std::process::{Command, Output};

const EXE: &str = env!("CARGO_BIN_EXE_head");

fn fixture(dir: &tempfile::TempDir, name: &str, content: &[u8]) -> PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, content).unwrap();
    path
}

fn run_tool(args: &[&str]) -> Output {
    Command::new(EXE)
        .args(args)
        .output()
        .expect("failed to spawn head")
}

#[test]
fn prints_the_first_lines() {
    let dir = tempfile::tempdir().unwrap();
    let input = fixture(&dir, "in.txt", b"1\n2\n3\n4\n5\n");

    let output = run_tool(&["-n", "3", input.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"1\n2\n3\n");
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
fn elide_form_prints_all_but_the_last_lines() {
    let dir = tempfile::tempdir().unwrap();
    let input = fixture(&dir, "in.txt", b"a\nb\nc\nd\ne\n");

    let output = run_tool(&["-n", "-2", input.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"a\nb\nc\n");
}

#[test]
fn gnu_suffix_forms_parse_through_the_real_cli() {
    let dir = tempfile::tempdir().unwrap();
    let input = fixture(&dir, "in.txt", b"a\nb\nc\nd\ne\n");
    let path = input.to_str().unwrap();

    let lowercase_k = run_tool(&["-c", "1k", path]);
    assert_eq!(lowercase_k.status.code(), Some(0));
    assert_eq!(lowercase_k.stdout, b"a\nb\nc\nd\ne\n");

    let saturating = run_tool(&["-c", "5Z", path]);
    assert_eq!(saturating.status.code(), Some(0));
    assert_eq!(saturating.stdout, b"a\nb\nc\nd\ne\n");

    let unknown_suffix = run_tool(&["-c", "5X", path]);
    assert_ne!(unknown_suffix.status.code(), Some(0));
}

#[test]
fn help_exits_zero() {
    let output = run_tool(&["--help"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(!output.stdout.is_empty());
}

#[test]
fn obsolete_count_syntax_prints_that_many_lines() {
    let dir = tempfile::tempdir().unwrap();
    let input = fixture(&dir, "in.txt", b"1\n2\n3\n4\n5\n");

    let output = run_tool(&["-3", input.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"1\n2\n3\n");
}

#[test]
fn obsolete_trailing_letters_apply_the_matching_flags() {
    let dir = tempfile::tempdir().unwrap();
    let input = fixture(&dir, "one.txt", b"x\ny\n");

    let output = run_tool(&["-1qv", input.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("==>"), "stdout: {stdout}");
}

#[test]
fn obsolete_invalid_trailing_letter_is_named_on_stderr() {
    let output = run_tool(&["-5X"]);
    // Exit 2 through the shared parse-error path; the changelog documents the
    // divergence from GNU's exit 1.
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid trailing option -- 'X'"),
        "stderr: {stderr}"
    );
}

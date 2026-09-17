use std::process::{Command, Stdio};

const EXE: &str = env!("CARGO_BIN_EXE_guid");

#[test]
fn single_guid_prints_one_line_and_exits_zero() {
    let output = Command::new(EXE).output().expect("failed to spawn guid");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout.lines().count(), 1);
    assert_eq!(stdout.trim().len(), 36);
}

#[test]
fn single_guid_with_closed_stdout_exits_zero() {
    // The reader side is dropped before the spawn, so the child's write hits
    // a pipe with no consumer and must end as a quiet success, not a panic.
    let (reader, writer) = std::io::pipe().expect("failed to create a pipe");
    drop(reader);

    let status = Command::new(EXE)
        .stdout(writer)
        .stderr(Stdio::null())
        .status()
        .expect("failed to spawn guid");

    assert_eq!(status.code(), Some(0));
}

#[test]
fn multiple_guids_with_closed_stdout_exit_zero() {
    let (reader, writer) = std::io::pipe().expect("failed to create a pipe");
    drop(reader);

    let status = Command::new(EXE)
        .args(["-m", "5"])
        .stdout(writer)
        .stderr(Stdio::null())
        .status()
        .expect("failed to spawn guid");

    assert_eq!(status.code(), Some(0));
}

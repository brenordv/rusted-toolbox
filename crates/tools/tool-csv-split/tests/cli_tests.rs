use std::process::{Command, Stdio};

const EXE: &str = env!("CARGO_BIN_EXE_csv-split");

#[test]
fn split_with_closed_stdout_exits_zero_and_writes_the_parts() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.txt");
    std::fs::write(&input, "l1\nl2\nl3\nl4\n").unwrap();
    let outdir = dir.path().join("out");

    // The reader side is dropped before the spawn, so every stdout write
    // (progress feedback, elapsed-time pair) hits a pipe with no consumer.
    // The split itself goes to files and must still complete with exit 0.
    let (reader, writer) = std::io::pipe().expect("failed to create a pipe");
    drop(reader);

    let status = Command::new(EXE)
        .args([
            "-f",
            input.to_str().unwrap(),
            "-o",
            outdir.to_str().unwrap(),
            "-l",
            "2",
            "-i",
            "1",
        ])
        .stdout(writer)
        .stderr(Stdio::null())
        .status()
        .expect("failed to spawn csv-split");

    assert_eq!(status.code(), Some(0));
    assert_eq!(
        std::fs::read_to_string(outdir.join("split_input_1.txt")).unwrap(),
        "l1\nl2\n"
    );
    assert_eq!(
        std::fs::read_to_string(outdir.join("split_input_2.txt")).unwrap(),
        "l3\nl4\n"
    );
}

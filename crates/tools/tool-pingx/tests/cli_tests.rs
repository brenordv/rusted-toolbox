use std::process::Command;

fn get_pingx_binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_pingx")
}

#[test]
fn output_template_without_tags_should_fail() {
    let output = Command::new(get_pingx_binary_path())
        .arg("localhost")
        .arg("--output")
        .arg("hello world")
        .output()
        .expect("failed to run pingx");
    assert!(
        !output.status.success(),
        "pingx should fail with invalid template"
    );
}

#[test]
fn invalid_count_should_fail() {
    let output = Command::new(get_pingx_binary_path())
        .arg("127.0.0.1")
        .arg("--count")
        .arg("0")
        .output()
        .expect("failed to run pingx");
    assert!(!output.status.success(), "--count 0 must fail");
}

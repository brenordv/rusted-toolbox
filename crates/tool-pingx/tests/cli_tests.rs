use std::process::Command;

fn get_pingx_binary_path() -> String {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let target_dir = format!("{}/../../target", manifest_dir);
    let debug_path = format!("{}/debug/pingx.exe", target_dir);
    let release_path = format!("{}/release/pingx.exe", target_dir);
    if std::path::Path::new(&release_path).exists() {
        release_path
    } else {
        debug_path
    }
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

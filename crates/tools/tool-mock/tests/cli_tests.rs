use std::process::Command;

fn mock(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_mock"))
        .args(args)
        .output()
        .expect("the mock binary runs")
}

#[test]
fn a_password_length_over_the_cap_exits_1_and_prints_nothing() {
    let output = mock(&["internet.password", "-n", "2000000"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("length (2000000) must be <= 1048576")
    );
}

#[test]
fn a_password_length_under_the_cap_prints_that_many_characters() {
    let output = mock(&["internet.password", "-n", "32"]);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim_end().len(),
        32
    );
}

use std::process::{Command, Output};

const EXE: &str = env!("CARGO_BIN_EXE_eh-gui");

fn eh_gui(args: &[&str]) -> Output {
    Command::new(EXE)
        .args(args)
        .env_remove("RUST_LOG")
        .output()
        .expect("failed to run eh-gui")
}

#[test]
fn help_prints_clap_usage_and_exits_zero() {
    let output = eh_gui(&["--help"]);

    assert!(output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("Usage: eh-gui"));
}

#[test]
fn a_rune_tooling_flag_is_refused_by_clap() {
    let output = eh_gui(&["--tree", "tree.json"]);

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("unexpected argument '--tree'"),
        "{output:?}"
    );
}

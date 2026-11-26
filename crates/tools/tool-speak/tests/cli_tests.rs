use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use tempfile::{TempDir, tempdir};

const EXE: &str = env!("CARGO_BIN_EXE_speak");

/// Runs speak with `home` as the user's home folder (`USERPROFILE` on Windows, `HOME`
/// elsewhere), so a run never reads or moves the settings of the user running the tests.
fn speak_in(home: &Path, args: &[&str], piped: &str) -> Output {
    let mut child = Command::new(EXE)
        .args(args)
        .env_remove("RUST_LOG")
        .env("HOME", home)
        .env("USERPROFILE", home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to run speak");
    child
        .stdin
        .take()
        .expect("speak's stdin")
        .write_all(piped.as_bytes())
        .expect("failed to write speak's stdin");
    child.wait_with_output().expect("failed to wait for speak")
}

fn speak(args: &[&str], piped: &str) -> Output {
    let home = tempdir().expect("a temp home folder");
    speak_in(home.path(), args, piped)
}

fn home_with_old_settings(settings: &str) -> TempDir {
    let home = tempdir().expect("a temp home folder");
    let old = home.path().join(".rusted-toolbox").join("tool-speak");
    fs::create_dir_all(&old).expect("the old settings folder");
    fs::write(old.join("config.json"), settings).expect("the old settings file");
    home
}

#[test]
fn settings_in_the_old_folder_move_to_speaks_folder_at_start_with_a_warning() {
    let settings = "{\"url\":\"http://old-host:10157/v1/audio/speech\"}";
    let home = home_with_old_settings(settings);

    let output = speak_in(home.path(), &["   "], "");

    let toolbox = home.path().join(".rusted-toolbox");
    assert_eq!(
        fs::read_to_string(toolbox.join("speak").join("config.json")).unwrap(),
        settings
    );
    assert!(!toolbox.join("tool-speak").join("config.json").exists());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("WARN"), "{stderr}");
    assert!(stderr.contains("tool-speak"), "{stderr}");
}

#[test]
fn help_prints_clap_usage_and_exits_zero() {
    let output = speak(&["--help"], "");

    assert!(output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("Usage: speak"));
}

#[test]
fn help_lists_the_gui_flag() {
    let output = speak(&["--help"], "");

    assert!(
        String::from_utf8_lossy(&output.stdout).contains("-g, --gui"),
        "{output:?}"
    );
}

#[test]
fn a_rune_tooling_flag_beside_gui_is_a_usage_error_and_opens_no_window() {
    let output = speak(&["--gui", "--tree", "tree.json"], "");

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("unexpected argument '--tree'"),
        "{output:?}"
    );
}

#[test]
fn an_empty_piped_stdin_and_no_argument_exits_one_with_the_blank_text_message() {
    let output = speak(&[], "");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("The text to speak is blank"),
        "{output:?}"
    );
}

#[test]
fn a_blank_argument_exits_one_with_its_message_on_stderr_and_an_empty_stdout() {
    let output = speak(&["   "], "");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("The text to speak is blank"),
        "{output:?}"
    );
    assert!(output.stdout.is_empty(), "{output:?}");
}

#[test]
fn a_rune_tooling_flag_is_a_usage_error() {
    let output = speak(&["--tree", "tree.json"], "");

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("unexpected argument '--tree'"),
        "{output:?}"
    );
}

use super::*;
use crate::cli_utils::Commands;
use crate::interactive::scripted::{Reply, ScriptedPrompter};
use rstest::rstest;
use std::path::Path;

const PROFILE_CONFIG: &str = r#"{"tool":"eh","schema_version":1,"target":{"namespace":"orders.servicebus.windows.net","event_hub":"orders","auth":{"method":"credential","credential":{"kind":"developer-tools"}}}}"#;

fn write_profile(root: &Path, name: &str, config: &str) {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("config.json"), config).unwrap();
}

fn terminals(stdin: bool, stdout: bool, stderr: bool) -> Terminals {
    Terminals {
        stdin,
        stdout,
        stderr,
    }
}

#[test]
fn the_menu_opens_for_no_arguments_in_a_terminal() {
    assert!(wanted(1, terminals(true, true, true)));
}

#[rstest]
#[case::an_argument(2, terminals(true, true, true))]
#[case::stdin_piped(1, terminals(false, true, true))]
#[case::stdout_piped(1, terminals(true, false, true))]
#[case::stderr_redirected(1, terminals(true, true, false))]
fn the_menu_stays_shut_with_arguments_or_a_stream_that_is_not_a_terminal(
    #[case] argument_count: usize,
    #[case] streams: Terminals,
) {
    assert!(!wanted(argument_count, streams));
}

#[tokio::test]
async fn a_confirmed_menu_gives_the_parsed_command_and_does_not_boot() {
    let root = tempfile::tempdir().unwrap();
    write_profile(root.path(), "orders-dev", PROFILE_CONFIG);
    let mut prompter = ScriptedPrompter::new([
        Reply::Pick(0),
        Reply::Pick(0),
        Reply::Type("eh-local"),
        Reply::Pick(0),
        Reply::Yes,
    ]);
    let mut booted = false;

    let opened = open(
        &mut prompter,
        || Ok(Workspace::at(root.path().to_path_buf())),
        &mut Vec::new(),
        || booted = true,
    )
    .await
    .unwrap();

    let MenuOutcome::Run(args) = opened else {
        panic!("the menu did not build a run");
    };
    let Commands::Read(read) = args.command else {
        panic!("not a read");
    };
    assert_eq!(read.data.profile.as_deref(), Some("orders-dev"));
    assert!(!booted);
}

#[tokio::test]
async fn a_failing_prompt_boots_logging_before_the_error_returns() {
    let root = tempfile::tempdir().unwrap();
    write_profile(root.path(), "orders-dev", PROFILE_CONFIG);
    let mut prompter = ScriptedPrompter::new([Reply::Broken]);
    let mut booted = false;

    let failed = open(
        &mut prompter,
        || Ok(Workspace::at(root.path().to_path_buf())),
        &mut Vec::new(),
        || booted = true,
    )
    .await
    .unwrap_err();

    assert_eq!(format!("{failed:#}"), "the terminal went away");
    assert!(booted);
}

#[tokio::test]
async fn a_workspace_that_cannot_be_found_leaves_the_menu_shut_with_the_reason() {
    let mut prompter = ScriptedPrompter::new([]);
    let mut booted = false;

    let opened = open(
        &mut prompter,
        || Err(anyhow::anyhow!("No home folder\u{7}")),
        &mut Vec::new(),
        || booted = true,
    )
    .await
    .unwrap();

    assert!(
        matches!(&opened, MenuOutcome::Unavailable(line) if line == "No home folder\\u{7}"),
        "{opened:?}"
    );
    assert!(!booted);
}

#[tokio::test]
async fn a_workspace_that_cannot_be_listed_leaves_the_menu_shut_with_the_reason() {
    let root = tempfile::tempdir().unwrap();
    let not_a_folder = root.path().join("profiles");
    std::fs::write(&not_a_folder, "a file").unwrap();
    let mut prompter = ScriptedPrompter::new([]);

    let opened = open(
        &mut prompter,
        || Ok(Workspace::at(not_a_folder)),
        &mut Vec::new(),
        || {},
    )
    .await
    .unwrap();

    assert!(
        matches!(&opened, MenuOutcome::Unavailable(line) if line.starts_with("Failed to list the workspace")),
        "{opened:?}"
    );
}

#[tokio::test]
async fn gathering_splits_the_profiles_that_load_from_the_ones_that_cannot() {
    let root = tempfile::tempdir().unwrap();
    write_profile(root.path(), "good", PROFILE_CONFIG);
    write_profile(root.path(), "bad", r#"{"tool":"eh","schema_version":"x"}"#);

    let context = gather(|| Ok(Workspace::at(root.path().to_path_buf())))
        .await
        .unwrap();

    let ready: Vec<&str> = context
        .profiles
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(ready, ["good"]);
    assert_eq!(context.cannot_load, ["bad"]);
}

#[test]
fn arguments_eh_cannot_parse_are_the_menus_error() {
    let tokens = ["eh", "read", "--no-such-flag"].map(OsString::from);

    let failed = parse(tokens.to_vec()).unwrap_err();

    assert!(
        failed
            .to_string()
            .starts_with("The menu built arguments eh could not parse"),
        "{failed:#}"
    );
}

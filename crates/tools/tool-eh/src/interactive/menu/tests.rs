use super::*;
use crate::cli_utils::{CliArgs, Commands, FormatArg};
use crate::interactive::scripted::{Asked, Reply, ScriptedPrompter};
use crate::send_cli::DEFAULT_SEND_COUNT;
use clap::Parser;
use eh_persistence::config::{AuthMethodConfig, CredentialKindConfig, TargetConfig};
use rstest::rstest;

const NAMESPACE: &str = "orders.servicebus.windows.net";
const HUB: &str = "orders";

fn config(namespace: Option<&str>, hub: Option<&str>) -> ToolConfig {
    ToolConfig::new(TargetConfig {
        namespace: namespace.map(str::to_owned),
        event_hub: hub.map(str::to_owned),
        consumer_group: "eh-local".to_owned(),
        auth: AuthMethodConfig::Credential {
            credential: CredentialKindConfig::DeveloperTools,
        },
    })
}

fn named() -> ToolConfig {
    config(Some(NAMESPACE), Some(HUB))
}

fn context(profiles: Vec<(&str, ToolConfig)>, cannot_load: &[&str]) -> MenuContext {
    MenuContext {
        root: PathBuf::from("workspace"),
        profiles: profiles
            .into_iter()
            .map(|(name, config)| (name.to_owned(), config))
            .collect(),
        cannot_load: cannot_load.iter().map(|name| (*name).to_owned()).collect(),
    }
}

fn one(name: &str, config: ToolConfig) -> MenuContext {
    context(vec![(name, config)], &[])
}

/// What one menu run gave back, what it asked, and what it printed.
struct Ran {
    outcome: anyhow::Result<MenuOutcome<Vec<OsString>>>,
    asked: Vec<Asked>,
    printed: String,
}

fn run_menu(context: &MenuContext, replies: impl IntoIterator<Item = Reply>) -> Ran {
    let mut prompter = ScriptedPrompter::new(replies);
    let mut out = Vec::new();
    let outcome = run(&mut prompter, context, &mut out);
    Ran {
        outcome,
        asked: std::mem::take(&mut prompter.asked),
        printed: String::from_utf8(out).unwrap(),
    }
}

fn tokens(ran: Ran) -> Vec<String> {
    match ran.outcome.unwrap() {
        MenuOutcome::Run(tokens) => tokens
            .into_iter()
            .map(|token| token.into_string().unwrap())
            .collect(),
        other => panic!("the menu did not build a run: {other:?}"),
    }
}

fn parsed(tokens: &[String]) -> Commands {
    CliArgs::try_parse_from(tokens).unwrap().command
}

fn read_replies() -> Vec<Reply> {
    vec![
        Reply::Pick(0),
        Reply::Pick(0),
        Reply::Type("eh-local"),
        Reply::Pick(0),
        Reply::Yes,
    ]
}

fn replies_stopped_at(position: usize, stop: Reply) -> Vec<Reply> {
    let mut replies: Vec<Reply> = read_replies().into_iter().take(position).collect();
    replies.push(stop);
    replies
}

#[test]
fn a_read_with_the_defaults_names_the_profile_and_consumer_group() {
    let ran = run_menu(&one("orders-dev", named()), read_replies());

    assert_eq!(
        tokens(ran),
        [
            "eh",
            "read",
            "--profile",
            "orders-dev",
            "--consumer-group=eh-local"
        ]
    );
}

#[test]
fn a_read_review_names_the_target_and_the_same_command() {
    let ran = run_menu(&one("orders-dev", named()), read_replies());

    assert_eq!(
        ran.printed,
        "Reads from: profile orders-dev, namespace orders.servicebus.windows.net, event hub \
         orders\nSame as: eh read --profile orders-dev --consumer-group=eh-local\n"
    );
}

#[test]
fn a_read_prefills_the_saved_consumer_group_and_confirms_with_yes_on_enter() {
    let ran = run_menu(&one("orders-dev", named()), read_replies());

    assert_eq!(
        ran.asked[2],
        Asked::Text {
            prompt: "Consumer group:".to_owned(),
            initial: "eh-local".to_owned(),
        }
    );
    assert_eq!(
        ran.asked[4],
        Asked::Confirm {
            prompt: "Start reading?".to_owned(),
            default: true,
        }
    );
}

#[test]
fn a_read_parses_as_the_read_the_menu_meant() {
    let ran = run_menu(&one("orders-dev", named()), read_replies());

    let Commands::Read(args) = parsed(&tokens(ran)) else {
        panic!("not a read");
    };
    assert_eq!(args.data.profile.as_deref(), Some("orders-dev"));
    assert_eq!(args.consumer_group.as_deref(), Some("eh-local"));
    assert!(!args.ignore_checkpoint);
}

#[test]
fn starting_from_the_earliest_event_ignores_the_checkpoint() {
    let ran = run_menu(
        &one("p", named()),
        [
            Reply::Pick(0),
            Reply::Pick(0),
            Reply::Type("eh-local"),
            Reply::Pick(1),
            Reply::Yes,
        ],
    );

    let Commands::Read(args) = parsed(&tokens(ran)) else {
        panic!("not a read");
    };
    assert!(args.ignore_checkpoint);
}

#[test]
fn a_profile_that_ignores_the_checkpoint_shows_it_and_asks_nothing_about_it() {
    let mut ignoring = named();
    ignoring.capture.ignore_checkpoint = true;

    let ran = run_menu(
        &one("p", ignoring),
        [
            Reply::Pick(0),
            Reply::Pick(0),
            Reply::Type("eh-local"),
            Reply::Yes,
        ],
    );

    assert!(
        ran.printed
            .starts_with("Start from: Earliest event (set in the profile)\n"),
        "{}",
        ran.printed
    );
    assert_eq!(
        tokens(ran),
        ["eh", "read", "--profile", "p", "--consumer-group=eh-local"]
    );
}

#[test]
fn a_send_names_the_profiles_destination_as_flags_and_passes_no_yes() {
    let ran = run_menu(
        &one("p", named()),
        [
            Reply::Pick(0),
            Reply::Pick(1),
            Reply::Pick(0),
            Reply::Type("{\"id\": 1}"),
            Reply::Type("500"),
            Reply::Type("1000"),
            Reply::No,
            Reply::Yes,
        ],
    );

    assert_eq!(
        tokens(ran),
        [
            "eh",
            "send",
            "--profile",
            "p",
            "--namespace=orders.servicebus.windows.net",
            "--entity-path=orders",
            "--body={\"id\": 1}",
            "--count=500",
            "--interval-ms=1000"
        ]
    );
}

#[test]
fn an_empty_count_sends_one_event_without_a_count_flag() {
    let ran = run_menu(
        &one("p", named()),
        [
            Reply::Pick(0),
            Reply::Pick(1),
            Reply::Pick(0),
            Reply::Type("ping"),
            Reply::Type(""),
            Reply::Type(""),
            Reply::No,
            Reply::Yes,
        ],
    );

    assert_eq!(
        ran.asked.last(),
        Some(&Asked::Confirm {
            prompt: "Send 1 event to orders.servicebus.windows.net/orders?".to_owned(),
            default: false,
        })
    );
    let Commands::Send(args) = parsed(&tokens(ran)) else {
        panic!("not a send");
    };
    assert_eq!(args.count, DEFAULT_SEND_COUNT);
}

#[test]
fn a_typed_body_with_a_line_break_is_refused() {
    let ran = run_menu(
        &one("p", named()),
        [
            Reply::Pick(0),
            Reply::Pick(1),
            Reply::Pick(0),
            Reply::Type("{\n}"),
            Reply::Esc,
        ],
    );

    assert_eq!(
        ran.asked.last(),
        Some(&Asked::Rejected {
            prompt: "Body text (one line):".to_owned(),
            message: "The body has line breaks or characters that can't be shown; pick From a \
                      file"
                .to_owned(),
        })
    );
}

#[test]
fn a_send_confirm_names_the_hub_and_defaults_to_no() {
    let ran = run_menu(
        &one("p", named()),
        [
            Reply::Pick(0),
            Reply::Pick(1),
            Reply::Pick(0),
            Reply::Type("ping"),
            Reply::Type("500"),
            Reply::Type(""),
            Reply::No,
            Reply::No,
        ],
    );

    assert_eq!(ran.outcome.unwrap(), MenuOutcome::Cancelled);
    assert_eq!(
        ran.asked.last(),
        Some(&Asked::Confirm {
            prompt: "Send 500 events to orders.servicebus.windows.net/orders?".to_owned(),
            default: false,
        })
    );
}

fn send_typing_both_names() -> Vec<Reply> {
    vec![
        Reply::Pick(0),
        Reply::Pick(1),
        Reply::Type("not a host"),
        Reply::Type(NAMESPACE),
        Reply::Type(HUB),
        Reply::Pick(1),
        Reply::Type("event.json"),
        Reply::Type("1"),
        Reply::Type(""),
        Reply::Yes,
        Reply::Yes,
    ]
}

#[test]
fn a_send_from_a_profile_without_a_namespace_asks_for_both_names() {
    let ran = run_menu(&one("p", config(None, Some(HUB))), send_typing_both_names());

    assert_eq!(
        ran.asked[2..5],
        [
            Asked::Text {
                prompt: "Namespace (e.g. my-ns.servicebus.windows.net):".to_owned(),
                initial: String::new(),
            },
            Asked::Rejected {
                prompt: "Namespace (e.g. my-ns.servicebus.windows.net):".to_owned(),
                message: "Invalid Event Hubs namespace: expected a host name such as \
                          '<name>.servicebus.windows.net'"
                    .to_owned(),
            },
            Asked::Text {
                prompt: "Event hub (e.g. orders):".to_owned(),
                initial: String::new(),
            },
        ]
    );
}

#[test]
fn a_send_with_typed_names_passes_them_and_the_options_picked() {
    let ran = run_menu(&one("p", config(None, Some(HUB))), send_typing_both_names());

    assert_eq!(
        tokens(ran),
        [
            "eh",
            "send",
            "--profile",
            "p",
            "--namespace=orders.servicebus.windows.net",
            "--entity-path=orders",
            "--body-file=event.json",
            "--count=1",
            "--template"
        ]
    );
}

#[test]
fn a_send_with_typed_names_parses_with_both_names_and_no_yes() {
    let ran = run_menu(
        &one("p", config(None, None)),
        [
            Reply::Pick(0),
            Reply::Pick(1),
            Reply::Type(NAMESPACE),
            Reply::Type(HUB),
            Reply::Pick(0),
            Reply::Type("ping"),
            Reply::Type("1"),
            Reply::Type(""),
            Reply::No,
            Reply::Yes,
        ],
    );

    let Commands::Send(args) = parsed(&tokens(ran)) else {
        panic!("not a send");
    };
    assert_eq!(args.target.namespace.as_deref(), Some(NAMESPACE));
    assert_eq!(args.target.event_hub.as_deref(), Some(HUB));
    assert!(!args.yes);
}

#[test]
fn an_export_passes_the_format_and_metadata_picked() {
    let ran = run_menu(
        &one("p", named()),
        [
            Reply::Pick(0),
            Reply::Pick(2),
            Reply::Pick(1),
            Reply::No,
            Reply::Yes,
        ],
    );

    assert_eq!(
        tokens(ran),
        [
            "eh",
            "export",
            "--profile",
            "p",
            "--export-format=csv",
            "--include-metadata=false"
        ]
    );
}

#[test]
fn an_export_starts_from_the_saved_format_and_parses() {
    let mut saved = named();
    saved.storage.export.format = ExportFormat::Json;

    let ran = run_menu(
        &one("p", saved),
        [
            Reply::Pick(0),
            Reply::Pick(2),
            Reply::Pick(2),
            Reply::Yes,
            Reply::Yes,
        ],
    );

    assert_eq!(
        ran.asked[2],
        Asked::Select {
            prompt: "Format:".to_owned(),
            options: vec!["txt".to_owned(), "csv".to_owned(), "json".to_owned()],
            start: 2,
        }
    );
    let Commands::Export(args) = parsed(&tokens(ran)) else {
        panic!("not an export");
    };
    assert_eq!(args.format, Some(FormatArg::Json));
    assert_eq!(args.include_metadata, Some(true));
}

#[rstest]
fn esc_at_any_prompt_cancels(#[values(0, 1, 2, 3, 4)] position: usize) {
    let ran = run_menu(&one("p", named()), replies_stopped_at(position, Reply::Esc));

    assert_eq!(ran.outcome.unwrap(), MenuOutcome::Cancelled);
}

#[rstest]
fn ctrl_c_at_any_prompt_interrupts(#[values(0, 1, 2, 3, 4)] position: usize) {
    let ran = run_menu(
        &one("p", named()),
        replies_stopped_at(position, Reply::CtrlC),
    );

    assert_eq!(ran.outcome.unwrap(), MenuOutcome::Interrupted);
}

#[rstest]
fn a_terminal_that_cannot_be_read_ends_the_menu(#[values(0, 3)] position: usize) {
    let ran = run_menu(
        &one("p", named()),
        replies_stopped_at(position, Reply::NoTerminal),
    );

    assert_eq!(
        ran.outcome.unwrap(),
        MenuOutcome::Unavailable(CANNOT_READ_KEYS.to_owned())
    );
}

#[test]
fn no_at_the_confirm_cancels() {
    let ran = run_menu(&one("p", named()), replies_stopped_at(4, Reply::No));

    assert_eq!(ran.outcome.unwrap(), MenuOutcome::Cancelled);
}

#[test]
fn a_failing_prompt_ends_the_menu_with_its_error() {
    let ran = run_menu(&one("p", named()), [Reply::Pick(0), Reply::Broken]);

    assert_eq!(
        format!("{:#}", ran.outcome.unwrap_err()),
        "the terminal went away"
    );
}

#[test]
fn a_choice_that_was_not_offered_is_an_error() {
    let ran = run_menu(&one("p", named()), [Reply::Pick(5)]);

    assert_eq!(
        format!("{:#}", ran.outcome.unwrap_err()),
        "The menu got choice 6 of a 1-item list"
    );
}

#[test]
fn a_consumer_group_that_looks_like_a_flag_stays_a_value() {
    let ran = run_menu(
        &one("p", named()),
        [
            Reply::Pick(0),
            Reply::Pick(0),
            Reply::Type("-x"),
            Reply::Pick(0),
            Reply::Yes,
        ],
    );

    let Commands::Read(args) = parsed(&tokens(ran)) else {
        panic!("not a read");
    };
    assert_eq!(args.consumer_group.as_deref(), Some("-x"));
}

#[test]
fn a_body_that_looks_like_a_flag_stays_a_value() {
    let ran = run_menu(
        &one("p", named()),
        [
            Reply::Pick(0),
            Reply::Pick(1),
            Reply::Pick(0),
            Reply::Type("--yes"),
            Reply::Type("1"),
            Reply::Type(""),
            Reply::No,
            Reply::Yes,
        ],
    );

    let Commands::Send(args) = parsed(&tokens(ran)) else {
        panic!("not a send");
    };
    assert_eq!(args.body.map(|body| body.0).as_deref(), Some("--yes"));
    assert!(!args.yes);
}

#[test]
fn a_saved_consumer_group_with_control_characters_is_not_prefilled() {
    let mut hostile = named();
    hostile.target.consumer_group = "eh\u{1b}[31m".to_owned();

    let ran = run_menu(
        &one("p", hostile),
        [
            Reply::Pick(0),
            Reply::Pick(0),
            Reply::Type("eh-local"),
            Reply::Pick(0),
            Reply::Yes,
        ],
    );

    assert_eq!(
        ran.asked[2],
        Asked::Text {
            prompt: "Consumer group (the saved value has characters that can't be shown):"
                .to_owned(),
            initial: String::new(),
        }
    );
}

#[test]
fn a_hostile_target_is_escaped_in_the_options_and_the_review() {
    let hostile = config(Some("evil\u{1b}]52;c;x\u{7}"), Some(HUB));

    let ran = run_menu(&one("p", hostile), read_replies());

    let Asked::Select { options, .. } = &ran.asked[0] else {
        panic!("the first prompt is not the profile list");
    };
    assert_eq!(
        options[0],
        "p  namespace evil\\u{1b}]52;c;x\\u{7}, event hub orders"
    );
    assert!(!ran.printed.contains('\u{1b}'), "{}", ran.printed);
    assert!(
        ran.printed.contains("evil\\u{1b}]52;c;x\\u{7}"),
        "{}",
        ran.printed
    );
}

#[test]
fn profile_names_are_padded_to_line_up_their_targets() {
    let ran = run_menu(
        &context(vec![("a", named()), ("billing", named())], &[]),
        read_replies(),
    );

    let Asked::Select { options, .. } = &ran.asked[0] else {
        panic!("the first prompt is not the profile list");
    };
    assert_eq!(
        options,
        &[
            "a        namespace orders.servicebus.windows.net, event hub orders",
            "billing  namespace orders.servicebus.windows.net, event hub orders",
        ]
    );
}

#[test]
fn an_empty_workspace_offers_nothing_and_says_how_to_save_a_profile() {
    let ran = run_menu(&context(Vec::new(), &[]), []);

    assert_eq!(
        ran.outcome.unwrap(),
        MenuOutcome::Unavailable(
            "No profiles in 'workspace' yet. Save one with: eh read --profile NAME \
             --save-config <target flags>"
                .to_owned()
        )
    );
}

#[test]
fn a_workspace_where_nothing_loads_offers_nothing() {
    let ran = run_menu(&context(Vec::new(), &["scratch"]), []);

    assert_eq!(
        ran.outcome.unwrap(),
        MenuOutcome::Unavailable(NONE_CAN_LOAD.to_owned())
    );
}

#[rstest]
#[case::one(
    &["scratch"],
    "Skipped 1 profile that can't load (scratch); eh profile list shows why.\n"
)]
#[case::two(
    &["old", "scratch"],
    "Skipped 2 profiles that can't load (old, scratch); eh profile list shows why.\n"
)]
fn profiles_that_cannot_load_are_named_and_not_offered(
    #[case] cannot_load: &[&str],
    #[case] expected: &str,
) {
    let ran = run_menu(&context(vec![("p", named())], cannot_load), [Reply::Esc]);

    assert_eq!(ran.printed, expected);
    let Asked::Select { options, .. } = &ran.asked[0] else {
        panic!("the first prompt is not the profile list");
    };
    assert_eq!(options.len(), 1);
}

#[rstest]
#[case::zero("0")]
#[case::above_the_cap("1000001")]
#[case::not_a_number("ten")]
fn the_count_check_refuses_what_the_cli_refuses(#[case] input: &str) {
    assert_eq!(
        count_check(input),
        Err("Type a whole number from 1 to 1000000".to_owned())
    );
}

#[rstest]
#[case::empty_for_one("")]
#[case::one("1")]
#[case::the_cap("1000000")]
#[case::padded(" 5 ")]
fn the_count_check_takes_empty_or_one_to_the_cap(#[case] input: &str) {
    assert_eq!(count_check(input), Ok(()));
}

#[rstest]
#[case::zero("0")]
#[case::above_the_cap("3600001")]
#[case::not_a_number("soon")]
fn the_pacing_check_refuses_what_the_cli_refuses(#[case] input: &str) {
    assert_eq!(
        interval_check(input),
        Err("Type a whole number from 1 to 3600000".to_owned())
    );
}

#[rstest]
#[case::empty_for_batches("")]
#[case::one("1")]
#[case::the_cap("3600000")]
fn the_pacing_check_takes_empty_or_one_to_the_cap(#[case] input: &str) {
    assert_eq!(interval_check(input), Ok(()));
}

#[rstest]
#[case::empty("", "Type the body file's path")]
#[case::blank("  ", "Type the body file's path")]
#[case::stdin(" - ", "The menu can't read a body from stdin; type a file path")]
#[case::control_character("in\u{7}.json", "The path has characters that can't be shown")]
fn the_body_file_check_refuses_what_it_cannot_pass_on(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(body_file_check(input), Err(expected.to_owned()));
}

#[rstest]
#[case::blank(" ", "Type a consumer group, such as $Default")]
#[case::control_character(
    "eh\u{1b}[31m",
    "The consumer group has characters that can't be shown"
)]
fn the_consumer_group_check_refuses_blank_and_unshowable_names(
    #[case] input: &str,
    #[case] expected: &str,
) {
    assert_eq!(consumer_group_check(input), Err(expected.to_owned()));
}

#[rstest]
#[case::tab("a\tb")]
#[case::bidi_override("a\u{202E}b")]
fn a_typed_body_the_terminal_would_show_escaped_is_refused(#[case] input: &str) {
    assert!(one_line_body_check(input).is_err());
}

#[test]
fn an_event_hub_name_is_checked_by_cores_rule() {
    assert_eq!(
        event_hub_check("-orders"),
        Err(
            "Invalid event hub name: use 1 to 256 letters, digits, '.', '-', or '_', starting \
             and ending with a letter or digit"
                .to_owned()
        )
    );
}

#[test]
fn a_number_the_checks_would_refuse_is_an_error() {
    assert!(matches!(number("ten"), Err(Stop::Failed(_))));
}

#[test]
fn the_same_as_line_quotes_what_a_posix_shell_would_change() {
    let line = command_line(&[
        "eh".to_owned(),
        "--body={\"a\": 1}".to_owned(),
        "--consumer-group=$Default".to_owned(),
        "it's".to_owned(),
        String::new(),
        "--count=5".to_owned(),
    ]);

    assert_eq!(
        line,
        r#"eh '--body={"a": 1}' '--consumer-group=$Default' 'it'\''s' '' --count=5"#
    );
}

#[test]
fn the_same_as_line_escapes_control_characters_before_quoting() {
    assert_eq!(
        command_line(&["--body=a\u{7}".to_owned()]),
        r"'--body=a\u{7}'"
    );
}

use super::*;
use chrono::Utc;
use common_cli::test_writers::FailingDisk;
use eh_core::models::{BodyKind, MessageKey};
use eh_persistence::config::StoredSecret;
use std::collections::BTreeMap;

fn message(partition_id: &str, body: &str) -> CapturedMessage {
    CapturedMessage {
        key: MessageKey {
            partition_id: partition_id.to_owned(),
            sequence_number: 7,
        },
        offset: None,
        partition_key: None,
        enqueued_time: None,
        processed_at: Utc::now(),
        message_id: None,
        correlation_id: None,
        content_type: None,
        properties: BTreeMap::new(),
        body: body.to_owned(),
        body_kind: BodyKind::Utf8,
    }
}

fn target(auth: AuthMethodConfig, namespace: Option<&str>) -> TargetConfig {
    TargetConfig {
        namespace: namespace.map(str::to_owned),
        event_hub: Some("orders".to_owned()),
        consumer_group: "$Default".to_owned(),
        auth,
    }
}

fn connection_string_auth() -> AuthMethodConfig {
    AuthMethodConfig::ConnectionString {
        stored: StoredSecret::LocalDb,
    }
}

fn renderer(verbose: bool) -> Renderer<Vec<u8>, Vec<u8>> {
    Renderer::new(Vec::new(), Vec::new(), verbose, 200)
}

fn text(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).unwrap()
}

#[test]
fn a_preview_escapes_terminal_control_bytes_and_stays_on_one_line() {
    let hostile = message("0", "pay\u{1b}]52;c;cHduZWQ=\u{7}\nline two");

    let line = preview_line(&hostile, 200);

    assert!(!line.contains('\u{1b}'), "{line:?}");
    assert!(!line.contains('\n'), "{line:?}");
    assert!(line.contains("pay\\u{1b}]52"), "{line}");
    assert!(
        line.starts_with("Partition/Sequence: 0/7 | Bytes: "),
        "{line}"
    );
}

#[test]
fn a_preview_escapes_the_partition_id() {
    let line = preview_line(&message("0\u{1b}[2J", "x"), 200);

    assert!(!line.contains('\u{1b}'), "{line:?}");
}

#[test]
fn a_long_preview_is_cut_to_the_width() {
    let line = preview_line(&message("0", &"x".repeat(500)), 60);

    assert_eq!(line.chars().count(), 60);
    assert!(line.ends_with("..."));
}

#[rstest::rstest]
#[case::fits("short", 10, "short")]
#[case::exact("abcde", 5, "abcde")]
#[case::cut("abcdefgh", 6, "abc...")]
#[case::multibyte("ééééééé", 5, "éé...")]
fn truncate_keeps_whole_characters(
    #[case] line: &str,
    #[case] width: usize,
    #[case] expected: &str,
) {
    assert_eq!(truncate(line, width), expected);
}

#[test]
fn a_notice_clears_the_progress_line_and_goes_to_stderr() {
    let mut renderer = renderer(false);

    renderer.progress(&StatsSnapshot {
        received: 5,
        ..StatsSnapshot::default()
    });
    renderer.notice("Partition 1 failed: boom");
    let (out, err) = renderer.into_parts();

    let out = text(out);
    assert!(out.contains("Received: 5"), "{out:?}");
    assert!(
        out.ends_with("\u{1b}[2K"),
        "the line must be cleared: {out:?}"
    );
    assert!(text(err).contains("Partition 1 failed: boom"));
}

#[test]
fn previews_print_only_when_verbose() {
    let event = EngineEvent::Message(Box::new(message("0", "hello")));
    let mut quiet = renderer(false);
    let mut verbose = renderer(true);

    quiet.engine_event(&event);
    verbose.engine_event(&event);

    assert!(!text(quiet.into_parts().0).contains("hello"));
    assert!(text(verbose.into_parts().0).contains("Preview: hello"));
}

#[test]
fn finish_line_ends_an_open_line_once() {
    let mut renderer = renderer(false);
    renderer.progress(&StatsSnapshot::default());

    renderer.finish_line();
    renderer.finish_line();

    let out = text(renderer.into_parts().0);
    assert_eq!(out.matches('\n').count(), 1, "{out:?}");
}

#[test]
fn a_failing_terminal_never_panics_or_stops_rendering() {
    let mut renderer = Renderer::new(FailingDisk, FailingDisk, true, 80);

    renderer.progress(&StatsSnapshot::default());
    renderer.notice("one");
    renderer.engine_event(&EngineEvent::Message(Box::new(message("0", "x"))));
    renderer.block("done\n");

    assert!(renderer.write_failed);
}

#[test]
fn the_read_summary_names_the_end_counts_partitions_and_at_end_files() {
    let summary = RunSummary {
        stats: StatsSnapshot {
            received: 10,
            stored: 8,
            duplicated: 2,
            filtered_out: 3,
            ..StatsSnapshot::default()
        },
        partitions: BTreeMap::from([
            ("0".to_owned(), PartitionEnd::Stopped),
            (
                "1".to_owned(),
                PartitionEnd::Failed("hub\u{1b}[31m recreated".to_owned()),
            ),
        ]),
        end: RunEnd::Stopped,
    };
    let at_end = ExportOutcome {
        exported: 4,
        skipped: 0,
        failed: 1,
        first_error: Some("inbound/x.txt: denied".to_owned()),
        cancelled: false,
    };

    let block = read_summary_block(&summary, Some(&at_end));

    assert!(
        block.starts_with("Run ended: stopped on request\n"),
        "{block}"
    );
    assert!(block.contains("Received: 10 | Stored: 8 | Duplicated: 2 | Filtered out: 3"));
    assert!(block.contains("  - 0: stopped on request\n"), "{block}");
    assert!(
        block.contains("  - 1: failed: hub\\u{1b}[31m recreated\n"),
        "{block}"
    );
    assert!(block.contains("Message files written at the end: 4, failed: 1\n"));
    assert!(block.contains("First failure: inbound/x.txt: denied\n"));
}

#[test]
fn a_failed_run_summary_shows_the_reason() {
    let summary = RunSummary {
        stats: StatsSnapshot::default(),
        partitions: BTreeMap::new(),
        end: RunEnd::Failed("every partition failed".to_owned()),
    };

    let block = read_summary_block(&summary, None);

    assert!(block.starts_with("Run ended: failed: every partition failed\n"));
    assert!(!block.contains("Partitions:"));
    assert!(!block.contains("Message files"));
}

#[test]
fn the_export_summary_says_when_it_was_cancelled() {
    let outcome = ExportOutcome {
        exported: 2,
        skipped: 5,
        failed: 0,
        first_error: None,
        cancelled: false,
    };

    let complete = export_summary_block(&outcome);
    let cancelled = export_summary_block(&ExportOutcome {
        cancelled: true,
        ..outcome
    });

    assert_eq!(
        complete,
        "Exported: 2 | Skipped (already exported): 5 | Failed: 0\n"
    );
    assert!(cancelled.ends_with("Cancelled; every written file is complete.\n"));
}

#[test]
fn the_export_line_shows_counts_and_rate() {
    let outcome = ExportOutcome {
        exported: 10,
        skipped: 1,
        failed: 2,
        first_error: None,
        cancelled: false,
    };

    let line = export_line(&outcome, Duration::from_secs(5));

    assert_eq!(
        line,
        "Exported: 10 | Skipped: 1 | Failed: 2 | Rate: 2.00 msg/s | Runtime: 00:00:05.000"
    );
}

/// What crossterm's `Clear(ClearType::CurrentLine)` writes.
const CLEAR_LINE: &str = "\u{1b}[2K";

#[test]
fn a_finished_export_on_a_terminal_erases_its_line_for_the_summary() {
    let mut line = ExportProgressLine::new(Vec::new(), true);

    line.draw(&ExportOutcome::default());
    line.draw(&ExportOutcome::default());
    line.finish(true);

    let out = text(line.into_inner());
    assert_eq!(out.matches("Exported: 0").count(), 1, "throttled: {out:?}");
    assert!(out.ends_with(CLEAR_LINE), "{out:?}");
    assert!(!out.contains('\n'), "{out:?}");
}

#[rstest::rstest]
#[case::failed_export_on_a_terminal(true, false)]
#[case::finished_export_into_a_file(false, true)]
#[case::failed_export_into_a_file(false, false)]
fn the_last_counts_stay_on_their_own_line(#[case] terminal: bool, #[case] summary_follows: bool) {
    let mut line = ExportProgressLine::new(Vec::new(), terminal);

    line.draw(&ExportOutcome {
        exported: 4,
        ..ExportOutcome::default()
    });
    line.finish(summary_follows);

    let out = text(line.into_inner());
    assert!(out.contains("Exported: 4"), "{out:?}");
    assert!(out.ends_with('\n'), "{out:?}");
}

#[rstest::rstest]
#[case::finished(true)]
#[case::failed(false)]
fn a_line_that_never_drew_writes_nothing_when_it_finishes(#[case] summary_follows: bool) {
    let mut line = ExportProgressLine::new(Vec::new(), true);

    line.finish(summary_follows);

    assert!(line.into_inner().is_empty());
}

#[test]
fn a_failing_export_line_never_panics_and_reports_once() {
    let mut line = ExportProgressLine::new(FailingDisk, true);

    line.draw(&ExportOutcome::default());
    line.finish(true);

    assert!(line.write_failed);
}

#[test]
fn the_progress_line_shows_the_counters() {
    let line = progress_line(&StatsSnapshot {
        received: 3,
        stored: 2,
        duplicated: 1,
        filtered_out: 4,
        current_rate: 1.5,
        elapsed: Duration::from_millis(61_250),
        ..StatsSnapshot::default()
    });

    assert_eq!(
        line,
        "Received: 3 | Stored: 2 | Duplicated: 1 | Filtered: 4 | Rate: 1.50 msg/s | Runtime: \
         00:01:01.250 | Last: never"
    );
}

#[rstest::rstest]
#[case::credential(
    target(AuthMethodConfig::Credential { credential: CredentialKindConfig::ManagedIdentity }, Some("ns.servicebus.windows.net")),
    "Connecting to namespace ns.servicebus.windows.net, event hub orders (consumer group $Default)..."
)]
#[case::connection_string(
    target(connection_string_auth(), None),
    "Connecting to namespace (from the connection string), event hub orders (consumer group $Default)..."
)]
#[case::key_vault(
    target(
        AuthMethodConfig::KeyVaultConnectionString {
            vault_url: "https://v.vault.azure.net".to_owned(),
            secret_name: "s".to_owned(),
            credential: CredentialKindConfig::DeveloperTools,
        },
        None,
    ),
    "Connecting (connection string from Key Vault, consumer group $Default)..."
)]
fn the_connecting_notice_names_what_is_known(#[case] target: TargetConfig, #[case] expected: &str) {
    assert_eq!(connecting_notice(&target, Some("$Default")), expected);
}

#[rstest::rstest]
#[case::credential(
    target(AuthMethodConfig::Credential { credential: CredentialKindConfig::DeveloperTools }, Some("ns.servicebus.windows.net")),
    "Connecting to namespace ns.servicebus.windows.net, event hub orders..."
)]
#[case::connection_string(
    target(connection_string_auth(), None),
    "Connecting to namespace (from the connection string), event hub orders..."
)]
#[case::key_vault(
    target(
        AuthMethodConfig::KeyVaultConnectionString {
            vault_url: "https://v.vault.azure.net".to_owned(),
            secret_name: "s".to_owned(),
            credential: CredentialKindConfig::DeveloperTools,
        },
        None,
    ),
    "Connecting (connection string from Key Vault)..."
)]
fn the_connecting_notice_without_a_consumer_group_leaves_it_out(
    #[case] target: TargetConfig,
    #[case] expected: &str,
) {
    assert_eq!(connecting_notice(&target, None), expected);
}

#[test]
fn a_credential_target_without_a_namespace_says_not_set() {
    let target = target(
        AuthMethodConfig::Credential {
            credential: CredentialKindConfig::DeveloperTools,
        },
        None,
    );

    assert_eq!(
        target_label(&target),
        "namespace (not set), event hub orders"
    );
}

#[rstest::rstest]
#[case::connection_string(connection_string_auth(), "connection string")]
#[case::client_secret(
    AuthMethodConfig::Credential {
        credential: CredentialKindConfig::ClientSecret { tenant_id: "t".to_owned(), client_id: "c".to_owned() },
    },
    "client secret (tenant t, client c)"
)]
#[case::key_vault(
    AuthMethodConfig::KeyVaultConnectionString {
        vault_url: "https://v.vault.azure.net".to_owned(),
        secret_name: "s".to_owned(),
        credential: CredentialKindConfig::ManagedIdentity,
    },
    "Key Vault secret s in https://v.vault.azure.net via managed identity"
)]
fn describe_auth_names_the_method(#[case] auth: AuthMethodConfig, #[case] expected: &str) {
    assert_eq!(describe_auth(&auth), expected);
}

#[test]
fn filters_are_described_with_escaped_values() {
    let at = Utc::now();

    assert_eq!(describe_time_filter(None), "none");
    assert_eq!(
        describe_time_filter(Some(&TimeFilter::Between { start: at, end: at })),
        format!("from {} to {}", at.to_rfc3339(), at.to_rfc3339())
    );
    assert_eq!(
        describe_text_filter(Some(&TextFilter::Contains(vec![
            "a".to_owned(),
            "b\u{1b}".to_owned()
        ]))),
        "contains any of a, b\\u{1b}"
    );
    assert_eq!(
        describe_text_filter(Some(&TextFilter::regex("x+").unwrap())),
        "matches x+"
    );
}

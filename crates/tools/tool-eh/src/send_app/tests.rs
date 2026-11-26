use super::*;
use eh_core::producer::{Pacing, Routing};
use std::sync::Arc;

fn renderer() -> Renderer<Vec<u8>, Vec<u8>> {
    Renderer::new(Vec::new(), Vec::new(), false, 200)
}

fn summary(end: SendEnd) -> SendSummary {
    SendSummary {
        sent: 4,
        batches: 2,
        elapsed: Duration::from_secs(2),
        end,
    }
}

#[tokio::test(start_paused = true)]
async fn ctrl_c_while_connecting_drops_the_open_and_reports_no_producer() {
    let shutdown = AtomicBool::new(true);
    let (alive, dropped) = tokio::sync::oneshot::channel::<()>();
    let open = async move {
        let _alive = alive;
        std::future::pending::<anyhow::Result<u32>>().await
    };

    let opened = open_or_interrupt(open, &shutdown).await.unwrap();

    assert_eq!(opened, None);
    assert!(dropped.await.is_err(), "the open future must be dropped");
}

#[tokio::test(start_paused = true)]
async fn an_open_that_finished_wins_over_a_ctrl_c_in_the_same_poll() {
    let shutdown = AtomicBool::new(true);

    let opened = open_or_interrupt(async { Ok(7) }, &shutdown).await.unwrap();

    assert_eq!(opened, Some(7));
}

#[tokio::test(start_paused = true)]
async fn a_failed_open_is_returned_as_the_error() {
    let shutdown = AtomicBool::new(false);

    let error = open_or_interrupt(
        async { Err::<u32, _>(anyhow!("namespace not allowed")) },
        &shutdown,
    )
    .await
    .unwrap_err();

    assert_eq!(error.to_string(), "namespace not allowed");
}

#[test]
fn the_destination_names_the_resolved_hub_and_routing() {
    let plan = SendPlan::new(5, Routing::automatic(), Pacing::batched()).unwrap();

    let notice = describe_destination("orders.servicebus.windows.net", "orders", &plan);

    assert_eq!(
        notice,
        "Sending 5 event(s) to orders.servicebus.windows.net/orders, automatic routing..."
    );
}

#[test]
fn a_paced_destination_names_the_key_and_the_interval() {
    let plan = SendPlan::new(
        2,
        Routing::partition_key("customer\u{1b}[2J").unwrap(),
        Pacing::interval(Duration::from_millis(1_000)).unwrap(),
    )
    .unwrap();

    let notice = describe_destination("ns.servicebus.windows.net", "orders", &plan);

    assert_eq!(
        notice,
        "Sending 2 event(s) to ns.servicebus.windows.net/orders, partition key \
         customer\\u{1b}[2J, one every 1000 ms..."
    );
}

#[test]
fn the_progress_line_shows_counts_rate_and_runtime() {
    let progress = SendProgress {
        sent: 10,
        batches: 2,
        elapsed: Duration::from_secs(4),
    };

    assert_eq!(
        send_line(&progress, 20),
        "Sent: 10 of 20 | Batches: 2 | Rate: 2.50 ev/s | Runtime: 00:00:04.000"
    );
}

#[test]
fn the_summary_shows_the_end_counts_and_rate() {
    let block = summary_block(&summary(SendEnd::Completed), 4, None);

    assert_eq!(
        block,
        "Send ended: completed\nSent: 4 of 4 | Batches: 2 | Elapsed: 00:00:02.000 | Average \
         rate: 2.00 ev/s\n"
    );
}

#[test]
fn the_summary_escapes_the_failure_and_shows_a_close_failure() {
    let close = anyhow!("link\u{1b}]52;c;x\u{7} gone");

    let block = summary_block(
        &summary(SendEnd::Failed("rejected\u{1b}]52;".to_owned())),
        9,
        Some(&close),
    );

    assert!(!block.contains('\u{1b}'), "{block:?}");
    assert!(
        block.starts_with("Send ended: failed: rejected\\u{1b}]52;\n"),
        "{block}"
    );
    assert!(
        block.contains("Closing the connection failed: link"),
        "{block}"
    );
}

#[rstest::rstest]
#[case::completed(SendEnd::Completed, RunOutcome::Completed)]
#[case::cancelled(SendEnd::Cancelled, RunOutcome::Interrupted)]
fn clean_ends_map_to_completed_or_interrupted(#[case] end: SendEnd, #[case] expected: RunOutcome) {
    assert_eq!(send_outcome(end, Ok(())).unwrap(), expected);
}

#[rstest::rstest]
#[case::after_completed(SendEnd::Completed)]
#[case::after_cancelled(SendEnd::Cancelled)]
fn a_close_failure_fails_a_clean_run(#[case] end: SendEnd) {
    let error = send_outcome(end, Err(anyhow!("connection reset"))).unwrap_err();

    assert!(
        format!("{error:#}").contains(
            "Every event counted as sent was accepted, but closing the connection failed: \
             connection reset"
        ),
        "{error:#}"
    );
}

#[test]
fn a_failed_run_fails_with_its_reason_and_any_close_failure() {
    let alone = send_outcome(SendEnd::Failed("ServerBusy".to_owned()), Ok(())).unwrap_err();
    let both = send_outcome(
        SendEnd::Failed("ServerBusy".to_owned()),
        Err(anyhow!("connection reset")),
    )
    .unwrap_err();

    assert_eq!(alone.to_string(), "The send failed: ServerBusy");
    assert!(
        both.to_string()
            .contains("ServerBusy; closing the connection also failed: connection reset"),
        "{both:#}"
    );
}

#[tokio::test(start_paused = true)]
async fn a_stop_still_waiting_on_a_batch_says_so_once() {
    let shutdown = AtomicBool::new(true);
    let mut renderer = renderer();
    let send = async {
        tokio::time::sleep(Duration::from_secs(30)).await;
        summary(SendEnd::Cancelled)
    };

    wait_noting_stop(send, &shutdown, &mut renderer).await;

    let stderr = String::from_utf8(renderer.into_parts().1).unwrap();
    assert_eq!(stderr.matches(STOPPING).count(), 1, "{stderr}");
}

#[tokio::test(start_paused = true)]
async fn a_stop_that_ends_within_one_poll_prints_no_notice() {
    let shutdown = Arc::new(AtomicBool::new(false));
    let mut renderer = renderer();
    let flag = Arc::clone(&shutdown);
    let send = async move {
        tokio::time::sleep(Duration::from_millis(250)).await;
        flag.store(true, Ordering::Relaxed);
        tokio::time::sleep(CANCEL_POLL).await;
        summary(SendEnd::Cancelled)
    };

    wait_noting_stop(send, &shutdown, &mut renderer).await;

    let stderr = String::from_utf8(renderer.into_parts().1).unwrap();
    assert!(!stderr.contains(STOPPING), "{stderr}");
}

#[tokio::test(start_paused = true)]
async fn a_run_without_a_stop_prints_no_notice() {
    let shutdown = AtomicBool::new(false);
    let mut renderer = renderer();
    let send = async {
        tokio::time::sleep(Duration::from_secs(5)).await;
        summary(SendEnd::Completed)
    };

    let ended = wait_noting_stop(send, &shutdown, &mut renderer).await;

    assert_eq!(ended.end, SendEnd::Completed);
    assert!(renderer.into_parts().1.is_empty());
}

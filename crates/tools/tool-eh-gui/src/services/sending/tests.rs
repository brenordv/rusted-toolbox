use super::*;
use common_azure::auth::{AuthMethod, CredentialKind};
use common_mock_data::template::Template;
use eh_core::connection::DEFAULT_CONSUMER_GROUP;
use eh_core::producer::{Pacing, Routing};
use eh_persistence::config::{
    AuthMethodConfig, CredentialKindConfig, SensitiveValues, TargetConfig, ToolConfig,
};
use eh_persistence::paths::DataPaths;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use tokio::sync::oneshot;

#[derive(Clone, Copy)]
enum Sending {
    /// Sends this many events, one progress per event.
    Completes(u64),
    /// Sends this many, then fails.
    FailsAfter(u64),
    /// Sends this many, the given time apart.
    Spaced(u64, Duration),
    /// Ends cancelled with nothing sent after this long.
    StopsAfter(Duration),
    /// Never returns.
    Hangs,
    Panics,
    /// Builds this many events, then never returns.
    HangsAfter(u64),
    /// Builds this many events, then panics.
    PanicsAfter(u64),
}

#[derive(Clone, Copy)]
enum Closing {
    Closes,
    Fails,
    Hangs,
}

struct FakePort {
    resolve_fails: bool,
    open_fails: bool,
    /// When set, the open waits for this signal.
    open_gate: Mutex<Option<oneshot::Receiver<()>>>,
    opened: AtomicUsize,
    sending: Sending,
    closing: Closing,
    closed: AtomicUsize,
    seen_pacing: Mutex<Option<Pacing>>,
    /// The body of every event the send reported sent, built by the worker's event closure.
    bodies: Mutex<Vec<String>>,
}

impl FakePort {
    fn new(sending: Sending) -> Self {
        Self {
            resolve_fails: false,
            open_fails: false,
            open_gate: Mutex::new(None),
            opened: AtomicUsize::new(0),
            sending,
            closing: Closing::Closes,
            closed: AtomicUsize::new(0),
            seen_pacing: Mutex::new(None),
            bodies: Mutex::new(Vec::new()),
        }
    }

    fn bodies(&self) -> Vec<String> {
        self.bodies.lock().unwrap().clone()
    }
}

fn target() -> EventHubTarget {
    EventHubTarget {
        auth: AuthMethod::Credential {
            kind: CredentialKind::DeveloperTools,
        },
        client_secret: None,
        fully_qualified_namespace: Some("orders.servicebus.windows.net".to_owned()),
        event_hub_name: Some("orders".to_owned()),
        consumer_group: DEFAULT_CONSUMER_GROUP.to_owned(),
    }
}

impl SendPort for FakePort {
    type Producer = u32;

    async fn resolve(&self, _request: &ConnectRequest) -> anyhow::Result<EventHubTarget> {
        if self.resolve_fails {
            anyhow::bail!("No client secret for client 'client'");
        }
        Ok(target())
    }

    fn open(&self, _target: EventHubTarget) -> OpenFuture<u32> {
        self.opened.fetch_add(1, Ordering::SeqCst);
        let gate = self.open_gate.lock().unwrap().take();
        let fails = self.open_fails;
        Box::pin(async move {
            if let Some(gate) = gate {
                let _ = gate.await;
            }
            if fails {
                anyhow::bail!("Failed to open an Event Hubs client for 'orders'");
            }
            Ok(7)
        })
    }

    fn names(_producer: &u32) -> Names {
        Names {
            namespace: "orders.servicebus.windows.net".to_owned(),
            event_hub: "orders".to_owned(),
        }
    }

    fn send<'a>(
        &'a self,
        _producer: &'a u32,
        plan: &'a SendPlan,
        mut next_event: impl FnMut(u64) -> anyhow::Result<OutgoingEvent> + Send + 'a,
        _cancel: &'a AtomicBool,
        mut progress: impl FnMut(&SendProgress) + Send + 'a,
    ) -> impl Future<Output = SendSummary> + Send + 'a {
        *self.seen_pacing.lock().unwrap() = Some(plan.pacing());
        let sending = self.sending;
        async move {
            let (sent, end, gap) = match sending {
                Sending::Completes(sent) => (sent, SendEnd::Completed, Duration::ZERO),
                Sending::FailsAfter(sent) => (
                    sent,
                    SendEnd::Failed("a batch of 1 event(s) was not confirmed".to_owned()),
                    Duration::ZERO,
                ),
                Sending::Spaced(sent, gap) => (sent, SendEnd::Completed, gap),
                Sending::StopsAfter(after) => {
                    tokio::time::sleep(after).await;
                    (0, SendEnd::Cancelled, Duration::ZERO)
                }
                Sending::Hangs => return std::future::pending().await,
                Sending::Panics => panic!("the fake send panicked"),
                Sending::HangsAfter(built) => {
                    for index in 0..built {
                        next_event(index).unwrap();
                    }
                    return std::future::pending().await;
                }
                Sending::PanicsAfter(built) => {
                    for index in 0..built {
                        next_event(index).unwrap();
                    }
                    panic!("the fake send panicked");
                }
            };
            for count in 1..=sent {
                if count > 1 {
                    tokio::time::sleep(gap).await;
                }
                let event = next_event(count - 1).unwrap();
                self.bodies
                    .lock()
                    .unwrap()
                    .push(String::from_utf8(event.body).unwrap());
                progress(&SendProgress {
                    sent: count,
                    batches: count,
                    elapsed: Duration::ZERO,
                });
            }
            SendSummary {
                sent,
                batches: sent,
                elapsed: Duration::ZERO,
                end,
            }
        }
    }

    async fn close(&self, _producer: u32) -> anyhow::Result<()> {
        self.closed.fetch_add(1, Ordering::SeqCst);
        match self.closing {
            Closing::Closes => Ok(()),
            Closing::Fails => anyhow::bail!("Failed to close the Event Hubs producer"),
            Closing::Hangs => std::future::pending().await,
        }
    }
}

fn request(pacing: Pacing) -> SendRequest {
    SendRequest {
        run: 1,
        target: ConnectRequest {
            config: ToolConfig::new(TargetConfig {
                namespace: Some("orders.servicebus.windows.net".to_owned()),
                event_hub: Some("orders".to_owned()),
                consumer_group: DEFAULT_CONSUMER_GROUP.to_owned(),
                auth: AuthMethodConfig::Credential {
                    credential: CredentialKindConfig::DeveloperTools,
                },
            }),
            given: SensitiveValues::default(),
            paths: DataPaths::new(".eh-data"),
        },
        event: OutgoingEvent {
            body: b"ping".to_vec(),
            ..OutgoingEvent::default()
        },
        template: None,
        plan: SendPlan::new(3, Routing::automatic(), pacing).unwrap(),
        cancel: Arc::new(AtomicBool::new(false)),
        sequence: Arc::new(AtomicU64::new(0)),
    }
}

fn templated(source: &str, sequence: &Arc<AtomicU64>) -> SendRequest {
    SendRequest {
        template: Some(Arc::new(Template::compile(source).unwrap())),
        sequence: Arc::clone(sequence),
        ..request(Pacing::batched())
    }
}

#[derive(Clone, Default)]
struct Posts(Arc<Mutex<Vec<SendInput>>>);

impl Posts {
    fn post(&self) -> Arc<impl Fn(SendInput) + Send + Sync + 'static> {
        let posts = self.clone();
        Arc::new(move |input| posts.0.lock().unwrap().push(input))
    }

    fn started(&self) -> bool {
        self.0
            .lock()
            .unwrap()
            .iter()
            .any(|input| matches!(input, SendInput::Started { .. }))
    }

    /// Every finish posted, in order: the summary's end and the close error.
    fn finishes(&self) -> Vec<(SendEnd, u64, Option<String>)> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter_map(|input| match input {
                SendInput::Finished {
                    summary,
                    close_error,
                    ..
                } => Some((summary.end.clone(), summary.sent, close_error.clone())),
                SendInput::Started { .. } | SendInput::Progress { .. } => None,
                SendInput::SendPressed | SendInput::CancelPressed => None,
            })
            .collect()
    }
}

/// Spawns the worker through a generic function, as `spawn_joined` does: proving the concrete
/// worker future `Send` at the spawn site trips rustc issue 100013.
fn spawn_worker<P: SendPort>(
    port: Arc<P>,
    request: SendRequest,
    post: Arc<impl Fn(SendInput) + Send + Sync + 'static>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(run_send(port, request, post, Arc::new(AtomicU64::new(0))))
}

async fn run(port: FakePort, request: SendRequest) -> (Posts, Arc<FakePort>) {
    let posts = Posts::default();
    let port = Arc::new(port);
    run_send(
        Arc::clone(&port),
        request,
        posts.post(),
        Arc::new(AtomicU64::new(0)),
    )
    .await;
    (posts, port)
}

#[tokio::test(start_paused = true)]
async fn a_completed_send_posts_started_progress_and_one_finish_and_closes() {
    let (posts, port) = run(
        FakePort::new(Sending::Completes(3)),
        request(Pacing::batched()),
    )
    .await;

    assert!(posts.started());
    assert_eq!(posts.finishes(), vec![(SendEnd::Completed, 3, None)]);
    assert_eq!(port.closed.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn the_plan_reaches_the_send_with_its_pacing() {
    let pacing = Pacing::interval(Duration::from_millis(250)).unwrap();

    let (_, port) = run(FakePort::new(Sending::Completes(3)), request(pacing)).await;

    assert_eq!(*port.seen_pacing.lock().unwrap(), Some(pacing));
}

#[rstest::rstest]
#[case::with_a_template(Some("n={{random.integer min=4 max=4}}"), "n=4")]
#[case::without_one(None, "ping")]
#[tokio::test(start_paused = true)]
async fn every_event_is_the_request_body_or_a_render_of_its_template(
    #[case] source: Option<&str>,
    #[case] body: &str,
) {
    let mut request = request(Pacing::batched());
    request.template = source.map(|source| Arc::new(Template::compile(source).unwrap()));

    let (_, port) = run(FakePort::new(Sending::Completes(3)), request).await;

    assert_eq!(port.bodies(), [body; 3]);
}

#[tokio::test(start_paused = true)]
async fn the_sequence_runs_on_across_runs_that_share_a_counter() {
    let sequence = Arc::new(AtomicU64::new(0));

    let (_, first) = run(
        FakePort::new(Sending::Completes(3)),
        templated("{{sequence}}", &sequence),
    )
    .await;
    let (_, second) = run(
        FakePort::new(Sending::Completes(3)),
        templated("{{sequence}}", &sequence),
    )
    .await;

    assert_eq!(first.bodies(), ["0", "1", "2"]);
    assert_eq!(second.bodies(), ["3", "4", "5"]);
    assert_eq!(sequence.load(Ordering::Relaxed), 6);
}

#[tokio::test(start_paused = true)]
async fn a_template_without_a_sequence_leaves_the_counter_alone() {
    let sequence = Arc::new(AtomicU64::new(0));

    let (_, port) = run(
        FakePort::new(Sending::Completes(3)),
        templated("{{random.integer min=1 max=1}}", &sequence),
    )
    .await;

    assert_eq!(port.bodies(), ["1", "1", "1"]);
    assert_eq!(sequence.load(Ordering::Relaxed), 0);
}

#[tokio::test(start_paused = true)]
async fn numbers_taken_by_a_failed_send_stay_taken() {
    let sequence = Arc::new(AtomicU64::new(0));

    let (posts, port) = run(
        FakePort::new(Sending::FailsAfter(2)),
        templated("{{sequence}}", &sequence),
    )
    .await;

    assert!(matches!(
        posts.finishes().as_slice(),
        [(SendEnd::Failed(_), 2, None)]
    ));
    assert_eq!(port.bodies(), ["0", "1"]);
    assert_eq!(sequence.load(Ordering::Relaxed), 2);
}

#[tokio::test(start_paused = true)]
async fn a_failed_resolution_ends_failed_without_opening() {
    let port = FakePort {
        resolve_fails: true,
        ..FakePort::new(Sending::Completes(3))
    };

    let (posts, port) = run(port, request(Pacing::batched())).await;

    assert!(!posts.started());
    assert!(matches!(
        posts.finishes().as_slice(),
        [(SendEnd::Failed(reason), 0, None)] if reason.starts_with("No client secret")
    ));
    assert_eq!(port.opened.load(Ordering::SeqCst), 0);
}

#[tokio::test(start_paused = true)]
async fn a_failed_open_ends_failed_without_sending() {
    let port = FakePort {
        open_fails: true,
        ..FakePort::new(Sending::Completes(3))
    };

    let (posts, port) = run(port, request(Pacing::batched())).await;

    assert!(!posts.started());
    assert!(matches!(
        posts.finishes().as_slice(),
        [(SendEnd::Failed(_), 0, None)]
    ));
    assert_eq!(port.closed.load(Ordering::SeqCst), 0);
}

#[tokio::test(start_paused = true)]
async fn a_cancel_before_the_open_ends_cancelled_without_opening() {
    let request = request(Pacing::batched());
    request.cancel.store(true, Ordering::Release);

    let (posts, port) = run(FakePort::new(Sending::Completes(3)), request).await;

    assert_eq!(posts.finishes(), vec![(SendEnd::Cancelled, 0, None)]);
    assert_eq!(port.opened.load(Ordering::SeqCst), 0);
}

#[tokio::test(start_paused = true)]
async fn a_cancel_while_opening_ends_cancelled_and_closes_the_late_producer() {
    let (release, gate) = oneshot::channel();
    let port = Arc::new(FakePort {
        open_gate: Mutex::new(Some(gate)),
        ..FakePort::new(Sending::Completes(3))
    });
    let request = request(Pacing::batched());
    let cancel = Arc::clone(&request.cancel);
    let posts = Posts::default();
    let worker = spawn_worker(Arc::clone(&port), request, posts.post());
    tokio::time::sleep(Duration::from_millis(50)).await;

    cancel.store(true, Ordering::Release);
    worker.await.unwrap();
    release.send(()).unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await;

    assert!(!posts.started());
    assert_eq!(posts.finishes(), vec![(SendEnd::Cancelled, 0, None)]);
    assert_eq!(port.closed.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn a_send_failing_midway_reports_the_count_and_closes() {
    let (posts, port) = run(
        FakePort::new(Sending::FailsAfter(2)),
        request(Pacing::batched()),
    )
    .await;

    assert!(matches!(
        posts.finishes().as_slice(),
        [(SendEnd::Failed(_), 2, None)]
    ));
    assert_eq!(port.closed.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn a_failed_close_is_the_close_error() {
    let port = FakePort {
        closing: Closing::Fails,
        ..FakePort::new(Sending::Completes(3))
    };

    let (posts, _) = run(port, request(Pacing::batched())).await;

    assert!(matches!(
        posts.finishes().as_slice(),
        [(SendEnd::Completed, 3, Some(error))] if error.starts_with("Failed to close")
    ));
}

#[tokio::test(start_paused = true)]
async fn a_close_that_hangs_times_out_into_the_close_error() {
    let port = FakePort {
        closing: Closing::Hangs,
        ..FakePort::new(Sending::Completes(3))
    };

    let (posts, _) = run(port, request(Pacing::batched())).await;

    assert_eq!(
        posts.finishes(),
        vec![(SendEnd::Completed, 3, Some(CLOSE_TOO_SLOW.to_owned()))]
    );
}

#[tokio::test(start_paused = true)]
async fn a_send_that_does_not_stop_after_a_cancel_is_reported_past_the_bound() {
    let request = request(Pacing::batched());
    let cancel = Arc::clone(&request.cancel);
    let posts = Posts::default();
    let port = Arc::new(FakePort::new(Sending::Hangs));
    let worker = spawn_worker(port, request, posts.post());
    tokio::time::sleep(Duration::from_millis(50)).await;
    cancel.store(true, Ordering::Release);

    tokio::time::sleep(SEND_STOP_BOUND - Duration::from_secs(1)).await;
    let before = posts.finishes();
    tokio::time::sleep(Duration::from_secs(2)).await;
    let after = posts.finishes();
    worker.abort();

    assert!(before.is_empty());
    assert!(matches!(
        after.as_slice(),
        [(SendEnd::Failed(reason), 0, None)] if reason == DID_NOT_STOP
    ));
}

#[test]
fn a_worker_that_panics_still_ends_its_run() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    let posts = Posts::default();
    let post = posts.post();

    spawn_joined(
        rt.handle(),
        Arc::new(FakePort::new(Sending::Panics)),
        request(Pacing::batched()),
        move |input| post(input),
    );
    rt.block_on(async {
        for _ in 0..100 {
            if !posts.finishes().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });

    assert!(matches!(
        posts.finishes().as_slice(),
        [(SendEnd::Failed(reason), _, None)] if reason == STOPPED_UNEXPECTEDLY
    ));
}

#[test]
fn numbers_taken_before_a_worker_panic_stay_taken() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    let sequence = Arc::new(AtomicU64::new(0));
    let posts = Posts::default();
    let post = posts.post();

    spawn_joined(
        rt.handle(),
        Arc::new(FakePort::new(Sending::PanicsAfter(2))),
        templated("{{sequence}}", &sequence),
        move |input| post(input),
    );
    rt.block_on(async {
        for _ in 0..100 {
            if !posts.finishes().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });

    assert!(matches!(
        posts.finishes().as_slice(),
        [(SendEnd::Failed(reason), _, None)] if reason == STOPPED_UNEXPECTEDLY
    ));
    assert_eq!(sequence.load(Ordering::Relaxed), 2);
}

#[tokio::test(start_paused = true)]
async fn numbers_taken_before_a_send_that_does_not_stop_stay_taken() {
    let sequence = Arc::new(AtomicU64::new(0));
    let request = templated("{{sequence}}", &sequence);
    let cancel = Arc::clone(&request.cancel);
    let posts = Posts::default();
    let worker = spawn_worker(
        Arc::new(FakePort::new(Sending::HangsAfter(2))),
        request,
        posts.post(),
    );
    tokio::time::sleep(Duration::from_millis(50)).await;
    cancel.store(true, Ordering::Release);

    tokio::time::sleep(SEND_STOP_BOUND + Duration::from_secs(1)).await;
    let finished = posts.finishes();
    worker.abort();

    assert!(matches!(
        finished.as_slice(),
        [(SendEnd::Failed(reason), 0, None)] if reason == DID_NOT_STOP
    ));
    assert_eq!(sequence.load(Ordering::Relaxed), 2);
}

impl Posts {
    fn progress_count(&self) -> usize {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|input| matches!(input, SendInput::Progress { .. }))
            .count()
    }
}

#[rstest::rstest]
#[case::at_once(Duration::ZERO, 1)]
#[case::past_the_throttle(Duration::from_millis(300), 3)]
#[tokio::test(start_paused = true)]
async fn progress_posts_are_throttled_on_tokio_time(#[case] gap: Duration, #[case] posts: usize) {
    let (seen, _) = run(
        FakePort::new(Sending::Spaced(3, gap)),
        request(Pacing::batched()),
    )
    .await;

    assert_eq!(seen.progress_count(), posts);
}

#[tokio::test(start_paused = true)]
async fn after_a_cancel_the_close_shares_the_stop_deadline() {
    let request = request(Pacing::batched());
    let cancel = Arc::clone(&request.cancel);
    let port = Arc::new(FakePort {
        closing: Closing::Hangs,
        ..FakePort::new(Sending::StopsAfter(Duration::from_secs(25)))
    });
    let posts = Posts::default();
    let worker = spawn_worker(port, request, posts.post());
    tokio::time::sleep(Duration::from_millis(10)).await;
    let cancelled = tokio::time::Instant::now();
    cancel.store(true, Ordering::Release);

    worker.await.unwrap();

    let took = cancelled.elapsed();
    assert!(took <= SEND_STOP_BOUND + CANCEL_POLL, "{took:?}");
    assert_eq!(
        posts.finishes(),
        vec![(SendEnd::Cancelled, 0, Some(CLOSE_TOO_SLOW.to_owned()))]
    );
}

#[rstest::rstest]
#[case::close_fails(false, Closing::Fails, 1)]
#[case::open_fails(true, Closing::Closes, 0)]
#[tokio::test(start_paused = true)]
async fn an_abandoned_open_ends_cancelled_whatever_its_cleanup_meets(
    #[case] open_fails: bool,
    #[case] closing: Closing,
    #[case] closes: usize,
) {
    let (release, gate) = oneshot::channel();
    let port = Arc::new(FakePort {
        open_fails,
        closing,
        open_gate: Mutex::new(Some(gate)),
        ..FakePort::new(Sending::Completes(3))
    });
    let request = request(Pacing::batched());
    let cancel = Arc::clone(&request.cancel);
    let posts = Posts::default();
    let worker = spawn_worker(Arc::clone(&port), request, posts.post());
    tokio::time::sleep(Duration::from_millis(50)).await;

    cancel.store(true, Ordering::Release);
    worker.await.unwrap();
    release.send(()).unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await;

    assert_eq!(posts.finishes(), vec![(SendEnd::Cancelled, 0, None)]);
    assert_eq!(port.closed.load(Ordering::SeqCst), closes);
}

use super::*;
use crate::connection::DEFAULT_CONSUMER_GROUP;
use azure_messaging_eventhubs::models::{AmqpSimpleValue, MessageId};
use rstest::rstest;
use shared_azure::auth::{AuthMethod, CredentialKind};
use std::sync::Mutex;
use std::sync::atomic::Ordering;
use tokio::time::Instant;

const SECRET_KEY: &str = "c2VjcmV0LWtleS1kby1ub3QtbGVhaw==";

/// A publisher whose batches hold `max_bytes` of body, counting a batch's
/// first event twice as the SDK does.
struct FakePublisher {
    max_bytes: usize,
    create_fails_on: Option<u64>,
    create_hangs: bool,
    send_fails_on: Option<u64>,
    add_fails: bool,
    send_takes: Duration,
    started: Instant,
    created_at: Mutex<Vec<Duration>>,
    routing_keys: Mutex<Vec<Option<String>>>,
    sent: Mutex<Vec<Vec<Vec<u8>>>>,
}

impl FakePublisher {
    fn new(max_bytes: usize) -> Self {
        Self {
            max_bytes,
            create_fails_on: None,
            create_hangs: false,
            send_fails_on: None,
            add_fails: false,
            send_takes: Duration::ZERO,
            started: Instant::now(),
            created_at: Mutex::new(Vec::new()),
            routing_keys: Mutex::new(Vec::new()),
            sent: Mutex::new(Vec::new()),
        }
    }

    fn created(&self) -> usize {
        self.created_at.lock().unwrap().len()
    }

    fn created_at(&self) -> Vec<Duration> {
        self.created_at.lock().unwrap().clone()
    }

    fn sent_bodies(&self) -> Vec<Vec<String>> {
        self.sent
            .lock()
            .unwrap()
            .iter()
            .map(|batch| {
                batch
                    .iter()
                    .map(|body| String::from_utf8(body.clone()).unwrap())
                    .collect()
            })
            .collect()
    }
}

struct FakeBatch {
    max_bytes: usize,
    add_fails: bool,
    size: Mutex<usize>,
    bodies: Mutex<Vec<Vec<u8>>>,
}

impl PublishBatch for FakeBatch {
    fn try_add(&self, event: EventData) -> anyhow::Result<bool> {
        if self.add_fails {
            anyhow::bail!("serialization failed");
        }
        let body = event.body().unwrap_or_default().to_vec();
        let mut bodies = self.bodies.lock().unwrap();
        let cost = if bodies.is_empty() {
            2 * body.len()
        } else {
            body.len()
        };
        let mut size = self.size.lock().unwrap();
        if *size + cost > self.max_bytes {
            return Ok(false);
        }
        *size += cost;
        bodies.push(body);
        Ok(true)
    }
}

impl Publisher for FakePublisher {
    type Batch<'a> = FakeBatch;

    async fn create_batch(&self, routing: &Routing) -> anyhow::Result<FakeBatch> {
        let number = {
            let mut created = self.created_at.lock().unwrap();
            created.push(self.started.elapsed());
            created.len() as u64
        };
        self.routing_keys
            .lock()
            .unwrap()
            .push(routing.key().map(str::to_owned));
        if self.create_hangs {
            std::future::pending::<()>().await;
        }
        if self.create_fails_on == Some(number) {
            anyhow::bail!("link attach refused");
        }
        Ok(FakeBatch {
            max_bytes: self.max_bytes,
            add_fails: self.add_fails,
            size: Mutex::new(0),
            bodies: Mutex::new(Vec::new()),
        })
    }

    async fn send_batch(&self, batch: FakeBatch) -> anyhow::Result<()> {
        tokio::time::sleep(self.send_takes).await;
        let number = self.sent.lock().unwrap().len() as u64 + 1;
        if self.send_fails_on == Some(number) {
            anyhow::bail!("ServerBusy");
        }
        let bodies = batch.bodies.into_inner().unwrap();
        self.sent.lock().unwrap().push(bodies);
        Ok(())
    }
}

fn event(body: &str) -> OutgoingEvent {
    OutgoingEvent {
        body: body.as_bytes().to_vec(),
        ..OutgoingEvent::default()
    }
}

/// Two properties, `run` and `source`.
fn two_properties() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("run".to_owned(), "7".to_owned()),
        ("source".to_owned(), "load-test".to_owned()),
    ])
}

fn plan(count: u64) -> SendPlan {
    SendPlan::new(count, Routing::automatic(), Pacing::batched()).unwrap()
}

fn paced(count: u64, interval_ms: u64) -> SendPlan {
    SendPlan::new(
        count,
        Routing::automatic(),
        Pacing::interval(Duration::from_millis(interval_ms)).unwrap(),
    )
    .unwrap()
}

/// Bodies "e0", "e1", ... of two bytes each.
fn numbered(index: u64) -> anyhow::Result<OutgoingEvent> {
    Ok(event(&format!("e{index}")))
}

async fn send(publisher: &FakePublisher, plan: &SendPlan) -> SendSummary {
    send_loop::run(publisher, plan, numbered, &AtomicBool::new(false), |_| {}).await
}

fn failure(summary: &SendSummary) -> &str {
    match &summary.end {
        SendEnd::Failed(reason) => reason,
        SendEnd::Completed | SendEnd::Cancelled => panic!("expected a failure: {summary:?}"),
    }
}

fn seconds(values: &[f64]) -> Vec<Duration> {
    values
        .iter()
        .map(|&value| Duration::from_secs_f64(value))
        .collect()
}

#[tokio::test(start_paused = true)]
async fn events_are_packed_into_as_few_batches_as_fit() {
    let publisher = FakePublisher::new(6);

    let summary = send(&publisher, &plan(3)).await;

    assert_eq!(summary.end, SendEnd::Completed);
    assert_eq!((summary.sent, summary.batches), (3, 2));
    assert_eq!(
        publisher.sent_bodies(),
        [vec!["e0", "e1"], vec!["e2"]],
        "the event that did not fit opens the next batch"
    );
}

#[tokio::test(start_paused = true)]
async fn an_event_too_large_for_an_empty_batch_fails_naming_it() {
    let publisher = FakePublisher::new(3);
    let mut asked = Vec::new();

    let summary = send_loop::run(
        &publisher,
        &plan(5),
        |index| {
            asked.push(index);
            numbered(index)
        },
        &AtomicBool::new(false),
        |_| {},
    )
    .await;

    let reason = failure(&summary);
    assert!(
        reason.contains("event 0 (a 2-byte body) does not fit"),
        "{reason}"
    );
    assert!(
        reason.contains("about half the event hub's publish limit"),
        "{reason}"
    );
    assert_eq!(summary.sent, 0);
    assert_eq!(asked, [0]);
    assert!(publisher.sent_bodies().is_empty());
}

#[rstest]
#[case::one_property(1, false, "event 0 (a 2-byte body and 1 property) does not fit")]
#[case::two_properties(2, false, "event 0 (a 2-byte body and 2 properties) does not fit")]
#[case::an_id(0, true, "event 0 (a 2-byte body and a correlation id) does not fit")]
#[case::both(
    2,
    true,
    "event 0 (a 2-byte body, 2 properties, and a correlation id) does not fit"
)]
#[tokio::test(start_paused = true)]
async fn an_event_too_large_with_metadata_names_what_it_carries_but_no_text(
    #[case] count: usize,
    #[case] with_id: bool,
    #[case] expected: &str,
) {
    let publisher = FakePublisher::new(3);
    let properties: BTreeMap<String, String> =
        [("k-secret-a", "v-secret-a"), ("k-secret-b", "v-secret-b")]
            .into_iter()
            .take(count)
            .map(|(key, value)| (key.to_owned(), value.to_owned()))
            .collect();
    let correlation_id = with_id.then(|| "id-secret".to_owned());

    let summary = send_loop::run(
        &publisher,
        &plan(1),
        |_| {
            Ok(OutgoingEvent {
                properties: properties.clone(),
                correlation_id: correlation_id.clone(),
                ..event("e0")
            })
        },
        &AtomicBool::new(false),
        |_| {},
    )
    .await;

    let reason = failure(&summary);
    assert!(reason.contains(expected), "{reason}");
    assert!(!reason.contains("secret"), "{reason}");
}

#[tokio::test(start_paused = true)]
async fn cancel_before_the_first_batch_creates_nothing() {
    let publisher = FakePublisher::new(100);

    let summary = send_loop::run(
        &publisher,
        &plan(3),
        numbered,
        &AtomicBool::new(true),
        |_| {},
    )
    .await;

    assert_eq!(summary.end, SendEnd::Cancelled);
    assert_eq!(summary.sent, 0);
    assert_eq!(publisher.created(), 0);
}

#[tokio::test(start_paused = true)]
async fn cancel_after_the_first_batch_keeps_its_count() {
    let publisher = FakePublisher::new(6);
    let cancel = AtomicBool::new(false);

    let summary = send_loop::run(&publisher, &plan(5), numbered, &cancel, |_| {
        cancel.store(true, Ordering::Relaxed)
    })
    .await;

    assert_eq!(summary.end, SendEnd::Cancelled);
    assert_eq!((summary.sent, summary.batches), (2, 1));
    assert_eq!(publisher.created(), 1);
}

#[tokio::test(start_paused = true)]
async fn cancel_while_a_batch_is_being_created_stops_within_one_poll() {
    let mut publisher = FakePublisher::new(100);
    publisher.create_hangs = true;
    let cancel = AtomicBool::new(false);
    let plan = plan(3);
    let started = Instant::now();

    let (summary, ()) = tokio::join!(
        send_loop::run(&publisher, &plan, numbered, &cancel, |_| {}),
        async {
            tokio::time::sleep(Duration::from_millis(500)).await;
            cancel.store(true, Ordering::Relaxed);
        }
    );

    assert_eq!(summary.end, SendEnd::Cancelled);
    assert_eq!(summary.sent, 0);
    assert!(publisher.sent_bodies().is_empty());
    assert!(
        started.elapsed() <= Duration::from_millis(600),
        "{:?}",
        started.elapsed()
    );
}

#[rstest]
#[case::first_batch(1, 0)]
#[case::second_batch(2, 2)]
#[tokio::test(start_paused = true)]
async fn a_failed_batch_creation_fails_the_run_with_the_counts_so_far(
    #[case] fails_on: u64,
    #[case] sent_before: u64,
) {
    let mut publisher = FakePublisher::new(6);
    publisher.create_fails_on = Some(fails_on);

    let summary = send(&publisher, &plan(5)).await;

    let reason = failure(&summary);
    assert!(
        reason.contains(&format!("no event of batch {fails_on} was sent")),
        "{reason}"
    );
    assert!(reason.contains("link attach refused"), "{reason}");
    assert_eq!(summary.sent, sent_before);
    assert_eq!(publisher.sent_bodies().len() as u64, fails_on - 1);
}

#[tokio::test(start_paused = true)]
async fn a_failing_event_source_fails_naming_the_index_and_sends_nothing_of_its_batch() {
    let publisher = FakePublisher::new(100);

    let summary = send_loop::run(
        &publisher,
        &plan(5),
        |index| {
            if index == 2 {
                anyhow::bail!("template broke");
            }
            numbered(index)
        },
        &AtomicBool::new(false),
        |_| {},
    )
    .await;

    let reason = failure(&summary);
    assert!(reason.contains("event 2: template broke"), "{reason}");
    assert_eq!(summary.sent, 0);
    assert!(publisher.sent_bodies().is_empty());
}

#[tokio::test(start_paused = true)]
async fn an_event_the_batch_cannot_take_fails_and_sends_nothing() {
    let mut publisher = FakePublisher::new(100);
    publisher.add_fails = true;

    let summary = send(&publisher, &plan(2)).await;

    let reason = failure(&summary);
    assert!(
        reason.contains("Failed to add event 0 to a batch: serialization failed"),
        "{reason}"
    );
    assert!(publisher.sent_bodies().is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_failed_send_is_not_counted_and_says_it_was_not_confirmed() {
    let mut publisher = FakePublisher::new(6);
    publisher.send_fails_on = Some(2);

    let summary = send(&publisher, &plan(4)).await;

    let reason = failure(&summary);
    assert!(
        reason.contains("ServerBusy; a batch of 2 event(s) was not confirmed"),
        "{reason}"
    );
    assert_eq!((summary.sent, summary.batches), (2, 1));
}

#[tokio::test(start_paused = true)]
async fn progress_reports_after_every_sent_batch() {
    let publisher = FakePublisher::new(6);
    let mut seen = Vec::new();

    send_loop::run(
        &publisher,
        &plan(3),
        numbered,
        &AtomicBool::new(false),
        |progress| seen.push((progress.sent, progress.batches)),
    )
    .await;

    assert_eq!(seen, [(2, 1), (3, 2)]);
}

#[tokio::test(start_paused = true)]
async fn the_partition_key_reaches_every_batch() {
    let publisher = FakePublisher::new(6);
    let plan = SendPlan::new(
        3,
        Routing::partition_key("customer-17").unwrap(),
        Pacing::batched(),
    )
    .unwrap();

    send(&publisher, &plan).await;

    assert_eq!(
        *publisher.routing_keys.lock().unwrap(),
        [
            Some("customer-17".to_owned()),
            Some("customer-17".to_owned())
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn paced_events_go_one_per_batch_one_interval_apart() {
    let publisher = FakePublisher::new(100);
    let mut reported = 0;

    let summary = send_loop::run(
        &publisher,
        &paced(3, 1_000),
        numbered,
        &AtomicBool::new(false),
        |_| reported += 1,
    )
    .await;

    assert_eq!(summary.end, SendEnd::Completed);
    assert_eq!((summary.sent, summary.batches), (3, 3));
    assert_eq!(publisher.created_at(), seconds(&[0.0, 1.0, 2.0]));
    assert_eq!(reported, 3);
    assert_eq!(summary.elapsed, Duration::from_secs(2));
}

#[tokio::test(start_paused = true)]
async fn a_send_slower_than_the_interval_delays_the_next_event_without_a_burst() {
    let mut publisher = FakePublisher::new(100);
    publisher.send_takes = Duration::from_millis(1_500);

    send(&publisher, &paced(3, 1_000)).await;

    assert_eq!(publisher.created_at(), seconds(&[0.0, 1.5, 3.0]));
}

#[tokio::test(start_paused = true)]
async fn cancel_during_a_paced_wait_stops_within_one_poll() {
    let publisher = FakePublisher::new(100);
    let cancel = AtomicBool::new(false);
    let plan = paced(5, 10_000);
    let started = Instant::now();

    let (summary, ()) = tokio::join!(
        send_loop::run(&publisher, &plan, numbered, &cancel, |_| {}),
        async {
            tokio::time::sleep(Duration::from_millis(12_000)).await;
            cancel.store(true, Ordering::Relaxed);
        }
    );

    assert_eq!(summary.end, SendEnd::Cancelled);
    assert_eq!(summary.sent, 2);
    assert_eq!(publisher.created(), 2);
    assert!(
        started.elapsed() <= Duration::from_millis(12_100),
        "{:?}",
        started.elapsed()
    );
}

#[rstest]
#[case::zero(Duration::ZERO)]
#[case::under_a_millisecond(Duration::from_micros(999))]
#[case::over_the_maximum(MAX_SEND_INTERVAL + Duration::from_millis(1))]
fn intervals_outside_the_bounds_are_refused(#[case] interval: Duration) {
    let error = Pacing::interval(interval).unwrap_err();

    assert!(
        error.to_string().contains("between 1 ms and 3600000 ms"),
        "{error:#}"
    );
}

#[rstest]
#[case::one_millisecond(Duration::from_millis(1))]
#[case::the_maximum(MAX_SEND_INTERVAL)]
fn intervals_on_the_bounds_are_accepted(#[case] interval: Duration) {
    assert_eq!(Pacing::interval(interval).unwrap().get(), Some(interval));
}

#[rstest]
#[case::zero(0)]
#[case::over_the_maximum(MAX_SEND_COUNT + 1)]
fn counts_outside_the_bounds_are_refused(#[case] count: u64) {
    let error = SendPlan::new(count, Routing::automatic(), Pacing::batched()).unwrap_err();

    assert!(
        error.to_string().contains("between 1 and 1000000"),
        "{error:#}"
    );
}

#[rstest]
#[case::empty("")]
#[case::blank("  ")]
fn a_blank_partition_key_is_refused(#[case] key: &str) {
    assert!(Routing::partition_key(key).is_err());
}

#[test]
fn routing_maps_to_batch_options() {
    let keyed = batch_options(&Routing::partition_key("customer-17").unwrap());
    let automatic = batch_options(&Routing::automatic());

    assert_eq!(keyed.partition_key.as_deref(), Some("customer-17"));
    assert_eq!(automatic.partition_key, None);
    assert_eq!((keyed.partition_id, keyed.max_size_in_bytes), (None, None));
}

#[test]
fn pacing_describes_itself_for_logs_and_headers() {
    assert_eq!(Pacing::batched().describe(), "batched");
    assert_eq!(
        Pacing::interval(Duration::from_millis(250))
            .unwrap()
            .describe(),
        "every 250 ms"
    );
}

#[test]
fn an_outgoing_event_converts_with_its_body_and_content_type() {
    let outgoing = OutgoingEvent {
        body: b"{\"orderId\": 1}".to_vec(),
        content_type: Some("application/json".to_owned()),
        ..OutgoingEvent::default()
    };

    let data = outgoing.to_event_data();

    assert_eq!(data.body(), Some(&b"{\"orderId\": 1}"[..]));
    assert_eq!(data.content_type(), Some("application/json"));
    assert_eq!(data.correlation_id(), None);
    assert_eq!(data.properties(), None);
}

#[test]
fn an_outgoing_event_converts_with_its_correlation_id_and_properties() {
    let outgoing = OutgoingEvent {
        correlation_id: Some("run-7".to_owned()),
        properties: two_properties(),
        ..event("{}")
    };

    let data = outgoing.to_event_data();

    assert_eq!(
        data.correlation_id(),
        Some(&MessageId::String("run-7".to_owned()))
    );
    let properties = data.properties().unwrap();
    assert_eq!(properties.len(), 2);
    assert_eq!(properties["run"], AmqpSimpleValue::from("7"));
    assert_eq!(properties["source"], AmqpSimpleValue::from("load-test"));
}

#[test]
fn debug_of_an_outgoing_event_never_shows_the_body() {
    let outgoing = OutgoingEvent {
        body: b"card-4111".to_vec(),
        content_type: Some("text/plain".to_owned()),
        ..OutgoingEvent::default()
    };

    let rendered = format!("{outgoing:?}");

    assert!(!rendered.contains("card"), "{rendered}");
    assert!(!rendered.contains("4111"), "{rendered}");
    assert!(rendered.contains("body_len: 9"), "{rendered}");
}

#[test]
fn debug_of_an_outgoing_event_counts_properties_and_hides_them_and_the_id() {
    let outgoing = OutgoingEvent {
        correlation_id: Some("trace-abc".to_owned()),
        properties: two_properties(),
        ..event("x")
    };

    let rendered = format!("{outgoing:?}");

    for hidden in ["trace-abc", "run", "7", "source", "load-test"] {
        assert!(!rendered.contains(hidden), "{hidden} in {rendered}");
    }
    assert!(rendered.contains("correlation_id: \"set\""), "{rendered}");
    assert!(rendered.contains("property_count: 2"), "{rendered}");
}

#[rstest]
#[case::plain("a=b", "a", "b")]
#[case::spaced(" a = b ", "a", "b")]
#[case::empty_value("a=", "a", "")]
#[case::first_equals_splits("a=b=c", "a", "b=c")]
fn a_property_splits_at_the_first_equals_sign_and_trims(
    #[case] text: &str,
    #[case] key: &str,
    #[case] value: &str,
) {
    assert_eq!(
        parse_property(text).unwrap(),
        (key.to_owned(), value.to_owned())
    );
}

#[rstest]
#[case::no_equals("secretab", "no '=' (write it as key=value)")]
#[case::no_key("=secret", "no key before '='")]
#[case::blank_key("  =secret", "no key before '='")]
fn a_bad_property_fails_without_its_text(#[case] text: &str, #[case] expected: &str) {
    let error = parse_property(text).unwrap_err();

    assert_eq!(format!("{error:#}"), expected);
}

#[test]
fn distinct_property_keys_are_kept_in_key_order() {
    let mut properties = BTreeMap::new();

    insert_property(&mut properties, "source".to_owned(), "cli".to_owned()).unwrap();
    insert_property(&mut properties, "run".to_owned(), "7".to_owned()).unwrap();

    assert_eq!(
        properties.into_iter().collect::<Vec<_>>(),
        [
            ("run".to_owned(), "7".to_owned()),
            ("source".to_owned(), "cli".to_owned())
        ]
    );
}

#[test]
fn a_repeated_property_key_fails_naming_the_key_but_no_value() {
    let mut properties = BTreeMap::new();
    insert_property(&mut properties, "run".to_owned(), "first-value".to_owned()).unwrap();

    let error =
        insert_property(&mut properties, "run".to_owned(), "second-value".to_owned()).unwrap_err();

    assert_eq!(format!("{error:#}"), "the key 'run' is given twice");
    assert_eq!(properties["run"], "first-value");
}

fn assert_send<T: Send>(_: &T) {}

#[test]
fn the_send_future_is_send() {
    fn check(producer: &EventHubProducer, plan: &SendPlan, cancel: &AtomicBool) {
        assert_send(&producer.send(plan, numbered, cancel, |_| {}));
    }
    let _: fn(&EventHubProducer, &SendPlan, &AtomicBool) = check;
}

#[tokio::test]
async fn a_namespace_outside_the_allowlist_fails_before_any_network() {
    let target = EventHubTarget {
        auth: AuthMethod::Credential {
            kind: CredentialKind::DeveloperTools,
        },
        client_secret: Some(SECRET_KEY.to_owned().into()),
        fully_qualified_namespace: Some("evil.example.com".to_owned()),
        event_hub_name: Some("orders".to_owned()),
        consumer_group: DEFAULT_CONSUMER_GROUP.to_owned(),
    };

    let error = match open_producer(&target).await {
        Ok(_) => panic!("expected the target to be refused"),
        Err(error) => error,
    };

    let rendered = format!("{error:#} {error:?}");
    assert!(
        rendered.contains("does not have one of the required suffixes"),
        "{rendered}"
    );
    assert!(!rendered.contains(SECRET_KEY), "{rendered}");
}

const RENDER_CAP: usize = 1024 * 1024;

/// A fixed render time: the exact text a clock placeholder prints is the behavior under test.
fn render_time() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-11-14T23:46:40.123Z")
        .unwrap()
        .with_timezone(&Utc)
}

#[test]
fn without_a_template_the_event_is_the_body_as_given() {
    let event = OutgoingEvent {
        body: b"{{person.email}}".to_vec(),
        content_type: Some("text/plain".to_owned()),
        ..OutgoingEvent::default()
    };

    let next = next_event(&event, None, RENDER_CAP, render_time()).unwrap();

    assert_eq!(next, event);
}

#[test]
fn a_clock_placeholder_renders_the_given_instant() {
    let template = Template::compile("{\"at\": \"{{utc-now}}\"}").unwrap();
    let event = OutgoingEvent {
        content_type: Some("application/json".to_owned()),
        ..OutgoingEvent::default()
    };

    let next = next_event(&event, Some(&template), RENDER_CAP, render_time()).unwrap();

    assert_eq!(next.body, b"{\"at\": \"2026-11-14T23:46:40.123Z\"}");
    assert_eq!(next.content_type.as_deref(), Some("application/json"));
}

#[test]
fn with_a_template_the_event_is_a_render_under_the_same_content_type() {
    let template = Template::compile("{\"n\": {{random.integer min=4 max=4}}}").unwrap();
    let event = OutgoingEvent {
        body: b"ignored".to_vec(),
        content_type: Some("application/json".to_owned()),
        ..OutgoingEvent::default()
    };

    let next = next_event(&event, Some(&template), RENDER_CAP, render_time()).unwrap();

    assert_eq!(next.body, b"{\"n\": 4}");
    assert_eq!(next.content_type.as_deref(), Some("application/json"));
}

#[test]
fn a_rendered_event_keeps_the_correlation_id_and_properties() {
    let template = Template::compile("{{random.integer min=4 max=4}}").unwrap();
    let event = OutgoingEvent {
        correlation_id: Some("run-7".to_owned()),
        properties: two_properties(),
        ..event("ignored")
    };

    let next = next_event(&event, Some(&template), RENDER_CAP, render_time()).unwrap();

    assert_eq!(next.body, b"4");
    assert_eq!(next.correlation_id.as_deref(), Some("run-7"));
    assert_eq!(next.properties, two_properties());
}

#[test]
fn a_render_past_the_cap_fails_naming_the_placeholder_and_not_the_text() {
    let template = Template::compile("secret-literal {{internet.password length=64}}").unwrap();
    let event = OutgoingEvent::default();

    let error = next_event(&event, Some(&template), 32, render_time()).unwrap_err();

    let rendered = format!("{error:#}");
    assert!(
        rendered.contains("line 1, column 16: internet.password"),
        "{rendered}"
    );
    assert!(!rendered.contains("secret-literal"), "{rendered}");
}

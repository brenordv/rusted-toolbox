//! Publishing events to an event hub: what to send (a fixed body or a fresh
//! template render per event), how often, and the producer that sends it in
//! batches.

mod send_loop;

use crate::connection::{EventHubTarget, OpenClient, auth_method_name, resolve_target};
use anyhow::Context;
use azure_core::credentials::TokenCredential;
use azure_messaging_eventhubs::builders::ProducerClientBuilder;
use azure_messaging_eventhubs::models::EventData;
use azure_messaging_eventhubs::{EventDataBatch, EventDataBatchOptions, ProducerClient};
use chrono::{DateTime, Utc};
use mock_data_utils::template::Template;
use std::fmt;
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;
use tracing::{debug, info};

/// The most events one run sends.
pub const MAX_SEND_COUNT: u64 = 1_000_000;
/// The longest pause between two paced events, in milliseconds.
pub const MAX_SEND_INTERVAL_MS: u64 = 3_600_000;
/// The longest pause between two paced events.
pub const MAX_SEND_INTERVAL: Duration = Duration::from_millis(MAX_SEND_INTERVAL_MS);

/// One event to publish. `Debug` shows the body's length and the content
/// type, never the body.
#[derive(Clone, PartialEq, Eq)]
pub struct OutgoingEvent {
    pub body: Vec<u8>,
    pub content_type: Option<String>,
}

impl fmt::Debug for OutgoingEvent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OutgoingEvent")
            .field("body_len", &self.body.len())
            .field("content_type", &self.content_type)
            .finish()
    }
}

impl OutgoingEvent {
    /// The SDK event for this body and content type.
    fn to_event_data(&self) -> EventData {
        let builder = EventData::builder().with_body(self.body.clone());
        match &self.content_type {
            Some(content_type) => builder.with_content_type(content_type.clone()).build(),
            None => builder.build(),
        }
    }
}

/// The event to send: `event` as given, or with `template` a fresh render
/// of it (at most `cap` bytes, with `now` as the time its `{{now}}` and
/// `{{utc-now}}` placeholders show) under `event`'s content type.
///
/// # Errors
/// Fails when the render fails; the error names the placeholder by its
/// position and type, never the template's text.
pub fn next_event(
    event: &OutgoingEvent,
    template: Option<&Template>,
    cap: usize,
    now: DateTime<Utc>,
) -> anyhow::Result<OutgoingEvent> {
    match template {
        Some(template) => Ok(OutgoingEvent {
            body: template.render(cap, now)?.into_bytes(),
            content_type: event.content_type.clone(),
        }),
        None => Ok(event.clone()),
    }
}

/// Where the service places the events: any partition it picks, or the
/// partition a key hashes to (every event with the same key lands in the
/// same partition, in order).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Routing {
    partition_key: Option<String>,
}

impl Routing {
    /// The service picks the partition.
    pub fn automatic() -> Self {
        Self::default()
    }

    /// Every event carries `key` as its partition key.
    ///
    /// # Errors
    /// Fails when `key` is empty or only whitespace.
    pub fn partition_key(key: impl Into<String>) -> anyhow::Result<Self> {
        let key = key.into();
        if key.trim().is_empty() {
            anyhow::bail!("The partition key is empty");
        }
        Ok(Self {
            partition_key: Some(key),
        })
    }

    /// The partition key, when there is one.
    pub fn key(&self) -> Option<&str> {
        self.partition_key.as_deref()
    }

    fn describe(&self) -> &'static str {
        match self.partition_key {
            Some(_) => "partition-key",
            None => "automatic",
        }
    }
}

/// How fast a run sends: as many events per batch as fit, or one event at a
/// time with a fixed interval between the starts of two sends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pacing {
    interval: Option<Duration>,
}

impl Pacing {
    /// As fast as batches go.
    pub fn batched() -> Self {
        Self::default()
    }

    /// One event per batch, each send starting `interval` after the previous
    /// one started (later when a send takes longer than that).
    ///
    /// # Errors
    /// Fails when `interval` is under 1 ms or above [`MAX_SEND_INTERVAL`].
    pub fn interval(interval: Duration) -> anyhow::Result<Self> {
        if interval < Duration::from_millis(1) || interval > MAX_SEND_INTERVAL {
            anyhow::bail!(
                "The interval must be between 1 ms and {MAX_SEND_INTERVAL_MS} ms (one hour)"
            );
        }
        Ok(Self {
            interval: Some(interval),
        })
    }

    /// The interval between sends, when the run is paced.
    pub fn get(self) -> Option<Duration> {
        self.interval
    }

    /// "batched" or "every <n> ms".
    pub fn describe(self) -> String {
        match self.interval {
            Some(interval) => format!("every {} ms", interval.as_millis()),
            None => "batched".to_owned(),
        }
    }
}

/// What one run sends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SendPlan {
    count: u64,
    routing: Routing,
    pacing: Pacing,
}

impl SendPlan {
    /// A plan for `count` events.
    ///
    /// # Errors
    /// Fails when `count` is 0 or above [`MAX_SEND_COUNT`].
    pub fn new(count: u64, routing: Routing, pacing: Pacing) -> anyhow::Result<Self> {
        if !(1..=MAX_SEND_COUNT).contains(&count) {
            anyhow::bail!("The count must be between 1 and {MAX_SEND_COUNT}");
        }
        Ok(Self {
            count,
            routing,
            pacing,
        })
    }

    /// How many events the run sends.
    pub fn count(&self) -> u64 {
        self.count
    }

    pub fn routing(&self) -> &Routing {
        &self.routing
    }

    pub fn pacing(&self) -> Pacing {
        self.pacing
    }
}

/// Counts after a sent batch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SendProgress {
    /// Events in batches the service accepted (or did not refuse).
    pub sent: u64,
    pub batches: u64,
    pub elapsed: Duration,
}

/// How a run ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SendEnd {
    /// Every planned event was sent.
    Completed,
    /// The cancel flag was set; nothing after the last sent batch was sent.
    Cancelled,
    /// The run stopped on this failure (no secret or body content in it).
    Failed(String),
}

/// The final counts and how the run ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SendSummary {
    pub sent: u64,
    pub batches: u64,
    pub elapsed: Duration,
    pub end: SendEnd,
}

/// An open producer client for one event hub, with the names it resolved.
pub struct EventHubProducer {
    client: ProducerClient,
    pub fully_qualified_namespace: String,
    pub event_hub_name: String,
}

/// Validates `target` (the consumer group is ignored) and opens a producer
/// for it.
///
/// Opening only establishes the AMQP connection; the credential is first used
/// when the first batch is created. For Key Vault auth the connection string
/// is fetched first and never stored beyond this call.
///
/// # Errors
/// Fails when the target is incomplete or inconsistent, when the namespace
/// host is outside the allowlist, when a credential cannot be built or the
/// Key Vault fetch fails, and when the SDK cannot open the client. Errors
/// name the namespace and event hub and never contain secret material.
pub async fn open_producer(target: &EventHubTarget) -> anyhow::Result<EventHubProducer> {
    let resolved = resolve_target(target).await?;
    info!(
        namespace = %resolved.fully_qualified_namespace,
        event_hub = %resolved.event_hub_name,
        auth = auth_method_name(&target.auth),
        "Opening an Event Hubs producer"
    );
    let client = resolved
        .open(ProducerClient::builder(), target.client_secret.clone())
        .await?;

    Ok(EventHubProducer {
        client,
        fully_qualified_namespace: resolved.fully_qualified_namespace,
        event_hub_name: resolved.event_hub_name,
    })
}

impl EventHubProducer {
    /// Sends `plan.count()` events, asking `next_event` for each one by its
    /// zero-based index, and calls `progress` after every sent batch.
    ///
    /// Batched runs pack as many events per batch as fit; paced runs send one
    /// event per batch and wait for the interval between sends. The run stops
    /// at the first failure (nothing is retried here: the SDK already retried
    /// a failed send). Setting `cancel` stops before the next batch, during a
    /// paced wait, or while a batch is being created; a batch already handed
    /// to the SDK is finished first, which can take as long as the SDK's
    /// retries (about a minute).
    pub async fn send(
        &self,
        plan: &SendPlan,
        next_event: impl FnMut(u64) -> anyhow::Result<OutgoingEvent> + Send,
        cancel: &AtomicBool,
        progress: impl FnMut(&SendProgress) + Send,
    ) -> SendSummary {
        send_loop::run(&self.client, plan, next_event, cancel, progress).await
    }

    /// Closes the connection. Dropping the producer does not reliably close
    /// it, so every producer that was opened is closed here.
    ///
    /// # Errors
    /// Fails when the SDK reports a failed close.
    pub async fn close(self) -> anyhow::Result<()> {
        self.client.close().await.with_context(|| {
            format!(
                "Failed to close the Event Hubs producer for '{}/{}'",
                self.fully_qualified_namespace, self.event_hub_name
            )
        })?;
        debug!("Producer closed");
        Ok(())
    }
}

impl OpenClient for ProducerClientBuilder {
    type Client = ProducerClient;

    async fn with_connection_string(
        self,
        connection_string: &str,
        event_hub_name: &str,
    ) -> azure_messaging_eventhubs::Result<ProducerClient> {
        self.open_with_connection_string(connection_string, Some(event_hub_name))
            .await
    }

    async fn with_credential(
        self,
        fully_qualified_namespace: &str,
        event_hub_name: &str,
        credential: Arc<dyn TokenCredential>,
    ) -> azure_messaging_eventhubs::Result<ProducerClient> {
        self.open(fully_qualified_namespace, event_hub_name, credential)
            .await
    }
}

/// Creates batches for a routing and sends them.
pub(crate) trait Publisher: Sync {
    type Batch<'a>: PublishBatch + Send
    where
        Self: 'a;

    fn create_batch(
        &self,
        routing: &Routing,
    ) -> impl Future<Output = anyhow::Result<Self::Batch<'_>>> + Send;

    fn send_batch<'a>(
        &'a self,
        batch: Self::Batch<'a>,
    ) -> impl Future<Output = anyhow::Result<()>> + Send + 'a;
}

/// The SDK batch options for `routing`: its partition key, and no partition
/// id or size override.
fn batch_options(routing: &Routing) -> EventDataBatchOptions {
    EventDataBatchOptions {
        partition_key: routing.key().map(str::to_owned),
        ..EventDataBatchOptions::default()
    }
}

/// A batch being filled.
pub(crate) trait PublishBatch {
    /// Adds `event` when it fits; `Ok(false)` leaves the batch unchanged.
    fn try_add(&self, event: EventData) -> anyhow::Result<bool>;
}

impl Publisher for ProducerClient {
    type Batch<'a> = EventDataBatch<'a>;

    async fn create_batch(&self, routing: &Routing) -> anyhow::Result<EventDataBatch<'_>> {
        Ok(ProducerClient::create_batch(self, Some(batch_options(routing))).await?)
    }

    async fn send_batch<'a>(&'a self, batch: EventDataBatch<'a>) -> anyhow::Result<()> {
        Ok(ProducerClient::send_batch(self, batch, None).await?)
    }
}

impl PublishBatch for EventDataBatch<'_> {
    fn try_add(&self, event: EventData) -> anyhow::Result<bool> {
        Ok(self.try_add_event_data(event, None)?)
    }
}

#[cfg(test)]
mod tests;

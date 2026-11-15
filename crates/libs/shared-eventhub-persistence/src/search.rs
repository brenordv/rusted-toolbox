//! Searching the archive by enqueue-time window and message text.

use crate::store::MessageStore;
use crate::store::schema::{BY_TIME, MESSAGES, NO_ENQUEUED_TIME, decode_message};
use anyhow::Context;
use shared_eventhub_core::filters::CaptureFilters;
use shared_eventhub_core::models::CapturedMessage;
use std::num::NonZeroUsize;
use std::ops::Bound;

/// What to look for in the archive. The filters keep exactly the semantics
/// they have during capture; an unset filter matches everything.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SearchQuery {
    pub filters: CaptureFilters,
    /// Most messages per page; `None` returns every match in one page, which
    /// suits small archives and tests.
    pub limit: Option<NonZeroUsize>,
    /// Continue after this position (the `next` of the previous page).
    pub after: Option<SearchCursor>,
}

/// A position in the archive's time order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchCursor {
    enqueued_ms: i64,
    partition_id: String,
    sequence_number: i64,
}

/// One page of search results, in enqueue-time order (messages without an
/// enqueue time first).
#[derive(Debug)]
pub struct SearchPage {
    pub items: Vec<CapturedMessage>,
    /// Where the next page starts; `None` when no further message matches.
    pub next: Option<SearchCursor>,
    /// Index entries examined to build this page.
    pub scanned: u64,
}

impl MessageStore {
    /// Returns the next page of messages matching `query`. Each call reads
    /// its own snapshot, so a message overwritten between pages can move
    /// across the cursor.
    ///
    /// # Errors
    /// Fails when the archive cannot be read or a stored message does not
    /// decode.
    pub fn search(&self, query: &SearchQuery) -> anyhow::Result<SearchPage> {
        let read = self.begin_read()?;
        let by_time = read
            .open_table(BY_TIME)
            .context("Failed to open the archive's time index")?;
        let messages = read
            .open_table(MESSAGES)
            .context("Failed to open the archive's messages table")?;
        let (lower, upper) = scan_bounds(&query.filters);
        let start = match &query.after {
            Some(cursor) => Bound::Excluded((
                cursor.enqueued_ms,
                cursor.partition_id.as_str(),
                cursor.sequence_number,
            )),
            None => Bound::Included((lower, "", i64::MIN)),
        };

        let mut page = SearchPage {
            items: Vec::new(),
            next: None,
            scanned: 0,
        };
        let entries = by_time
            .range::<(i64, &str, i64)>((start, Bound::Unbounded))
            .context("Failed to scan the archive's time index")?;
        for entry in entries {
            let (key, _) = entry.context("Failed to read the archive's time index")?;
            let (enqueued_ms, partition_id, sequence_number) = key.value();
            if upper.is_some_and(|upper| enqueued_ms > upper) {
                break;
            }
            page.scanned += 1;

            let stored = messages
                .get((partition_id, sequence_number))
                .context("Failed to read a stored message")?
                .with_context(|| {
                    format!(
                        "The archive's time index lists partition {partition_id}, sequence \
                         {sequence_number}, but the message is missing"
                    )
                })?;
            let message = decode_message(partition_id, sequence_number, stored.value())?;
            if !query.filters.matches(&message) {
                continue;
            }
            if query
                .limit
                .is_some_and(|limit| page.items.len() == limit.get())
            {
                page.next = page.items.last().map(cursor_of);
                break;
            }
            page.items.push(message);
        }

        Ok(page)
    }
}

/// The time-index range that can hold a match: from the lower bound (or the
/// first real time when only an upper bound is set) to the upper bound, both
/// floored to milliseconds, so the range is always a superset of the matches.
fn scan_bounds(filters: &CaptureFilters) -> (i64, Option<i64>) {
    let lower = match (filters.time.is_some(), filters.lower_time_bound()) {
        (_, Some(bound)) => bound.timestamp_millis(),
        (true, None) => NO_ENQUEUED_TIME + 1,
        (false, None) => NO_ENQUEUED_TIME,
    };
    let upper = filters
        .upper_time_bound()
        .map(|bound| bound.timestamp_millis());

    (lower, upper)
}

fn cursor_of(message: &CapturedMessage) -> SearchCursor {
    SearchCursor {
        enqueued_ms: crate::store::schema::time_key(message),
        partition_id: message.key.partition_id.clone(),
        sequence_number: message.key.sequence_number,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::DataPaths;
    use crate::store::schema::test_messages::{checkpoint_for, instant, message};
    use chrono::{DateTime, Duration, Utc};
    use rstest::rstest;
    use shared_eventhub_core::connection::EventHubIdentity;
    use shared_eventhub_core::engine::MessageSink;
    use shared_eventhub_core::filters::{TextFilter, TimeFilter};

    async fn archive_with(messages: &[CapturedMessage]) -> (tempfile::TempDir, MessageStore) {
        let root = tempfile::tempdir().unwrap();
        let store = MessageStore::open(&DataPaths::new(root.path())).unwrap();
        store
            .bind(&EventHubIdentity {
                fully_qualified_namespace: "orders.servicebus.windows.net".to_owned(),
                event_hub_name: "orders".to_owned(),
                created_on: None,
            })
            .await
            .unwrap();
        for message in messages {
            store
                .store(message, &checkpoint_for(message), true)
                .await
                .unwrap();
        }
        (root, store)
    }

    fn enqueued_at(partition: &str, sequence: i64, at: DateTime<Utc>) -> CapturedMessage {
        let mut message = message(partition, sequence, &format!("body {partition}-{sequence}"));
        message.enqueued_time = Some(at);
        message
    }

    fn sequences(page: &SearchPage) -> Vec<(String, i64)> {
        page.items
            .iter()
            .map(|message| {
                (
                    message.key.partition_id.clone(),
                    message.key.sequence_number,
                )
            })
            .collect()
    }

    fn time_filter(filter: TimeFilter) -> SearchQuery {
        SearchQuery {
            filters: CaptureFilters {
                time: Some(filter),
                text: None,
            },
            ..SearchQuery::default()
        }
    }

    #[rstest]
    #[case::newer_or_equal(TimeFilter::NewerOrEqual(instant(12, 0, 2)), vec![2, 3, 4])]
    #[case::newer_than(TimeFilter::NewerThan(instant(12, 0, 2)), vec![3, 4])]
    #[case::older_than(TimeFilter::OlderThan(instant(12, 0, 2)), vec![1])]
    #[case::older_or_equal(TimeFilter::OlderOrEqual(instant(12, 0, 2)), vec![1, 2])]
    #[case::between(TimeFilter::Between { start: instant(12, 0, 2), end: instant(12, 0, 3) }, vec![2, 3])]
    #[case::during_second(TimeFilter::During(instant(12, 0, 3)), vec![3])]
    #[tokio::test]
    async fn time_filters_select_exactly_the_matching_messages(
        #[case] filter: TimeFilter,
        #[case] expected: Vec<i64>,
    ) {
        let messages: Vec<CapturedMessage> = (1..=4)
            .map(|sequence| {
                enqueued_at(
                    "0",
                    sequence,
                    instant(12, 0, 0) + Duration::seconds(sequence),
                )
            })
            .collect();
        let (_root, store) = archive_with(&messages).await;

        let page = store.search(&time_filter(filter)).unwrap();

        let found: Vec<i64> = page
            .items
            .iter()
            .map(|message| message.key.sequence_number)
            .collect();
        assert_eq!(found, expected);
    }

    #[tokio::test]
    async fn messages_without_an_enqueue_time_match_only_unfiltered_searches() {
        let mut timeless = message("0", 9, "no time");
        timeless.enqueued_time = None;
        let timed = enqueued_at("0", 1, instant(12, 0, 0));
        let (_root, store) = archive_with(&[timed, timeless]).await;

        let unfiltered = store.search(&SearchQuery::default()).unwrap();
        let filtered = store
            .search(&time_filter(TimeFilter::OlderThan(instant(23, 0, 0))))
            .unwrap();

        assert_eq!(
            sequences(&unfiltered),
            vec![("0".to_owned(), 9), ("0".to_owned(), 1)]
        );
        assert_eq!(sequences(&filtered), vec![("0".to_owned(), 1)]);
    }

    #[tokio::test]
    async fn text_filters_apply_to_the_body() {
        let mut bacon = message("0", 1, "Crispy BACON");
        bacon.enqueued_time = Some(instant(12, 0, 1));
        let mut eggs = message("0", 2, "eggs order-17");
        eggs.enqueued_time = Some(instant(12, 0, 2));
        let (_root, store) = archive_with(&[bacon, eggs]).await;

        let contains = store
            .search(&SearchQuery {
                filters: CaptureFilters {
                    time: None,
                    text: Some(TextFilter::Contains(vec!["bacon".to_owned()])),
                },
                ..SearchQuery::default()
            })
            .unwrap();
        let regex = store
            .search(&SearchQuery {
                filters: CaptureFilters {
                    time: None,
                    text: Some(TextFilter::regex(r"order-\d+").unwrap()),
                },
                ..SearchQuery::default()
            })
            .unwrap();

        assert_eq!(sequences(&contains), vec![("0".to_owned(), 1)]);
        assert_eq!(sequences(&regex), vec![("0".to_owned(), 2)]);
    }

    #[tokio::test]
    async fn paging_walks_every_match_once_across_a_shared_millisecond() {
        let shared = instant(12, 0, 5);
        let mut messages = vec![
            enqueued_at("0", 1, instant(12, 0, 1)),
            enqueued_at("0", 2, shared),
            enqueued_at("1", 2, shared),
            enqueued_at("2", 2, shared),
            enqueued_at("3", 2, shared),
            enqueued_at("0", 3, instant(12, 0, 9)),
        ];
        let mut timeless = message("1", 1, "no time");
        timeless.enqueued_time = None;
        messages.push(timeless);
        let (_root, store) = archive_with(&messages).await;
        let mut query = SearchQuery {
            limit: NonZeroUsize::new(2),
            ..SearchQuery::default()
        };

        let mut walked = Vec::new();
        let mut pages = 0;
        loop {
            let page = store.search(&query).unwrap();
            pages += 1;
            walked.extend(sequences(&page));
            match page.next {
                Some(next) => query.after = Some(next),
                None => break,
            }
        }

        assert_eq!(pages, 4);
        assert_eq!(
            walked,
            vec![
                ("1".to_owned(), 1),
                ("0".to_owned(), 1),
                ("0".to_owned(), 2),
                ("1".to_owned(), 2),
                ("2".to_owned(), 2),
                ("3".to_owned(), 2),
                ("0".to_owned(), 3),
            ]
        );
    }

    #[tokio::test]
    async fn a_page_that_ends_on_the_last_match_has_no_next() {
        let messages: Vec<CapturedMessage> = (1..=2)
            .map(|sequence| {
                enqueued_at(
                    "0",
                    sequence,
                    instant(12, 0, 0) + Duration::seconds(sequence),
                )
            })
            .collect();
        let (_root, store) = archive_with(&messages).await;

        let page = store
            .search(&SearchQuery {
                limit: NonZeroUsize::new(2),
                ..SearchQuery::default()
            })
            .unwrap();

        assert_eq!(page.items.len(), 2);
        assert_eq!(page.next, None);
    }

    #[tokio::test]
    async fn an_upper_bound_stops_the_scan_early() {
        let messages: Vec<CapturedMessage> = (1..=5)
            .map(|sequence| {
                enqueued_at(
                    "0",
                    sequence,
                    instant(12, 0, 0) + Duration::seconds(sequence),
                )
            })
            .collect();
        let (_root, store) = archive_with(&messages).await;

        let page = store
            .search(&time_filter(TimeFilter::OlderOrEqual(instant(12, 0, 2))))
            .unwrap();

        assert_eq!(page.items.len(), 2);
        assert_eq!(page.scanned, 2);
    }

    #[tokio::test]
    async fn a_time_index_entry_without_its_message_is_an_error() {
        let present = enqueued_at("0", 1, instant(12, 0, 1));
        let (_root, store) = archive_with(std::slice::from_ref(&present)).await;
        let transaction = store.database().begin_write().unwrap();
        transaction
            .open_table(MESSAGES)
            .unwrap()
            .remove(("0", 1))
            .unwrap();
        transaction.commit().unwrap();

        let error = store.search(&SearchQuery::default()).unwrap_err();

        assert!(
            error.to_string().contains("message is missing"),
            "{error:#}"
        );
    }
}

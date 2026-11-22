use super::schema::test_messages::{checkpoint_for, instant, message};
use super::schema::{BY_TIME, MESSAGES, NO_ENQUEUED_TIME, encode_created_on};
use super::*;
use std::collections::BTreeSet;

/// When the test event hub was created: before every test message's enqueue time.
fn orders_created_on() -> DateTime<Utc> {
    instant(9, 0, 0)
}

pub(super) fn orders_hub() -> EventHubIdentity {
    orders_hub_created(Some(orders_created_on()))
}

fn orders_hub_created(created_on: Option<DateTime<Utc>>) -> EventHubIdentity {
    EventHubIdentity {
        fully_qualified_namespace: "orders.servicebus.windows.net".to_owned(),
        event_hub_name: "orders".to_owned(),
        created_on,
    }
}

fn open_store(root: &tempfile::TempDir) -> MessageStore {
    MessageStore::open(&DataPaths::new(root.path().join("data"))).unwrap()
}

async fn bound_store(root: &tempfile::TempDir) -> MessageStore {
    let store = open_store(root);
    store.bind(&orders_hub()).await.unwrap();
    store
}

pub(super) async fn store_one(
    store: &MessageStore,
    message: &CapturedMessage,
    dedup: bool,
) -> anyhow::Result<StoreOutcome> {
    store.store(message, &checkpoint_for(message), dedup).await
}

fn time_index(store: &MessageStore) -> BTreeSet<(i64, String, i64)> {
    let read = store.begin_read().unwrap();
    let by_time = read.open_table(BY_TIME).unwrap();
    by_time
        .iter()
        .unwrap()
        .map(|entry| {
            let (key, _) = entry.unwrap();
            let (time, partition, sequence) = key.value();
            (time, partition.to_owned(), sequence)
        })
        .collect()
}

fn message_keys(store: &MessageStore) -> BTreeSet<(String, i64)> {
    let read = store.begin_read().unwrap();
    let messages = read.open_table(MESSAGES).unwrap();
    messages
        .iter()
        .unwrap()
        .map(|entry| {
            let (key, _) = entry.unwrap();
            let (partition, sequence) = key.value();
            (partition.to_owned(), sequence)
        })
        .collect()
}

#[tokio::test]
async fn a_stored_message_reads_back_identically_right_away() {
    let root = tempfile::tempdir().unwrap();
    let store = bound_store(&root).await;
    let stored = message("0", 1, "hello");

    let outcome = store_one(&store, &stored, true).await.unwrap();

    assert!(outcome.newly_stored);
    assert_eq!(store.get(&stored.key).unwrap(), Some(stored));
    assert_eq!(store.message_count().unwrap(), 1);
}

#[tokio::test]
async fn storing_the_same_key_again_with_dedup_is_not_new() {
    let root = tempfile::tempdir().unwrap();
    let store = bound_store(&root).await;
    let first = message("0", 1, "first");
    let mut second = first.clone();
    second.body = "second".to_owned();
    store_one(&store, &first, true).await.unwrap();

    let outcome = store_one(&store, &second, true).await.unwrap();

    assert!(!outcome.newly_stored);
    assert_eq!(store.get(&first.key).unwrap(), Some(first));
}

#[tokio::test]
async fn storing_the_same_event_without_dedup_overwrites_it() {
    let root = tempfile::tempdir().unwrap();
    let store = bound_store(&root).await;
    let first = message("0", 1, "first");
    let mut second = first.clone();
    second.body = "second".to_owned();
    store_one(&store, &first, false).await.unwrap();

    let outcome = store_one(&store, &second, false).await.unwrap();

    assert!(outcome.newly_stored);
    assert_eq!(store.get(&first.key).unwrap(), Some(second));
    assert_eq!(store.message_count().unwrap(), 1);
}

#[tokio::test]
async fn an_overwrite_that_adds_an_enqueue_time_moves_the_time_index_entry() {
    let root = tempfile::tempdir().unwrap();
    let store = bound_store(&root).await;
    let mut without_time = message("0", 1, "x");
    without_time.enqueued_time = None;
    let with_time = message("0", 1, "x");
    store_one(&store, &without_time, false).await.unwrap();

    store_one(&store, &with_time, false).await.unwrap();

    let expected_time = with_time.enqueued_time.unwrap().timestamp_millis();
    assert_eq!(
        time_index(&store),
        BTreeSet::from([(expected_time, "0".to_owned(), 1)])
    );
}

#[tokio::test]
async fn a_different_event_at_the_same_key_is_refused_even_with_dedup_on() {
    let root = tempfile::tempdir().unwrap();
    let store = bound_store(&root).await;
    let original = message("0", 5000, "original");
    let mut recreated = message("0", 5000, "recreated");
    recreated.enqueued_time = original
        .enqueued_time
        .map(|time| time + chrono::Duration::days(1));
    store_one(&store, &original, true).await.unwrap();

    let error = store_one(&store, &recreated, true).await.unwrap_err();

    assert!(
        error.to_string().contains("likely deleted and recreated"),
        "{error:#}"
    );
    assert_eq!(store.get(&original.key).unwrap(), Some(original));
}

#[tokio::test]
async fn a_different_event_at_the_same_key_is_refused_and_nothing_changes() {
    let root = tempfile::tempdir().unwrap();
    let store = bound_store(&root).await;
    let original = message("0", 1, "original");
    let mut recreated = message("0", 1, "recreated");
    recreated.enqueued_time = original
        .enqueued_time
        .map(|time| time + chrono::Duration::days(1));
    store_one(&store, &original, false).await.unwrap();
    let index_before = time_index(&store);

    let error = store_one(&store, &recreated, false).await.unwrap_err();

    assert!(
        error.to_string().contains("likely deleted and recreated"),
        "{error:#}"
    );
    assert_eq!(store.get(&original.key).unwrap(), Some(original.clone()));
    assert_eq!(time_index(&store), index_before);
    assert_eq!(
        store.load_checkpoint("0").await.unwrap(),
        Some(checkpoint_for(&original))
    );
}

#[tokio::test]
async fn a_stored_value_that_does_not_decode_is_never_overwritten() {
    let root = tempfile::tempdir().unwrap();
    let store = bound_store(&root).await;
    let transaction = store.database().begin_write().unwrap();
    transaction
        .open_table(MESSAGES)
        .unwrap()
        .insert(("0", 1), b"not json".as_slice())
        .unwrap();
    transaction.commit().unwrap();

    let error = store_one(&store, &message("0", 1, "x"), false)
        .await
        .unwrap_err();

    assert!(
        error.to_string().contains("could not be decoded"),
        "{error:#}"
    );
    let read = store.begin_read().unwrap();
    let messages = read.open_table(MESSAGES).unwrap();
    assert_eq!(
        messages.get(("0", 1)).unwrap().unwrap().value(),
        b"not json"
    );
}

#[tokio::test]
async fn a_refused_store_does_not_stop_other_stores() {
    let root = tempfile::tempdir().unwrap();
    let store = bound_store(&root).await;
    let original = message("0", 1, "original");
    let mut recreated = message("0", 1, "recreated");
    recreated.enqueued_time = original
        .enqueued_time
        .map(|time| time + chrono::Duration::days(1));
    let other = message("1", 1, "other partition");
    store_one(&store, &original, false).await.unwrap();

    let (refused, accepted) = tokio::join!(
        store_one(&store, &recreated, false),
        store_one(&store, &other, false)
    );

    assert!(refused.is_err());
    assert!(accepted.unwrap().newly_stored);
    assert_eq!(store.get(&other.key).unwrap(), Some(other));
}

#[tokio::test]
async fn the_checkpoint_stored_with_a_message_loads_back() {
    let root = tempfile::tempdir().unwrap();
    let store = bound_store(&root).await;
    let first = message("0", 1, "a");
    let last = message("0", 2, "b");
    store_one(&store, &first, true).await.unwrap();
    store_one(&store, &last, true).await.unwrap();

    let checkpoint = store.load_checkpoint("0").await.unwrap();

    assert_eq!(checkpoint, Some(checkpoint_for(&last)));
    assert_eq!(store.load_checkpoint("7").await.unwrap(), None);
}

#[tokio::test]
async fn stores_survive_a_close_and_a_reopen() {
    let root = tempfile::tempdir().unwrap();
    let store = bound_store(&root).await;
    let stored = message("0", 1, "kept");
    store_one(&store, &stored, true).await.unwrap();

    store.close().await.unwrap();
    let reopened = open_store(&root);

    assert_eq!(reopened.get(&stored.key).unwrap(), Some(stored));
    assert_eq!(reopened.binding().unwrap(), Some(orders_hub()));
}

#[tokio::test]
async fn stores_survive_a_drop_and_a_reopen() {
    let root = tempfile::tempdir().unwrap();
    let store = bound_store(&root).await;
    let stored = message("0", 1, "kept");
    store_one(&store, &stored, true).await.unwrap();

    drop(store);
    let reopened = open_store(&root);

    assert_eq!(reopened.get(&stored.key).unwrap(), Some(stored));
}

#[tokio::test]
async fn concurrent_stores_all_land() {
    let root = tempfile::tempdir().unwrap();
    let store = bound_store(&root).await;
    let messages: Vec<CapturedMessage> = (0..8)
        .flat_map(|partition| {
            (1..=25).map(move |sequence| message(&partition.to_string(), sequence, "x"))
        })
        .collect();

    let tasks: Vec<_> = (0..8)
        .map(|partition| {
            let store = store.clone();
            let mine: Vec<CapturedMessage> = messages
                .iter()
                .filter(|message| message.key.partition_id == partition.to_string())
                .cloned()
                .collect();
            tokio::spawn(async move {
                for message in mine {
                    store_one(&store, &message, true).await.unwrap();
                }
            })
        })
        .collect();
    for task in tasks {
        task.await.unwrap();
    }

    assert_eq!(store.message_count().unwrap(), messages.len() as u64);
    assert_eq!(message_keys(&store).len(), time_index(&store).len());
}

#[tokio::test]
async fn a_store_whose_caller_stops_waiting_still_commits() {
    let root = tempfile::tempdir().unwrap();
    let store = bound_store(&root).await;
    let abandoned = message("0", 1, "abandoned");
    let (request, reply) = WriteRequest::store(abandoned.clone(), checkpoint_for(&abandoned), true);

    store.inner.writer.submit(request).unwrap();
    drop(reply);
    store_one(&store, &message("0", 2, "after"), true)
        .await
        .unwrap();

    assert_eq!(store.get(&abandoned.key).unwrap(), Some(abandoned));
}

#[tokio::test]
async fn the_time_index_and_the_messages_stay_in_step() {
    let root = tempfile::tempdir().unwrap();
    let store = bound_store(&root).await;
    let mut without_time = message("2", 9, "x");
    without_time.enqueued_time = None;
    for sequence in 1..=40 {
        store_one(&store, &message("0", sequence, "x"), true)
            .await
            .unwrap();
    }
    store_one(&store, &without_time, true).await.unwrap();

    let indexed: BTreeSet<(String, i64)> = time_index(&store)
        .into_iter()
        .map(|(_, partition, sequence)| (partition, sequence))
        .collect();

    assert_eq!(indexed, message_keys(&store));
    assert!(time_index(&store).contains(&(NO_ENQUEUED_TIME, "2".to_owned(), 9)));
}

#[tokio::test]
async fn binding_the_same_hub_again_succeeds_whatever_its_casing() {
    let root = tempfile::tempdir().unwrap();
    let store = bound_store(&root).await;
    let shouted = EventHubIdentity {
        fully_qualified_namespace: "orders.servicebus.windows.net".to_owned(),
        event_hub_name: "ORDERS".to_owned(),
        created_on: Some(orders_created_on()),
    };

    store.bind(&shouted).await.unwrap();

    assert_eq!(store.binding().unwrap(), Some(orders_hub()));
}

#[tokio::test]
async fn binding_another_hub_is_refused_with_both_names() {
    let root = tempfile::tempdir().unwrap();
    let store = bound_store(&root).await;
    let other = EventHubIdentity {
        fully_qualified_namespace: "orders.servicebus.windows.net".to_owned(),
        event_hub_name: "payments".to_owned(),
        created_on: Some(orders_created_on()),
    };

    let error = store.bind(&other).await.unwrap_err();

    let rendered = error.to_string();
    assert!(
        rendered.contains("'orders.servicebus.windows.net/orders'"),
        "{rendered}"
    );
    assert!(
        rendered.contains("'orders.servicebus.windows.net/payments'"),
        "{rendered}"
    );
    assert_eq!(store.binding().unwrap(), Some(orders_hub()));
}

#[tokio::test]
async fn concurrent_binds_of_one_hub_all_succeed() {
    let root = tempfile::tempdir().unwrap();
    let store = open_store(&root);
    let hub = orders_hub();

    let (first, second, third) = tokio::join!(store.bind(&hub), store.bind(&hub), store.bind(&hub));

    assert!(first.is_ok() && second.is_ok() && third.is_ok());
    assert_eq!(store.binding().unwrap(), Some(orders_hub()));
}

#[tokio::test]
async fn storing_into_an_unbound_archive_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let store = open_store(&root);
    let unbound = message("0", 1, "x");

    let error = store_one(&store, &unbound, true).await.unwrap_err();

    assert!(error.to_string().contains("not bound"), "{error:#}");
    assert_eq!(store.message_count().unwrap(), 0);
}

fn stored_created_on(store: &MessageStore) -> Option<String> {
    let read = store.begin_read().unwrap();
    let meta = read.open_table(META).unwrap();
    meta.get(CREATED_ON_KEY)
        .unwrap()
        .map(|guard| guard.value().to_owned())
}

fn write_meta(store: &MessageStore, entries: &[(&str, &str)]) {
    let transaction = store.database().begin_write().unwrap();
    {
        let mut meta = transaction.open_table(META).unwrap();
        for (key, value) in entries {
            meta.insert(*key, *value).unwrap();
        }
    }
    transaction.commit().unwrap();
}

/// An archive bound by name only, holding one event: as one bound before the creation time
/// was recorded.
async fn bound_without_a_time(root: &tempfile::TempDir) -> MessageStore {
    let store = open_store(root);
    store.bind(&orders_hub_created(None)).await.unwrap();
    store_one(&store, &message("0", 1, "x"), true)
        .await
        .unwrap();
    store
}

#[tokio::test]
async fn a_first_bind_records_the_creation_time_to_the_millisecond() {
    let root = tempfile::tempdir().unwrap();
    let store = open_store(&root);
    let created_on = orders_created_on() + chrono::Duration::milliseconds(123);

    store
        .bind(&orders_hub_created(Some(created_on)))
        .await
        .unwrap();
    store.close().await.unwrap();
    let reopened = open_store(&root);

    assert_eq!(
        reopened.binding().unwrap(),
        Some(orders_hub_created(Some(created_on)))
    );
}

#[tokio::test]
async fn a_first_bind_without_a_creation_time_records_none() {
    let root = tempfile::tempdir().unwrap();
    let store = open_store(&root);

    store.bind(&orders_hub_created(None)).await.unwrap();

    assert_eq!(stored_created_on(&store), None);
    assert_eq!(store.binding().unwrap(), Some(orders_hub_created(None)));
}

#[tokio::test]
async fn a_hub_created_at_another_time_is_refused_with_both_times() {
    let root = tempfile::tempdir().unwrap();
    let store = bound_store(&root).await;
    let recreated = orders_hub_created(Some(instant(10, 0, 0) + chrono::Duration::milliseconds(5)));

    let error = store.bind(&recreated).await.unwrap_err();

    let rendered = error.to_string();
    assert!(
        rendered.contains("created on 2026-07-02 10:00:00.005 UTC"),
        "{rendered}"
    );
    assert!(
        rendered.contains("the one created on 2026-07-02 09:00:00.000 UTC"),
        "{rendered}"
    );
    assert_eq!(store.binding().unwrap(), Some(orders_hub()));
}

#[tokio::test]
async fn an_archive_bound_without_a_time_records_the_reported_one() {
    let root = tempfile::tempdir().unwrap();
    let store = bound_without_a_time(&root).await;

    store.bind(&orders_hub()).await.unwrap();

    assert_eq!(store.binding().unwrap(), Some(orders_hub()));
}

fn bind_outcome(request: &WriteRequest) -> Result<(), String> {
    match request {
        WriteRequest::Bind { outcome, .. } => {
            outcome.as_ref().map(|_| ()).map_err(|e| e.to_string())
        }
        WriteRequest::Store { .. } => panic!("expected a bind request"),
    }
}

#[tokio::test]
async fn binds_in_one_batch_compare_against_a_time_recorded_in_that_batch() {
    let root = tempfile::tempdir().unwrap();
    let store = bound_without_a_time(&root).await;
    let later = orders_hub_created(Some(orders_created_on() + chrono::Duration::seconds(30)));
    let mut batch = [
        WriteRequest::bind(orders_hub()).0,
        WriteRequest::bind(later).0,
    ];

    writer::apply_batch(store.database(), &mut batch).unwrap();

    assert_eq!(bind_outcome(&batch[0]), Ok(()));
    assert!(
        bind_outcome(&batch[1]).is_err(),
        "{:?}",
        bind_outcome(&batch[1])
    );
    assert_eq!(store.binding().unwrap(), Some(orders_hub()));
}

const OLDER_EVENTS: &str = "holds events enqueued on 2026-07-02 12:00:01.000 UTC";

#[tokio::test]
async fn an_archive_without_a_time_holding_older_events_is_refused_and_left_as_is() {
    let root = tempfile::tempdir().unwrap();
    let store = bound_without_a_time(&root).await;

    let error = store
        .bind(&orders_hub_created(Some(instant(12, 5, 0))))
        .await
        .unwrap_err();

    assert!(error.to_string().contains(OLDER_EVENTS), "{error:#}");
    assert_eq!(stored_created_on(&store), None);
    assert_eq!(store.binding().unwrap(), Some(orders_hub_created(None)));
}

#[tokio::test]
async fn an_archive_with_the_same_time_holding_older_events_is_refused_and_left_as_is() {
    let root = tempfile::tempdir().unwrap();
    let store = bound_without_a_time(&root).await;
    let created_after = instant(12, 5, 0);
    let recorded = encode_created_on(created_after);
    write_meta(&store, &[(CREATED_ON_KEY, recorded.as_str())]);

    let error = store
        .bind(&orders_hub_created(Some(created_after)))
        .await
        .unwrap_err();

    assert!(error.to_string().contains(OLDER_EVENTS), "{error:#}");
    assert_eq!(stored_created_on(&store), Some(recorded));
}

#[tokio::test]
async fn events_within_a_minute_before_the_creation_time_still_bind() {
    let root = tempfile::tempdir().unwrap();
    let store = bound_without_a_time(&root).await;
    let created_just_after = instant(12, 0, 59);

    store
        .bind(&orders_hub_created(Some(created_just_after)))
        .await
        .unwrap();

    assert_eq!(
        store.binding().unwrap(),
        Some(orders_hub_created(Some(created_just_after)))
    );
}

#[tokio::test]
async fn a_bind_without_a_reported_time_keeps_the_stored_one() {
    let root = tempfile::tempdir().unwrap();
    let store = bound_store(&root).await;

    store.bind(&orders_hub_created(None)).await.unwrap();

    assert_eq!(store.binding().unwrap(), Some(orders_hub()));
}

#[tokio::test]
async fn an_unreadable_creation_time_fails_the_binding() {
    let root = tempfile::tempdir().unwrap();
    let store = bound_store(&root).await;
    write_meta(&store, &[(CREATED_ON_KEY, "yesterday")]);

    let error = store.binding().unwrap_err();

    assert!(
        error
            .to_string()
            .contains("creation time could not be read"),
        "{error:#}"
    );
}

#[test]
fn a_creation_time_without_a_hub_is_an_incomplete_binding() {
    let root = tempfile::tempdir().unwrap();
    let store = open_store(&root);
    write_meta(&store, &[(CREATED_ON_KEY, "2026-07-02T09:00:00.000Z")]);

    let error = store.binding().unwrap_err();

    assert!(error.to_string().contains("incomplete"), "{error:#}");
}

#[test]
fn an_incomplete_binding_is_an_error() {
    let root = tempfile::tempdir().unwrap();
    let store = open_store(&root);
    let transaction = store.database().begin_write().unwrap();
    transaction
        .open_table(META)
        .unwrap()
        .insert(NAMESPACE_KEY, "orders.servicebus.windows.net")
        .unwrap();
    transaction.commit().unwrap();

    let error = store.binding().unwrap_err();

    assert!(error.to_string().contains("incomplete"), "{error:#}");
}

#[tokio::test]
async fn a_second_open_of_the_same_archive_is_refused_until_the_first_closes() {
    let root = tempfile::tempdir().unwrap();
    let paths = DataPaths::new(root.path().join("data"));
    let first = MessageStore::open(&paths).unwrap();

    let error = MessageStore::open(&paths).err().unwrap();
    first.close().await.unwrap();
    let reopened = MessageStore::open(&paths);

    let rendered = error.to_string();
    assert!(
        rendered.contains("in use by another eh or eh-gui"),
        "{rendered}"
    );
    assert!(rendered.contains("archive.redb"), "{rendered}");
    assert!(reopened.is_ok());
}

#[tokio::test]
async fn closing_while_another_handle_is_open_is_refused_and_the_other_handle_keeps_working() {
    let root = tempfile::tempdir().unwrap();
    let store = bound_store(&root).await;
    let other = store.clone();
    let stored = message("0", 1, "x");

    let error = store.close().await.unwrap_err();
    store_one(&other, &stored, true).await.unwrap();

    assert!(error.to_string().contains("still open"), "{error:#}");
    assert_eq!(other.get(&stored.key).unwrap(), Some(stored));
}

fn write_raw_database(path: &Path, table: &str, key: &str, value: &str) {
    let definition: redb::TableDefinition<&str, &str> = redb::TableDefinition::new(table);
    let database = Database::create(path).unwrap();
    let transaction = database.begin_write().unwrap();
    transaction
        .open_table(definition)
        .unwrap()
        .insert(key, value)
        .unwrap();
    transaction.commit().unwrap();
}

#[test]
fn an_archive_with_an_unknown_schema_version_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let paths = DataPaths::new(root.path());
    paths.ensure_base().unwrap();
    write_raw_database(&paths.archive_db(), "meta", "schema_version", "99");

    let error = MessageStore::open(&paths).err().unwrap();

    assert!(error.to_string().contains("schema version 99"), "{error:#}");
}

#[test]
fn a_meta_table_without_a_schema_version_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let paths = DataPaths::new(root.path());
    paths.ensure_base().unwrap();
    write_raw_database(&paths.archive_db(), "meta", "something", "else");

    let error = MessageStore::open(&paths).err().unwrap();

    assert!(error.to_string().contains("no schema version"), "{error:#}");
}

#[test]
fn a_foreign_redb_file_is_refused_and_not_stamped() {
    let root = tempfile::tempdir().unwrap();
    let paths = DataPaths::new(root.path());
    paths.ensure_base().unwrap();
    write_raw_database(&paths.archive_db(), "other", "key", "value");

    let error = MessageStore::open(&paths).err().unwrap();

    assert!(error.to_string().contains("not an eh archive"), "{error:#}");
    let database = Database::open(paths.archive_db()).unwrap();
    let tables: Vec<String> = database
        .begin_read()
        .unwrap()
        .list_tables()
        .unwrap()
        .map(|table| table.name().to_owned())
        .collect();
    assert_eq!(tables, vec!["other".to_owned()]);
}

#[test]
fn a_redb_file_holding_only_multimap_tables_is_refused_and_not_stamped() {
    let root = tempfile::tempdir().unwrap();
    let paths = DataPaths::new(root.path());
    paths.ensure_base().unwrap();
    let definition: redb::MultimapTableDefinition<&str, &str> =
        redb::MultimapTableDefinition::new("other");
    {
        let database = Database::create(paths.archive_db()).unwrap();
        let transaction = database.begin_write().unwrap();
        transaction
            .open_multimap_table(definition)
            .unwrap()
            .insert("key", "value")
            .unwrap();
        transaction.commit().unwrap();
    }

    let error = MessageStore::open(&paths).err().unwrap();

    assert!(error.to_string().contains("not an eh archive"), "{error:#}");
    let database = Database::open(paths.archive_db()).unwrap();
    assert_eq!(
        database
            .begin_read()
            .unwrap()
            .list_tables()
            .unwrap()
            .count(),
        0
    );
}

#[tokio::test]
async fn closing_while_a_read_still_holds_the_database_succeeds_and_the_lock_follows() {
    let root = tempfile::tempdir().unwrap();
    let paths = DataPaths::new(root.path().join("data"));
    let store = MessageStore::open(&paths).unwrap();
    let reader = Arc::clone(&store.inner.database);

    store.close().await.unwrap();
    let while_held = MessageStore::open(&paths).err();
    drop(reader);
    let after_release = MessageStore::open(&paths);

    assert!(while_held.is_some());
    assert!(after_release.is_ok());
}

#[test]
fn a_file_that_is_not_a_redb_database_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let paths = DataPaths::new(root.path());
    paths.ensure_base().unwrap();
    std::fs::write(
        paths.archive_db(),
        b"definitely not a database, but long enough to look like one at first glance......",
    )
    .unwrap();

    let error = MessageStore::open(&paths).err().unwrap();

    assert!(
        error.to_string().contains("Failed to open the archive"),
        "{error:#}"
    );
}

#[cfg(unix)]
#[test]
fn a_new_archive_is_owner_only_and_a_wide_one_is_tightened() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().unwrap();
    let paths = DataPaths::new(root.path().join("data"));
    drop(MessageStore::open(&paths).unwrap());
    let fresh = std::fs::metadata(paths.archive_db())
        .unwrap()
        .permissions()
        .mode();
    std::fs::set_permissions(paths.archive_db(), std::fs::Permissions::from_mode(0o644)).unwrap();

    drop(MessageStore::open(&paths).unwrap());

    let tightened = std::fs::metadata(paths.archive_db())
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(fresh & 0o777, 0o600);
    assert_eq!(tightened & 0o777, 0o600);
}

#[tokio::test]
#[ignore = "measurement: run on demand with --ignored --nocapture"]
async fn measure_sequential_store_throughput() {
    const COUNT: i64 = 5_000;
    let root = tempfile::tempdir().unwrap();
    let store = bound_store(&root).await;
    let started = std::time::Instant::now();

    for sequence in 1..=COUNT {
        store_one(&store, &message("0", sequence, "measurement body"), true)
            .await
            .unwrap();
    }

    let elapsed = started.elapsed();
    eprintln!(
        "stored {COUNT} messages sequentially in {elapsed:?}: {:.0} events/s",
        COUNT as f64 / elapsed.as_secs_f64()
    );
}

use super::*;
use crate::read::test_support::captured;
use shared_eventhub_core::connection::EventHubIdentity;
use shared_eventhub_core::engine::{Checkpoint, MessageSink};
use shared_eventhub_persistence::store::test_support::{failing_store, store_with_dead_writer};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tempfile::TempDir;

const REFUSAL_TAIL: &str =
    "It reopens once the running session, export, or search ends; then Connect again.";

async fn store_one(store: &MessageStore, sequence: i64) -> anyhow::Result<()> {
    let message = captured("0", sequence, "body");
    let checkpoint = Checkpoint {
        partition_id: message.key.partition_id.clone(),
        sequence_number: sequence,
        offset: message.offset.clone(),
        enqueued_time: message.enqueued_time,
        updated_at: message.processed_at,
    };
    store.store(&message, &checkpoint, true).await.map(drop)
}

async fn bind_and_store_one(store: &MessageStore) -> anyhow::Result<()> {
    store
        .bind(&EventHubIdentity {
            fully_qualified_namespace: "orders.servicebus.windows.net".to_owned(),
            event_hub_name: "orders".to_owned(),
            created_on: None,
        })
        .await?;
    store_one(store, 2).await
}

/// A slot on `folder` holding an archive whose first batch failed.
async fn slot_after_a_failed_batch(folder: &DataPaths) -> StoreSlot {
    let (failed, failing) = failing_store().await.unwrap();
    failing.store(true, Ordering::SeqCst);
    store_one(&failed, 1).await.unwrap_err();
    let slot = StoreSlot::new(folder.clone());
    slot.seat(failed).await;
    slot
}

struct Folders {
    _root: TempDir,
    a: DataPaths,
    b: DataPaths,
}

fn folders() -> Folders {
    let root = tempfile::tempdir().unwrap();
    let a = DataPaths::new(root.path().join("a"));
    let b = DataPaths::new(root.path().join("b"));
    Folders { _root: root, a, b }
}

fn is_unopened(slot: &StoreSlot) -> bool {
    matches!(*slot.state.try_lock().unwrap(), SlotState::Unopened)
}

#[tokio::test]
async fn two_concurrent_acquires_of_one_folder_share_one_open() {
    let f = folders();
    let slot = Arc::new(StoreSlot::new(f.a.clone()));

    let (first, second) = tokio::join!(slot.acquire(&f.a), slot.acquire(&f.a));

    assert!(first.is_ok(), "{:?}", first.err());
    assert!(second.is_ok(), "{:?}", second.err());
}

#[tokio::test]
async fn release_stale_after_a_folder_change_closes_the_old_archive() {
    let f = folders();
    let slot = StoreSlot::new(f.a.clone());
    drop(slot.acquire(&f.a).await.unwrap());

    slot.set_folder(f.b.clone());
    slot.release_stale().await;

    assert!(MessageStore::open(&f.a).is_ok());
    assert!(is_unopened(&slot));
    assert!(slot.acquire(&f.b).await.is_ok());
}

#[tokio::test]
async fn acquire_after_a_folder_change_swaps_to_the_new_folder() {
    let f = folders();
    let slot = StoreSlot::new(f.a.clone());
    drop(slot.acquire(&f.a).await.unwrap());

    slot.set_folder(f.b.clone());
    let opened = slot.acquire(&f.b).await;

    assert!(opened.is_ok(), "{:?}", opened.err());
    assert!(MessageStore::open(&f.a).is_ok());
}

#[tokio::test]
async fn a_swap_with_a_live_clone_still_opens_the_new_folder() {
    let f = folders();
    let slot = StoreSlot::new(f.a.clone());
    let kept = slot.acquire(&f.a).await.unwrap();

    slot.set_folder(f.b.clone());
    let opened = slot.acquire(&f.b).await;

    assert!(opened.is_ok(), "{:?}", opened.err());
    assert!(
        MessageStore::open(&f.a).is_err(),
        "the live clone still holds a"
    );
    drop(kept);
}

#[tokio::test]
async fn a_request_for_a_previous_folder_is_refused_and_changes_nothing() {
    let f = folders();
    let slot = StoreSlot::new(f.a.clone());
    slot.set_folder(f.b.clone());

    let error = slot.acquire(&f.a).await.err().unwrap();

    assert!(
        error.to_string().contains("The data folder changed"),
        "{error:#}"
    );
    assert!(is_unopened(&slot));
    assert!(!f.a.base().exists());
}

#[tokio::test]
async fn an_open_failure_leaves_the_slot_unopened_with_the_store_text() {
    let f = folders();
    std::fs::create_dir_all(f.a.base()).unwrap();
    std::fs::write(f.a.base().join("notes.txt"), "someone else's file").unwrap();
    let slot = StoreSlot::new(f.a.clone());

    let error = slot.acquire(&f.a).await.err().unwrap();

    assert!(
        format!("{error:#}").contains("is not an eh data folder"),
        "{error:#}"
    );
    assert!(is_unopened(&slot));
}

#[tokio::test]
async fn acquire_after_close_for_quit_is_refused() {
    let f = folders();
    let slot = StoreSlot::new(f.a.clone());
    drop(slot.acquire(&f.a).await.unwrap());

    slot.close_for_quit().await;
    let error = slot.acquire(&f.a).await.err().unwrap();

    assert_eq!(error.to_string(), "eh-gui is quitting");
    assert!(MessageStore::open(&f.a).is_ok());
}

#[tokio::test]
async fn close_for_quit_with_a_live_clone_still_closes_the_slot() {
    let f = folders();
    let slot = StoreSlot::new(f.a.clone());
    let kept = slot.acquire(&f.a).await.unwrap();

    slot.close_for_quit().await;

    assert!(matches!(*slot.state.try_lock().unwrap(), SlotState::Closed));
    assert!(
        MessageStore::open(&f.a).is_err(),
        "the live clone still holds a"
    );
    drop(kept);
    assert!(MessageStore::open(&f.a).is_ok());
}

#[tokio::test]
async fn close_for_quit_on_an_unopened_slot_closes_it() {
    let f = folders();
    let slot = StoreSlot::new(f.a.clone());

    slot.close_for_quit().await;

    assert!(matches!(*slot.state.try_lock().unwrap(), SlotState::Closed));
}

#[tokio::test]
async fn release_stale_after_close_for_quit_does_nothing() {
    let f = folders();
    let slot = StoreSlot::new(f.a.clone());
    slot.close_for_quit().await;
    slot.set_folder(f.b.clone());

    slot.release_stale().await;

    assert!(matches!(*slot.state.try_lock().unwrap(), SlotState::Closed));
}

#[tokio::test]
async fn a_request_queued_before_a_folder_change_is_refused() {
    let f = folders();
    let slot = Arc::new(StoreSlot::new(f.a.clone()));
    let held = slot.state.lock().await;
    let queued = tokio::spawn({
        let slot = Arc::clone(&slot);
        let a = f.a.clone();
        async move { slot.acquire(&a).await.map(drop) }
    });
    tokio::task::yield_now().await;

    slot.set_folder(f.b.clone());
    drop(held);
    let result = queued.await.unwrap();

    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("The data folder changed")
    );
    assert!(!f.a.base().exists());
}

#[tokio::test]
async fn a_session_after_a_failed_batch_gets_an_archive_that_takes_writes() {
    let f = folders();
    let slot = slot_after_a_failed_batch(&f.a).await;

    let store = slot.acquire_for_session(&f.a).await.unwrap();

    assert_eq!(store.write_failure(), None);
    bind_and_store_one(&store).await.unwrap();
}

#[tokio::test]
async fn a_session_after_a_writer_panic_gets_an_archive_that_takes_writes() {
    let f = folders();
    let dead = store_with_dead_writer(&f.a).unwrap();
    store_one(&dead, 1).await.unwrap_err();
    let slot = StoreSlot::new(f.a.clone());
    slot.seat(dead).await;

    let store = slot.acquire_for_session(&f.a).await.unwrap();

    assert_eq!(store.write_failure(), None);
    bind_and_store_one(&store).await.unwrap();
}

#[tokio::test]
async fn a_session_is_refused_while_another_handle_to_the_failed_archive_is_alive() {
    let f = folders();
    let slot = slot_after_a_failed_batch(&f.a).await;
    let kept = slot.acquire(&f.a).await.unwrap();

    let error = slot.acquire_for_session(&f.a).await.err().unwrap();
    let still_seated = slot.acquire(&f.a).await.unwrap();

    let text = error.to_string();
    assert!(
        text.starts_with("Writing to the archive failed earlier ("),
        "{text}"
    );
    assert!(text.contains("simulated disk failure"), "{text}");
    assert!(text.ends_with(REFUSAL_TAIL), "{text}");
    assert!(still_seated.write_failure().is_some());
    drop(kept);
}

#[tokio::test]
async fn a_refused_session_gets_a_working_archive_once_the_other_handle_drops() {
    let f = folders();
    let slot = slot_after_a_failed_batch(&f.a).await;
    let kept = slot.acquire(&f.a).await.unwrap();
    slot.acquire_for_session(&f.a).await.err().unwrap();
    drop(kept);

    let store = slot.acquire_for_session(&f.a).await.unwrap();

    bind_and_store_one(&store).await.unwrap();
}

#[tokio::test]
async fn a_reader_still_gets_the_archive_whose_writer_failed() {
    let f = folders();
    let slot = slot_after_a_failed_batch(&f.a).await;

    let store = slot.acquire(&f.a).await.unwrap();

    assert!(
        store
            .write_failure()
            .unwrap()
            .contains("simulated disk failure")
    );
    assert!(!f.a.base().exists());
}

#[tokio::test]
async fn a_session_on_a_healthy_archive_shares_the_open_handle() {
    let f = folders();
    let slot = StoreSlot::new(f.a.clone());
    let reader = slot.acquire(&f.a).await.unwrap();

    let store = slot.acquire_for_session(&f.a).await.unwrap();

    bind_and_store_one(&store).await.unwrap();
    assert!(!reader.is_last_handle());
}

#[tokio::test]
async fn a_session_after_close_for_quit_is_refused_and_the_slot_stays_closed() {
    let f = folders();
    let slot = StoreSlot::new(f.a.clone());
    slot.close_for_quit().await;

    let error = slot.acquire_for_session(&f.a).await.err().unwrap();

    assert_eq!(error.to_string(), "eh-gui is quitting");
    assert!(matches!(*slot.state.try_lock().unwrap(), SlotState::Closed));
}

#[tokio::test]
async fn a_session_for_a_previous_folder_is_refused() {
    let f = folders();
    let slot = StoreSlot::new(f.a.clone());
    slot.set_folder(f.b.clone());

    let error = slot.acquire_for_session(&f.a).await.err().unwrap();

    assert!(
        error.to_string().contains("The data folder changed"),
        "{error:#}"
    );
    assert!(is_unopened(&slot));
}

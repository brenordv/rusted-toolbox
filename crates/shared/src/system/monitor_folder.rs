//! Monitors a folder for newly created files and allows handling of such events via a callback.
//!
//! # Arguments
//! - `folder_to_watch`: A string slice specifying the path to the folder to monitor.
//! - `on_created_handler`: An optional asynchronous closure or function to handle file creation events.
//!    The handler receives the event and the folder being watched as parameters and executes custom logic.
//!
//! # Errors
//! - Returns an error if the specified folder does not exist or cannot be watched.
//! - Errors that occur during the invocation of the passed `on_created_handler` are propagated.
use anyhow::Result;
use notify::{Event, EventKind, RecursiveMode, Watcher};
use crate::system::ensure_directory_exists::EnsureDirectoryExists;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{mpsc};
use std::time::Duration;
use notify::event::{AccessKind, CreateKind, ModifyKind, RemoveKind};
use serde::{Deserialize, Serialize};
use tracing::debug;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum EventType {
    /// The catch-all event kind, for unsupported/unknown events.
    /// This variant should be used as the "else" case when mapping native kernel bitmasks or
    /// bitmaps, such that if the mask is ever extended with new event types the backend will
    /// not gain bugs due to not matching new unknown event types.
    /// This variant is also the default variant used when Notify is in "imprecise" mode.
    Any,
    /// The catch-all case, to be used when the specific kind of event is unknown.
    CreatedAny,
    /// An event which results in the creation of a file.
    CreatedFile,
    /// An event which results in the creation of a folder.
    CreatedFolder,
    /// An event which specific kind is known but cannot be represented otherwise.
    CreatedOther,
    /// The catch-all case, to be used when the specific kind of event is unknown.
    AccessAny,
    /// An event emitted when the file is read.
    AccessRead,
    /// An event emitted when the file, or a handle to the file, is opened.
    AccessOpen,
    /// An event emitted when the file, or a handle to the file, is closed.
    AccessClose,
    /// An event which specific kind is known but cannot be represented otherwise.
    AccessOther,
    /// The catch-all case, to be used when the specific kind of event is unknown.
    ModifyAny,
    /// An event emitted when the data content of a file is changed.
    ModifyData,
    /// An event emitted when the metadata of a file or folder is changed.
    ModifyMetadata,
    /// An event emitted when the name of a file or folder is changed.
    ModifyName,
    /// An event which specific kind is known but cannot be represented otherwise.
    ModifyOther,
    /// The catch-all case, to be used when the specific kind of event is unknown.
    RemoveAny,
    /// An event emitted when a file is removed.
    RemoveFile,
    /// An event emitted when a folder is removed.
    RemoveFolder,
    /// An event which specific kind is known but cannot be represented otherwise.
    RemoveOther,
    /// An event not fitting in any of the above four categories.
    /// This may be used for meta-events about the watch itself.
    Other
}


/// Convenience wrapper for cases when we only care about the created events.
pub async fn monitor_folder_for_on_created_only<Fut, F>(
    folder_to_watch: &str,
    on_created_handler: F,
) -> Result<()>
where
    F: Fn(Event, EventType, PathBuf) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    let path = PathBuf::from(folder_to_watch);

    path.ensure_directory_exists()?;

    monitor_folder(
        folder_to_watch,
        None,
        None,
        Some(on_created_handler),
        None,
        None,
        None,
    )
    .await?;

    Ok(())
}

pub async fn monitor_folder<Fut, F>(
    folder_to_watch: &str,
    on_any_handler: Option<F>,
    on_access_handler: Option<F>,
    on_created_handler: Option<F>,
    on_modify_handler: Option<F>,
    on_remove_handler: Option<F>,
    on_other_handler: Option<F>,
) -> Result<()>
where
    F: Fn(Event, EventType, PathBuf) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    let (tx, rx) = mpsc::channel::<notify::Result<Event>>();

    let mut watcher = notify::recommended_watcher(tx)?;

    let folder = PathBuf::from(folder_to_watch);

    watcher.watch(Path::new(folder_to_watch), RecursiveMode::Recursive)?;

    for res in rx {
        match res {
            Ok(event) => match event.kind {
                EventKind::Any => {
                    if let Some(handler) = on_any_handler.as_ref() {
                        handler(event, EventType::Any, folder.clone()).await?;
                    }
                }
                EventKind::Access(kind) => {
                    let event_type = match kind {
                        AccessKind::Any => EventType::AccessAny,
                        AccessKind::Read => EventType::AccessRead,
                        AccessKind::Open(_) => EventType::AccessOpen,
                        AccessKind::Close(_) => EventType::AccessClose,
                        AccessKind::Other => EventType::AccessOther,
                    };

                    if let Some(handler) = on_access_handler.as_ref() {
                        handler(event, event_type, folder.clone()).await?;
                    }
                }
                EventKind::Create(kind) => {
                    let event_type = match kind {
                        CreateKind::Any => EventType::CreatedAny,
                        CreateKind::File => EventType::CreatedFile,
                        CreateKind::Folder => EventType::CreatedFolder,
                        CreateKind::Other => EventType::CreatedOther,
                    };

                    if let Some(handler) = on_created_handler.as_ref() {
                        handler(event, event_type, folder.clone()).await?;
                    }
                }
                EventKind::Modify(kind) => {
                    let event_type = match kind {
                        ModifyKind::Any => EventType::ModifyAny,
                        ModifyKind::Data(_) => EventType::ModifyData,
                        ModifyKind::Metadata(_) => EventType::ModifyMetadata,
                        ModifyKind::Name(_) => EventType::ModifyName,
                        ModifyKind::Other => EventType::ModifyOther,
                    };

                    if let Some(handler) = on_modify_handler.as_ref() {
                        handler(event, event_type, folder.clone()).await?;
                    }
                }
                EventKind::Remove(kind) => {
                    let event_type = match kind {
                        RemoveKind::Any => EventType::RemoveAny,
                        RemoveKind::File => EventType::RemoveFile,
                        RemoveKind::Folder => EventType::RemoveFolder,
                        RemoveKind::Other => EventType::RemoveOther,
                    };

                    if let Some(handler) = on_remove_handler.as_ref() {
                        handler(event, event_type, folder.clone()).await?;
                    }
                }
                EventKind::Other => {
                    if let Some(handler) = on_other_handler.as_ref() {
                        handler(event, EventType::Other, folder.clone()).await?;
                    }
                }
            },
            Err(e) => anyhow::bail!("watch error: {:?}", e),
        }
    }

    Ok(())
}


/// Dummy function that handles the "file created" event triggered within a watched folder.
/// Doesn't do anything with the files, just logs the entries received.
pub async fn dummy_handle_event_created(event: Event, event_type: EventType, _: PathBuf) -> Result<()> {
    let created_entries = &event.paths;
    debug!("[{:?}] On Created Event (count: {}): [{:?}]", event_type, created_entries.len(), event);

    for entry in created_entries {
        debug!("Processing file: {:?}", entry);
        tokio::time::sleep(Duration::from_secs(2)).await;

    }

    Ok(())
}
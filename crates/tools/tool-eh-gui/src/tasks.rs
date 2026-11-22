//! Background work shared by the UI: the single archive handle.

use anyhow::Context;
use shared_eventhub_persistence::paths::DataPaths;
use shared_eventhub_persistence::store::MessageStore;
use std::sync::{Mutex, PoisonError};
use tracing::{debug, info, warn};

/// The app's one archive handle. redb refuses a second open of a file in one process, and
/// [`MessageStore::close`] consumes the handle, so every user of the archive borrows a clone
/// from here.
///
/// The current data folder is kept apart from the handle: `set_folder` only records the new
/// folder (no I/O, so the UI thread may call it), and the handle moves to that folder on the
/// next [`acquire`](Self::acquire) or [`release_stale`](Self::release_stale). Every caller names
/// the folder its request was made for, and a request for any folder but the current one is
/// refused, so a stale request can neither reopen an old folder nor read the new one.
///
/// `acquire`, `acquire_for_session`, and `close_for_quit` are not cancel-safe: a dropped
/// `acquire` still finishes its open in the background and keeps the file locked. Run them to
/// completion in their own task.
pub struct StoreSlot {
    current: Mutex<DataPaths>,
    state: tokio::sync::Mutex<SlotState>,
}

enum SlotState {
    Unopened,
    Open {
        base: DataPaths,
        store: MessageStore,
    },
    /// The app is quitting; nothing opens again.
    Closed,
}

impl StoreSlot {
    pub fn new(folder: DataPaths) -> Self {
        Self {
            current: Mutex::new(folder),
            state: tokio::sync::Mutex::new(SlotState::Unopened),
        }
    }

    /// The data folder requests are checked against.
    pub fn current(&self) -> DataPaths {
        // A panic while the lock was held can only have happened around a plain assignment,
        // so the value inside is whole either way.
        self.current
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Makes `folder` the current data folder. The open handle, if any, belongs to the old
    /// folder from now on and is closed by the next `acquire` or `release_stale`.
    pub fn set_folder(&self, folder: DataPaths) {
        *self.current.lock().unwrap_or_else(PoisonError::into_inner) = folder;
    }

    /// Returns a handle to the archive of `folder`, opening it on first use.
    ///
    /// # Errors
    /// Fails when `folder` is no longer the current data folder, when the app is quitting, and
    /// when the archive cannot be opened (another process holds it, or the folder is not an eh
    /// data folder); the open error's own text is user-facing.
    pub async fn acquire(&self, folder: &DataPaths) -> anyhow::Result<MessageStore> {
        let mut state = self.state.lock().await;
        let current = self.checked_current(folder)?;
        open_or_share(&mut state, current).await
    }

    /// Returns a handle to the archive of `folder` that takes writes. An open archive whose
    /// writer failed is closed and opened again when the slot holds its only handle.
    ///
    /// # Errors
    /// Fails as [`acquire`](Self::acquire) does, and when the open archive's writer failed
    /// while another handle to it is still alive: nothing is closed then, and the text says
    /// when to try again.
    pub async fn acquire_for_session(&self, folder: &DataPaths) -> anyhow::Result<MessageStore> {
        let mut state = self.state.lock().await;
        let current = self.checked_current(folder)?;
        let failure = match &*state {
            SlotState::Open { base, store } if *base == current => store
                .write_failure()
                .map(|cause| (cause, store.is_last_handle())),
            SlotState::Open { .. } | SlotState::Unopened | SlotState::Closed => None,
        };
        match failure {
            Some((cause, true)) => close_failed(&mut state, &cause).await,
            Some((cause, false)) => anyhow::bail!(
                "Writing to the archive failed earlier ({cause}). It reopens once the running \
                 session, export, or search ends; then Connect again."
            ),
            None => {}
        }
        open_or_share(&mut state, current).await
    }

    /// The current data folder, when `folder` is it.
    fn checked_current(&self, folder: &DataPaths) -> anyhow::Result<DataPaths> {
        let current = self.current();
        if *folder != current {
            debug!(
                requested = %folder.base().display(),
                current = %current.base().display(),
                "refused an archive request for a previous data folder"
            );
            anyhow::bail!(
                "The data folder changed; this request was for '{}'",
                folder.base().display()
            );
        }
        Ok(current)
    }

    /// Makes `store` the open handle of the current data folder.
    #[cfg(test)]
    async fn seat(&self, store: MessageStore) {
        *self.state.lock().await = SlotState::Open {
            base: self.current(),
            store,
        };
    }

    /// Closes the handle when it belongs to a data folder that is no longer current.
    pub async fn release_stale(&self) {
        let mut state = self.state.lock().await;
        let current = self.current();
        let stale = match &*state {
            SlotState::Open { base, .. } => *base != current,
            SlotState::Unopened | SlotState::Closed => false,
        };
        if stale {
            close_stale(&mut state, &current).await;
        }
    }

    /// Closes the handle for good. A close error is logged; every stored batch is already
    /// durable, so nothing depends on it.
    pub async fn close_for_quit(&self) {
        let mut state = self.state.lock().await;
        match std::mem::replace(&mut *state, SlotState::Closed) {
            SlotState::Open { base, store } => {
                if let Err(error) = store.close().await {
                    warn!(
                        folder = %base.base().display(),
                        error = %format!("{error:#}"),
                        "could not close the archive at quit"
                    );
                }
            }
            SlotState::Unopened | SlotState::Closed => {}
        }
    }
}

/// A handle to the archive of `current`: the open one, or a new open after closing a handle
/// that belongs to another folder.
async fn open_or_share(state: &mut SlotState, current: DataPaths) -> anyhow::Result<MessageStore> {
    match &*state {
        SlotState::Closed => anyhow::bail!("eh-gui is quitting"),
        SlotState::Open { base, store } if *base == current => return Ok(store.clone()),
        SlotState::Open { .. } => close_stale(state, &current).await,
        SlotState::Unopened => {}
    }

    let opening = current.clone();
    let store = tokio::task::spawn_blocking(move || MessageStore::open(&opening))
        .await
        .context("Opening the archive stopped unexpectedly")??;
    *state = SlotState::Open {
        base: current,
        store: store.clone(),
    };
    Ok(store)
}

/// Takes the open handle, whose writer failed with `cause`, out of `state` (leaving it
/// `Unopened`) and closes it, so the next open starts a new writer.
async fn close_failed(state: &mut SlotState, cause: &str) {
    let SlotState::Open { base, store } = std::mem::replace(state, SlotState::Unopened) else {
        return;
    };
    info!(
        folder = %base.base().display(),
        cause,
        "reopening the archive after a write failure"
    );
    if let Err(error) = store.close().await {
        // Best-effort: a writer that died always reports its cause here, with the file
        // lock released all the same, so the reopen goes on.
        warn!(
            folder = %base.base().display(),
            error = %format!("{error:#}"),
            "closing the archive that failed reported an error; reopening it anyway"
        );
    }
}

/// Takes the open handle out of `state` (leaving it `Unopened`) and closes it. A close that
/// fails (another clone is still alive) is logged and the swap goes on: the old folder's lock
/// is released when that last clone drops.
async fn close_stale(state: &mut SlotState, current: &DataPaths) {
    let SlotState::Open { base, store } = std::mem::replace(state, SlotState::Unopened) else {
        return;
    };
    info!(
        old = %base.base().display(),
        new = %current.base().display(),
        "closing the previous data folder's archive"
    );
    if let Err(error) = store.close().await {
        warn!(
            old = %base.base().display(),
            new = %current.base().display(),
            error = %format!("{error:#}"),
            "could not close the previous data folder's archive; its lock is released when the \
             last handle drops"
        );
    }
}

#[cfg(test)]
mod tests;

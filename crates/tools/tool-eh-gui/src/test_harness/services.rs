//! A `Services` that records each request and does nothing else, so a test answers it by
//! delivering what the real task would post.

use crate::archive::PageRequest;
use crate::archive::bulk::BulkRequest;
use crate::read::ExportOneRequest;
use crate::read::search::RebuildRequest;
use crate::send::SendRequest;
use crate::services::{ConnectRequest, CreateRequest, SaveRequest, Services};
use shared_eventhub_persistence::paths::DataPaths;
use shared_eventhub_persistence::workspace::Workspace;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// One call the app made, with its whole request. Every secret a request carries is a
/// `SecretString`, whose `Debug` prints `<redacted>`.
#[derive(Debug)]
pub enum ServiceCall {
    PrepareSession {
        attempt: u64,
        request: ConnectRequest,
    },
    SaveConfig(SaveRequest),
    SwitchDataFolder(DataPaths),
    ListProfiles {
        request: u64,
        workspace: Workspace,
    },
    LoadProfile {
        request: u64,
        dir: PathBuf,
    },
    CreateProfile(CreateRequest),
    ReopenWindow,
    Quit,
    RebuildSearch(RebuildRequest),
    ExportOne(ExportOneRequest),
    SearchArchive(PageRequest),
    BulkExport(BulkRequest),
    StartSend(SendRequest),
}

/// The calls recorded so far, shared between the services and the test.
#[derive(Clone, Debug, Default)]
pub struct CallLog(Arc<Mutex<Vec<ServiceCall>>>);

impl CallLog {
    fn lock(&self) -> MutexGuard<'_, Vec<ServiceCall>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn push(&self, call: ServiceCall) {
        self.lock().push(call);
    }

    /// Removes and returns every call recorded so far.
    pub fn take(&self) -> Vec<ServiceCall> {
        std::mem::take(&mut *self.lock())
    }

    /// Removes and returns the calls `wanted` picks, keeping the rest in order.
    pub fn take_where(&self, wanted: impl Fn(&ServiceCall) -> bool) -> Vec<ServiceCall> {
        let mut calls = self.lock();
        let (taken, kept) = std::mem::take(&mut *calls).into_iter().partition(wanted);
        *calls = kept;
        taken
    }

    /// How many recorded calls `counted` picks.
    pub fn count(&self, counted: impl Fn(&ServiceCall) -> bool) -> usize {
        self.lock().iter().filter(|call| counted(call)).count()
    }

    /// How many archive pages were asked for.
    pub fn searches(&self) -> usize {
        self.count(|call| matches!(call, ServiceCall::SearchArchive(_)))
    }

    /// How many sends were started.
    pub fn sends(&self) -> usize {
        self.count(|call| matches!(call, ServiceCall::StartSend(_)))
    }
}

/// Records every call into its [`CallLog`].
pub struct RecordingServices(CallLog);

impl RecordingServices {
    /// Services that record into `log`.
    pub fn new(log: CallLog) -> Self {
        Self(log)
    }
}

impl Services for RecordingServices {
    fn prepare_session(&self, attempt: u64, request: ConnectRequest) {
        self.0
            .push(ServiceCall::PrepareSession { attempt, request });
    }

    fn save_config(&self, request: SaveRequest) {
        self.0.push(ServiceCall::SaveConfig(request));
    }

    fn switch_data_folder(&self, paths: DataPaths) {
        self.0.push(ServiceCall::SwitchDataFolder(paths));
    }

    fn list_profiles(&self, request: u64, workspace: Workspace) {
        self.0
            .push(ServiceCall::ListProfiles { request, workspace });
    }

    fn load_profile(&self, request: u64, dir: PathBuf) {
        self.0.push(ServiceCall::LoadProfile { request, dir });
    }

    fn create_profile(&self, request: CreateRequest) {
        self.0.push(ServiceCall::CreateProfile(request));
    }

    fn reopen_window(&self) {
        self.0.push(ServiceCall::ReopenWindow);
    }

    fn quit(&self) {
        self.0.push(ServiceCall::Quit);
    }

    fn rebuild_search(&self, request: RebuildRequest) {
        self.0.push(ServiceCall::RebuildSearch(request));
    }

    fn export_one(&self, request: ExportOneRequest) {
        self.0.push(ServiceCall::ExportOne(request));
    }

    fn search_archive(&self, request: PageRequest) {
        self.0.push(ServiceCall::SearchArchive(request));
    }

    fn bulk_export(&self, request: BulkRequest) {
        self.0.push(ServiceCall::BulkExport(request));
    }

    fn start_send(&self, request: SendRequest) {
        self.0.push(ServiceCall::StartSend(request));
    }
}

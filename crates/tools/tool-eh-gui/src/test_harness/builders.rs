//! App states the journeys, sweeps, sequences, and view tests start from.

use super::services::{CallLog, RecordingServices};
use crate::archive::results::Results;
use crate::archive::test_support::{cursor, messages, page};
use crate::archive::{ArchiveEffect, ArchiveInput};
use crate::bridge::UiDelivery;
use crate::bridge::test_support::detached_session;
use crate::form::{AuthChoice, ConnectForm};
use crate::read::ReadInput;
use crate::read::test_support::captured;
use crate::send::compose::PacingChoice;
use crate::send::run::Names;
use crate::send::{SendEffect, SendInput};
use crate::state::{AppState, Effect, Input, Section, SheetPurpose};
use common_cli::app_logger::FileLogging;
use eh_core::engine::{EngineEvent, RunEnd, RunSummary};
use eh_core::filters::{CaptureFilters, TextFilter};
use eh_core::stats::StatsTracker;
use eh_persistence::config::{
    AuthMethodConfig, CredentialKindConfig, StoredSecret, TargetConfig, ToolConfig,
};
use eh_persistence::export::ExportFormat;
use eh_persistence::workspace::{
    ConfigHome, Profile, ProfileEntry, ProfileName, ProfileState, Workspace,
};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// A typed connection string for the `orders` hub; its key is not a secret.
pub const SEND_CONNECTION_STRING: &str = "Endpoint=sb://orders.servicebus.windows.net/;\
    SharedAccessKeyName=send;SharedAccessKey=c2VjcmV0;EntityPath=orders";
/// A typed listen connection string for the `orders` hub; its key is not a secret.
pub const ORDERS: &str = "Endpoint=sb://orders.servicebus.windows.net/;\
    SharedAccessKeyName=listen;SharedAccessKey=bGlzdGVu;EntityPath=orders";

/// The summary of a capture run that was stopped and found nothing.
pub fn stopped() -> RunSummary {
    RunSummary {
        stats: StatsTracker::new(chrono::Utc::now()).snapshot(),
        partitions: BTreeMap::new(),
        end: RunEnd::Stopped,
    }
}

/// A first-launch state: disconnected, no services.
pub fn state() -> AppState {
    AppState::new(None, ConnectForm::default(), None, FileLogging::Off)
}

/// Gives `state` recording services and returns their log.
pub fn record(state: &mut AppState) -> CallLog {
    let log = CallLog::default();
    state.services = Some(Box::new(RecordingServices::new(log.clone())));
    log
}

/// The connection sheet open on `auth`.
pub fn open_sheet(auth: AuthChoice) -> AppState {
    let mut s = state();
    s.form.set_auth(auth);
    s.sheet = Some(SheetPurpose::Connect);
    s
}

/// A state connected to a session over four partitions; the runtime must outlive it.
pub fn connected(rt: &tokio::runtime::Runtime) -> AppState {
    let mut s = state();
    s.form
        .connection_string
        .edit_mut()
        .replace_text("Endpoint=sb://orders.servicebus.windows.net/;SharedAccessKey=k");
    let out = s.apply(Input::ConnectPressed);
    let [Effect::PrepareSession { attempt, .. }] = out.effects.as_slice() else {
        panic!("expected PrepareSession, got {:?}", out.effects);
    };
    let attempt = *attempt;
    let (session, _) = detached_session(attempt, rt.handle());
    s.apply(Input::SessionReady {
        attempt,
        result: Ok(session),
    });
    s.apply(Input::Delivery(UiDelivery {
        at: chrono::Utc::now(),
        session: attempt,
        events: vec![EngineEvent::Started {
            partitions: ["0", "1", "2", "3"].map(str::to_owned).to_vec(),
        }],
        stats: None,
    }));
    s
}

/// Delivers `count` messages on partition 0 from sequence `first` and flushes them onto the
/// list.
pub fn with_rows(s: &mut AppState, first: usize, count: usize) {
    let session = s.connection.session().map(|session| session.id());
    let Some(session) = session else {
        panic!("with_rows needs a connected state");
    };
    let events = (first..first + count)
        .map(|sequence| EngineEvent::Message(Box::new(captured("0", sequence as i64, "body"))))
        .collect();
    s.apply(Input::Delivery(UiDelivery {
        at: chrono::Utc::now(),
        session,
        events,
        stats: None,
    }));
    s.apply(Input::Read(ReadInput::ListFlush));
}

/// The Export section with a search whose first page showed `count` rows, more to come when
/// `more`; the log sees the calls made after that.
pub fn export_results(count: usize, more: bool) -> (AppState, CallLog) {
    let mut s = state();
    let log = record(&mut s);
    s.section = Section::Export;
    let out = s.apply(Input::Archive(ArchiveInput::SearchPressed));
    let [Effect::Archive(ArchiveEffect::Search(request))] = out.effects.as_slice() else {
        panic!("expected a page request, got {:?}", out.effects);
    };
    let generation = request.generation;
    s.apply(Input::Archive(ArchiveInput::PageLoaded {
        generation,
        elapsed_ms: 1,
        result: Ok(page(messages(count), more.then(cursor))),
    }));
    (s, log)
}

/// The Export section showing `results`.
pub fn export_state(results: Results) -> AppState {
    let mut s = state();
    s.section = Section::Export;
    s.archive.results = results;
    s
}

/// The Send section with a usable destination; the log sees each send started.
pub fn send_state() -> (AppState, CallLog) {
    let mut form = ConnectForm::default();
    form.connection_string
        .edit_mut()
        .replace_text(SEND_CONNECTION_STRING);
    let mut s = AppState::new(None, form, None, FileLogging::Off);
    let log = record(&mut s);
    s.section = Section::Send;
    (s, log)
}

/// Starts a send of 1,000 (paced one a second when `paced`) and moves it to 340 sent, four
/// seconds in; returns its run id.
pub fn send_running(s: &mut AppState, paced: bool) -> u64 {
    if paced {
        s.send.composer.pacing = PacingChoice::Paced;
    }
    s.send.composer.count.replace_text("1000");
    let out = s.apply(Input::Send(SendInput::SendPressed));
    let [Effect::Send(SendEffect::StartSend(request))] = out.effects.as_slice() else {
        panic!("expected StartSend, got {:?}", out.effects);
    };
    let run = request.run;
    let at = chrono::Utc::now();
    s.apply(Input::Send(SendInput::Started {
        run,
        target: Names {
            namespace: "orders.servicebus.windows.net".to_owned(),
            event_hub: "orders".to_owned(),
        },
        at,
    }));
    s.apply(Input::Send(SendInput::Progress { run, sent: 340 }));
    s.apply(Input::Tick(at + chrono::TimeDelta::seconds(4)));
    run
}

/// The Send section with `body` and Template on.
pub fn template_state(body: &str) -> (AppState, CallLog) {
    let (mut s, log) = send_state();
    s.send.composer.body.replace_text(body);
    s.send.composer.set_template(true);
    (s, log)
}

/// A profile that loads, with a stored connection string, its own rules and export options.
pub const ORDERS_DEV: &str = "orders-dev";
/// A profile that loads, signing in with Entra ID.
pub const BILLING: &str = "billing";
/// A profile folder whose `config.json` can't load.
pub const SCRATCH: &str = "scratch";
/// The keyword `orders-dev`'s capture rules look for.
pub const ORDERS_KEYWORD: &str = "refund";

/// A launch folder that nothing creates or writes. The view shows it, so it is a relative path
/// that reads the same on every machine.
pub fn launch_folder() -> PathBuf {
    PathBuf::from("eh-gui-test-launch")
}

/// A workspace that nothing creates, so listing it finds no profiles. The view shows its
/// profile folders, so its root is a relative path that reads the same on every machine.
pub fn test_workspace() -> Workspace {
    Workspace::at(PathBuf::from("eh-gui-test-workspace"))
}

/// `s` launched in [`launch_folder`] over [`test_workspace`].
pub fn with_profiles(mut s: AppState) -> AppState {
    s.set_launch(launch_folder(), Ok(test_workspace()));
    s
}

/// The profile folder `name` of [`test_workspace`].
///
/// # Panics
/// When `name` is not a valid profile name, or the lookup fails.
pub fn profile(name: &str) -> Profile {
    let name = ProfileName::parse(name).unwrap_or_else(|error| panic!("{error:#}"));
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap_or_else(|error| panic!("a test tokio runtime: {error}"))
        .block_on(test_workspace().profile(&name))
        .unwrap_or_else(|error| panic!("{error:#}"))
}

/// The home of profile `name`.
pub fn profile_home(name: &str) -> ConfigHome {
    ConfigHome::Profile(profile(name))
}

/// `orders-dev`'s config: a stored connection string, a keyword rule, JSON exports without
/// metadata, and the default `.eh-data` data folder.
pub fn orders_config() -> ToolConfig {
    let mut config = ToolConfig::new(TargetConfig {
        namespace: None,
        event_hub: None,
        consumer_group: "audit".to_owned(),
        auth: AuthMethodConfig::ConnectionString {
            stored: StoredSecret::LocalDb,
        },
    });
    config.capture.filters = CaptureFilters {
        time: None,
        text: Some(TextFilter::Contains(vec![ORDERS_KEYWORD.to_owned()])),
    };
    config.storage.export.format = ExportFormat::Json;
    config.storage.export.include_metadata = false;
    config
}

/// `billing`'s config: Entra ID developer tools for the `invoices` hub.
pub fn billing_config() -> ToolConfig {
    ToolConfig::new(TargetConfig {
        namespace: Some("billing.servicebus.windows.net".to_owned()),
        event_hub: Some("invoices".to_owned()),
        consumer_group: "$Default".to_owned(),
        auth: AuthMethodConfig::Credential {
            credential: CredentialKindConfig::DeveloperTools,
        },
    })
}

/// The entry of profile `name` that loads `config`.
pub fn ready(name: &str, config: ToolConfig) -> ProfileEntry {
    ProfileEntry {
        profile: profile(name),
        state: ProfileState::Ready(Box::new(config)),
    }
}

/// The entry of profile `name` that can't load.
pub fn cannot_load(name: &str) -> ProfileEntry {
    ProfileEntry {
        profile: profile(name),
        state: ProfileState::CannotLoad("missing field `target`".to_owned()),
    }
}

/// The test workspace's listing: `billing` and `orders-dev` load, `scratch` can't.
pub fn listing() -> Vec<ProfileEntry> {
    vec![
        ready(BILLING, billing_config()),
        ready(ORDERS_DEV, orders_config()),
        cannot_load(SCRATCH),
    ]
}

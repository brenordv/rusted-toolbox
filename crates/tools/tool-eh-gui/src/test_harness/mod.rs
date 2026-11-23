//! The headless test harness: the app's real view in Rune's `Runtime`, with recording services,
//! a virtual clock, and the helpers the journeys, sweeps, sequences, and view tests share. It is
//! the one place tests reach into Rune's runtime API, so a Rune bump edits this module.
//!
//! Test support only (`cfg(test)` or the dev-only `test-harness` feature): helpers panic on a
//! failed expectation, which fails the test that called them.

pub mod builders;
pub mod journeys;
pub mod services;

#[cfg(test)]
mod a11y_tests;
#[cfg(test)]
mod details_tests;
#[cfg(test)]
mod hint_tests;
#[cfg(test)]
mod journey_tests;
#[cfg(test)]
mod metadata_tests;
#[cfg(test)]
mod pane_tests;
#[cfg(test)]
mod profile_tests;
#[cfg(test)]
mod sequence_tests;
#[cfg(test)]
mod sweep_tests;
#[cfg(test)]
mod switch_tests;
#[cfg(test)]
mod toast_tests;
#[cfg(test)]
mod worst_cases;

use crate::bridge::UiDelivery;
use crate::bridge::test_support::detached_session;
use crate::read::ReadInput;
use crate::read::search::{RebuildRequest, Rebuilt};
use crate::services::apply_input;
use crate::state::{AppState, Input};
use crate::views::{view, window_key};
use chrono::{DateTime, TimeDelta, Utc};
pub use common_gui_rune::test_harness::{
    FRAME, SETTLE_FRAMES, a11y_node, a11y_nodes, command, command_mods, element, focused_key,
    has_key, has_text, label_of, press, press_key, rect, settle, text_visible,
};
use rune::accesskit::{Action, Role};
use rune::clipboard::MemoryClipboard;
use rune::{Key, KeyEvent, Mods, Runtime};
use serde_json::Value;
use services::{CallLog, ServiceCall};
use shared_eventhub_core::engine::{EngineCommand, EngineEvent};
use shared_eventhub_core::models::CapturedMessage;
use shared_eventhub_core::stats::StatsSnapshot;
use std::path::Path;
use tokio::sync::mpsc;

/// A runtime over `state` at the default window size, settled.
pub fn runtime(state: AppState) -> Runtime<AppState> {
    runtime_at(state, crate::design::tokens::WINDOW_SIZE)
}

/// A runtime over `state` at `size`, settled.
pub fn runtime_at(state: AppState, size: (f32, f32)) -> Runtime<AppState> {
    common_gui_rune::test_harness::runtime_at(state, view, size)
}

/// Whether the element keyed `key` is mounted with some of it on screen.
pub fn row_visible(rt: &Runtime<AppState>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|element| element.key.as_deref() == Some(key) && element.visible[3] > 0.0)
}

/// The text runs of the multi-line field whose value is `value`, top to bottom: each run's text
/// and the byte length of each of its characters. They come from
/// [`Runtime::accessibility_tree`], since [`Runtime::accessibility_json`] leaves runs out.
///
/// # Panics
/// When no multi-line field holds `value`.
pub fn text_runs(rt: &Runtime<AppState>, value: &str) -> Vec<(String, Vec<u8>)> {
    let tree = rt.accessibility_tree();
    let (_, field) = tree
        .nodes
        .iter()
        .find(|(_, node)| node.role() == Role::MultilineTextInput && node.value() == Some(value))
        .unwrap_or_else(|| panic!("no multi-line field holds {value:?}"));
    field
        .children()
        .iter()
        .filter_map(|id| tree.nodes.iter().find(|(node_id, _)| node_id == id))
        .filter(|(_, node)| node.role() == Role::TextRun)
        .map(|(_, run)| {
            (
                run.value().unwrap_or_default().to_owned(),
                run.character_lengths().to_vec(),
            )
        })
        .collect()
}

/// Whether `node` lists `word` under `field` (`states` or `actions`).
pub fn lists(node: &Value, field: &str, word: &str) -> bool {
    node[field]
        .as_array()
        .is_some_and(|words| words.iter().any(|w| w.as_str() == Some(word)))
}

/// The app's view in a headless runtime with recording services and a virtual clock. Inputs a
/// background task would post go through [`Harness::deliver`]; nothing waits on the wall clock.
pub struct Harness {
    pub rt: Runtime<AppState>,
    pub calls: CallLog,
    /// Frames each action runs after it; [`SETTLE_FRAMES`] unless a test needs fewer.
    pub settle_frames: usize,
    clock: DateTime<Utc>,
    tokio: tokio::runtime::Runtime,
    clipboard: MemoryClipboard,
}

impl Harness {
    /// `state` at `size`, with fresh recording services, the window's key handler, and an
    /// in-memory clipboard.
    pub fn new(mut state: AppState, size: (f32, f32)) -> Self {
        let calls = builders::record(&mut state);
        let mut rt = runtime_at(state, size);
        rt.on_key = Some(window_key);
        let clipboard = rune::clipboard::memory();
        rt.use_clipboard(&clipboard);
        Self {
            rt,
            calls,
            clipboard,
            settle_frames: SETTLE_FRAMES,
            clock: Utc::now(),
            tokio: tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap_or_else(|error| panic!("a test tokio runtime: {error}")),
        }
    }

    /// The tokio runtime detached sessions run their commands on.
    pub fn tokio(&self) -> &tokio::runtime::Runtime {
        &self.tokio
    }

    /// The virtual clock's current instant.
    pub fn now(&self) -> DateTime<Utc> {
        self.clock
    }

    /// Runs [`Harness::settle_frames`] frames.
    pub fn settle(&mut self) {
        for _ in 0..self.settle_frames {
            self.rt.frame(FRAME);
        }
    }

    /// A pointer press and release at the centre of the element keyed `key`, then a settle.
    ///
    /// # Panics
    /// When nothing is keyed `key`.
    pub fn press(&mut self, key: &str) {
        let (x, y) = self
            .rt
            .find(key)
            .unwrap_or_else(|| panic!("no element keyed {key}"));
        self.press_at(x, y);
    }

    /// A pointer press and release at (`x`, `y`), then a settle.
    pub fn press_at(&mut self, x: f32, y: f32) {
        self.rt.pointer_down(x, y);
        self.rt.pointer_up(x, y);
        self.settle();
    }

    /// A drag from the centre of the element keyed `key`, `dx` points sideways in a few frames,
    /// then a settle.
    ///
    /// # Panics
    /// When nothing is keyed `key`.
    pub fn drag(&mut self, key: &str, dx: f32) {
        const STEPS: u8 = 4;
        let (x, y) = self
            .rt
            .find(key)
            .unwrap_or_else(|| panic!("no element keyed {key}"));
        self.rt.pointer_down(x, y);
        self.rt.frame(FRAME);
        for step in 1..=STEPS {
            self.rt
                .pointer_move(x + dx * f32::from(step) / f32::from(STEPS), y);
            self.rt.frame(FRAME);
        }
        self.rt.pointer_up(x + dx, y);
        self.settle();
    }

    /// The pointer moved to the centre of the element keyed `key`, then a settle.
    ///
    /// # Panics
    /// When nothing is keyed `key`.
    pub fn hover(&mut self, key: &str) {
        let (x, y) = self
            .rt
            .find(key)
            .unwrap_or_else(|| panic!("no element keyed {key}"));
        self.rt.pointer_move(x, y);
        self.settle();
    }

    /// The pointer left the window, then a settle.
    pub fn pointer_away(&mut self) {
        self.rt.pointer_leave();
        self.settle();
    }

    /// The window resized to `size` at scale 1, then a settle.
    pub fn resize(&mut self, size: (f32, f32)) {
        self.rt.set_viewport(size, 1.0);
        self.settle();
    }

    /// Presses the element labeled `name` inside the element keyed `within`, for a control the
    /// kit builds without a key.
    ///
    /// # Panics
    /// When nothing keyed `within` holds an element labeled `name`.
    pub fn press_named(&mut self, within: &str, name: &str) {
        let inside = self.path_of(within);
        let Some(target) = self.rt.elements().into_iter().find(|element| {
            !element.exiting
                && element.label.as_deref() == Some(name)
                && element.path.starts_with(&format!("{inside}/"))
        }) else {
            panic!("nothing labeled {name} inside {within}");
        };
        let [x, y, w, h] = target.rect;
        self.press_at(x + w / 2.0, y + h / 2.0);
    }

    /// The label of the focused element inside the element keyed `within`.
    ///
    /// # Panics
    /// When nothing is keyed `within`.
    pub fn focused_label_within(&self, within: &str) -> Option<String> {
        let inside = self.path_of(within);
        self.rt
            .elements()
            .into_iter()
            .find(|element| element.focused && element.path.starts_with(&format!("{inside}/")))
            .and_then(|element| element.label)
    }

    /// The text the app last copied, from the harness's in-memory clipboard.
    pub fn clipboard_text(&self) -> Option<String> {
        self.clipboard.text().map(|text| text.to_string())
    }

    /// The element path of the element keyed `key`.
    ///
    /// # Panics
    /// When nothing is keyed `key`.
    pub fn path_of(&self, key: &str) -> String {
        self.rt
            .elements()
            .into_iter()
            .find(|element| element.key.as_deref() == Some(key) && !element.exiting)
            .map(|element| element.path)
            .unwrap_or_else(|| panic!("no element keyed {key}"))
    }

    /// What an assistive client's click does on `key`: the keyboard and accessibility path.
    ///
    /// # Panics
    /// When nothing keyed `key` takes a click.
    pub fn activate(&mut self, key: &str) {
        assert!(
            self.rt.accessibility_action(key, Action::Click, None),
            "nothing keyed {key} takes a click"
        );
        self.settle();
    }

    /// Presses the field keyed `key` (the key may sit on a wrapper around the editor), selects
    /// its text, and pastes `text` over it.
    ///
    /// # Panics
    /// When nothing is keyed `key`, or the press leaves focus outside it.
    pub fn type_into(&mut self, key: &str, text: &str) {
        self.press(key);
        let field = self
            .rt
            .elements()
            .into_iter()
            .find(|element| element.key.as_deref() == Some(key) && !element.exiting)
            .map(|element| element.path);
        let focused = self
            .rt
            .elements()
            .into_iter()
            .find(|element| element.focused)
            .map(|element| element.path);
        let inside = match (&field, &focused) {
            (Some(field), Some(focused)) => {
                focused == field || focused.starts_with(&format!("{field}/"))
            }
            (None, _) | (_, None) => false,
        };
        assert!(inside, "{key} did not take focus: {focused:?}");
        self.command(Key::Command('a'));
        self.rt.paste(text);
        self.settle();
    }

    /// Presses `key` with no modifiers.
    pub fn key(&mut self, key: Key) {
        self.rt.key(KeyEvent {
            key,
            mods: Mods::default(),
        });
        self.settle();
    }

    /// Presses `mod+<key>`.
    pub fn command(&mut self, key: Key) {
        self.rt.key(KeyEvent {
            key,
            mods: command_mods(),
        });
        self.settle();
    }

    /// Answers the open file dialog with `path`, or cancels it with `None`.
    ///
    /// # Panics
    /// When no file dialog is open.
    pub fn answer_dialog(&mut self, path: Option<&Path>) {
        assert!(self.rt.file_dialog().is_some(), "no file dialog is open");
        if let Err(error) = self.rt.resolve_file_dialog(path.map(Path::to_path_buf)) {
            panic!("answering the dialog failed: {error:?}");
        }
        self.settle();
    }

    /// What a background task posts: applied, its motion and effects run, then a settle.
    pub fn deliver(&mut self, input: Input) {
        self.rt
            .update_if(move |state, tx| apply_input(state, tx, input));
        self.settle();
    }

    /// The once-a-second tick at the virtual clock, which then moves on a second.
    pub fn tick(&mut self) {
        let now = self.clock;
        self.clock = now + TimeDelta::seconds(1);
        self.deliver(Input::Tick(now));
    }

    /// Runs the newest recorded quick-search rebuild as the real service does and delivers its
    /// result; false when none was recorded.
    pub fn answer_rebuild(&mut self) -> bool {
        let rebuilds = self
            .calls
            .take_where(|call| matches!(call, ServiceCall::RebuildSearch(_)));
        let Some(ServiceCall::RebuildSearch(request)) = rebuilds.into_iter().last() else {
            return false;
        };
        let Some(rebuilt) = rebuilt(request) else {
            return false;
        };
        self.deliver(Input::Read(ReadInput::SearchRebuilt(rebuilt)));
        true
    }

    /// Presses Connect through the state (the form must be complete), answers the prepare with
    /// a detached session, and reports `partitions` started. Returns the session id and the
    /// commands the app sends it.
    ///
    /// # Panics
    /// When Connect prepares no session (the form has issues).
    pub fn start_session(&mut self, partitions: usize) -> (u64, mpsc::Receiver<EngineCommand>) {
        self.deliver(Input::ConnectPressed);
        let prepared = self
            .calls
            .take_where(|call| matches!(call, ServiceCall::PrepareSession { .. }));
        let Some(ServiceCall::PrepareSession { attempt, .. }) = prepared.into_iter().last() else {
            panic!("Connect did not prepare a session; does the form have issues?");
        };
        let (session, commands) = detached_session(attempt, self.tokio.handle());
        self.deliver(Input::SessionReady {
            attempt,
            result: Ok(session),
        });
        let at = self.now();
        self.deliver(Input::Delivery(UiDelivery {
            at,
            session: attempt,
            events: vec![EngineEvent::Started {
                partitions: (0..partitions).map(|id| id.to_string()).collect(),
            }],
            stats: None,
        }));
        (attempt, commands)
    }

    /// Delivers `messages` to `session` with `stats`, then flushes them onto the list.
    pub fn capture(
        &mut self,
        session: u64,
        messages: Vec<CapturedMessage>,
        stats: Option<StatsSnapshot>,
    ) {
        let at = self.now();
        self.deliver(Input::Delivery(UiDelivery {
            at,
            session,
            events: messages
                .into_iter()
                .map(|message| EngineEvent::Message(Box::new(message)))
                .collect(),
            stats,
        }));
        self.deliver(Input::Read(ReadInput::ListFlush));
    }

    /// Removes and returns every call recorded so far.
    pub fn take_calls(&mut self) -> Vec<ServiceCall> {
        self.calls.take()
    }

    /// The accessibility tree as JSON (see [`a11y_nodes`]).
    pub fn a11y(&self) -> Value {
        self.rt.accessibility_json()
    }

    /// Whether an element keyed `key` is mounted.
    pub fn has_key(&self, key: &str) -> bool {
        has_key(&self.rt, key)
    }

    /// Whether an element's text holds `needle`.
    pub fn has_text(&self, needle: &str) -> bool {
        has_text(&self.rt, needle)
    }

    /// The label of the first element keyed `key`.
    pub fn label_of(&self, key: &str) -> Option<String> {
        label_of(&self.rt, key)
    }

    /// The app state the view shows.
    pub fn state(&self) -> &AppState {
        &self.rt.state
    }
}

/// `request`'s scan as a result, `None` when its search moved on.
fn rebuilt(request: RebuildRequest) -> Option<Rebuilt> {
    let matches = request.scan()?;
    Some(Rebuilt {
        generation: request.current,
        scanned: request.rows.len(),
        matcher: request.matcher,
        result: Ok(matches),
        upto: request.upto,
        follow_up: request.follow_up,
        elapsed_ms: 0,
    })
}

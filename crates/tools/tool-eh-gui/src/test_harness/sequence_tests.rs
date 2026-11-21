use super::Harness;
use super::builders::{
    BILLING, ORDERS, ORDERS_DEV, billing_config, launch_folder, listing, orders_config, profile,
    profile_home, state, stopped, with_profiles,
};
use super::journeys::orders;
use super::services::ServiceCall;
use super::toast_tests::focus_on_a_shown_card;
use crate::archive::ArchiveInput;
use crate::archive::bulk::BulkExport;
use crate::archive::test_support::{cursor, messages, page};
use crate::bridge::EngineDone;
use crate::bridge::test_support::detached_session;
use crate::design::tokens::WINDOW_SIZE;
use crate::read::ReadInput;
use crate::read::inspector::InspectorInput;
use crate::read::rows::ROW_CAP;
use crate::read::search::RebuildRequest;
use crate::read::test_support::captured;
use crate::send::SendInput;
use crate::send::run::SendRun;
use crate::state::{AppState, Input, ListSection, MAX_TOASTS, Section};
use rand::rngs::Xoshiro256PlusPlus;
use rand::{Rng, SeedableRng};
use rune::{Key, KeyEvent, Mods};
use shared_eventhub_core::engine::EngineEvent;
use shared_eventhub_core::models::MessageKey;
use shared_eventhub_core::producer::{SendEnd, SendSummary};
use shared_eventhub_persistence::config::ConfigLoad;
use shared_eventhub_persistence::export::ExportOutcome;
use shared_eventhub_persistence::search::SearchCursor;
use shared_eventhub_persistence::workspace::{ConfigHome, ProfileEntry};
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;

/// Frames after each step: enough for the view to mount what the step changed.
const STEP_FRAMES: usize = 3;
const STEPS: usize = 300;
const REPLAY_STEPS: usize = 100;
const BODIES: [&str; 4] = ["order 7", "{\"refund\": 3}", "ping", "order shipped"];
/// A failed prepare's text: long enough that its surfaces offer the details modal.
const LONG_FAILURE: &str = "the archive is in use by another eh or eh-gui; close the other \
    process that holds the data folder's archive.redb, or pick another data folder in the \
    connect sheet, then connect again";

/// A call the app made that the outside world has not finished answering.
enum Open {
    Prepare(u64),
    Session(u64),
    Save,
    Rebuild(RebuildRequest),
    ExportOne {
        section: ListSection,
        owner: u64,
        key: MessageKey,
    },
    Page(u64),
    Bulk(u64),
    Send {
        run: u64,
        stage: u8,
    },
    List(u64),
    Load {
        request: u64,
        dir: PathBuf,
    },
    Create {
        request: u64,
        name: String,
    },
}

/// How an open call is answered.
#[derive(Clone, Copy, Debug)]
enum Answer {
    InOrder,
    Twice,
    Stale,
}

/// What a sequence's windows and generator hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mix {
    /// No workspace, and the base actions only.
    Base,
    /// The test workspace, and the base actions plus profile picks and connects cancelled
    /// while preparing.
    Profiles,
}

struct Sequence {
    seed: u64,
    mix: Mix,
    rng: Xoshiro256PlusPlus,
    h: Harness,
    step: usize,
    trace: Vec<String>,
    open: Vec<Open>,
    unfinished_sends: Vec<u64>,
    next_sequence: i64,
    cursor: SearchCursor,
    dialogs: TempDir,
    /// Steps that ended with a card's details holding toasts.
    held_steps: usize,
    /// Steps that ended with a message export as the only job running.
    lone_export_steps: usize,
    /// The workspace's listing and the homes a pick chooses from.
    entries: Vec<ProfileEntry>,
    homes: Vec<ConfigHome>,
    /// Steps that moved the home.
    switched_steps: usize,
    /// Picks made while the data folder was held.
    refused_picks: usize,
    /// Steps that ended with an abandoned connect attempt holding the data folder.
    abandoned_steps: usize,
    /// Set when the step's quit opened a fresh window, whose home is the launch folder again.
    fresh_window: bool,
}

impl Drop for Sequence {
    fn drop(&mut self) {
        if std::thread::panicking() {
            let from = self.trace.len().saturating_sub(10);
            eprintln!(
                "seed={} step={} last actions: {:#?}",
                self.seed,
                self.step,
                &self.trace[from..]
            );
        }
    }
}

fn window(mix: Mix) -> Harness {
    let mut s = match mix {
        Mix::Base => state(),
        Mix::Profiles => with_profiles(state()),
    };
    s.form.connection_string.edit_mut().replace_text(ORDERS);
    s.send.refresh_destination(&s.form);
    let mut h = Harness::new(s, WINDOW_SIZE);
    h.settle_frames = STEP_FRAMES;
    h
}

fn completed() -> SendSummary {
    SendSummary {
        sent: 1,
        batches: 1,
        elapsed: Duration::from_millis(5),
        end: SendEnd::Completed,
    }
}

/// Whether a session, a save, a message export in either section, a bulk export, or a send still
/// runs: the jobs the quit waits for.
fn jobs_running(s: &AppState) -> bool {
    s.connection.session().is_some()
        || s.save_status.is_saving()
        || s.read.export_running()
        || s.archive.busy()
        || s.send.is_running()
}

/// Whether a message export runs with no other job holding the data folder, so only the lock's
/// message export arm keeps the folder from changing.
fn lone_message_export(s: &AppState) -> bool {
    let exporting = s.read.export_running() || s.archive.inspector.export.is_running();
    exporting
        && !s.connection.is_active()
        && !s.save_status.is_saving()
        && !s.archive.bulk.is_running()
        && !s.send.is_running()
}

/// The run a send in `run` belongs to, while it runs.
fn running_send(run: &SendRun) -> Option<u64> {
    match run {
        SendRun::Preparing { run, .. } | SendRun::Sending { run, .. } => Some(*run),
        SendRun::Idle | SendRun::Done { .. } => None,
    }
}

impl Sequence {
    fn new(seed: u64, mix: Mix) -> Self {
        let mut sequence = Self {
            seed,
            mix,
            rng: Xoshiro256PlusPlus::seed_from_u64(seed),
            h: window(mix),
            step: 0,
            trace: Vec::new(),
            open: Vec::new(),
            unfinished_sends: Vec::new(),
            next_sequence: 0,
            cursor: cursor(),
            dialogs: tempfile::tempdir().unwrap(),
            held_steps: 0,
            lone_export_steps: 0,
            entries: listing(),
            homes: vec![
                ConfigHome::CurrentFolder(launch_folder()),
                profile_home(BILLING),
                profile_home(ORDERS_DEV),
            ],
            switched_steps: 0,
            refused_picks: 0,
            abandoned_steps: 0,
            fresh_window: false,
        };
        sequence.open_window();
        sequence
    }

    /// A fresh window; about half of them (by the seed) start capturing with some rows.
    fn open_window(&mut self) {
        self.h = window(self.mix);
        self.fresh_window = true;
        self.open.clear();
        self.unfinished_sends.clear();
        if self.below(2) == 0 {
            let (session, _) = self.h.start_session(4);
            self.open.push(Open::Session(session));
            self.messages();
            self.trace.push("window opens capturing".to_owned());
        } else {
            self.trace.push("window opens".to_owned());
        }
    }

    fn below(&mut self, n: usize) -> usize {
        (self.rng.next_u64() % n as u64) as usize
    }

    fn run(&mut self, steps: usize) {
        for step in 0..steps {
            self.step = step;
            let locked = self.h.state().data_folder_lock().is_some();
            let finishing = self.h.state().still_finishing;
            let home = self.h.state().home().clone();
            self.fresh_window = false;
            let action = self.act();
            self.trace.push(action);
            self.collect(finishing, locked);
            self.check();
            if self.h.state().toast_hold().is_some() {
                self.held_steps += 1;
            }
            if lone_message_export(self.h.state()) {
                self.lone_export_steps += 1;
            }
            if *self.h.state().home() != home && !self.fresh_window {
                self.switched_steps += 1;
            }
            if !self.h.state().abandoned_attempts().is_empty() {
                self.abandoned_steps += 1;
            }
        }
    }

    /// One step, by weight; user input only while the window is open, and the profile actions
    /// only in [`Mix::Profiles`] (in [`Mix::Base`] their rolls answer an open call).
    fn act(&mut self) -> String {
        let closed = self.h.state().window_is_closed();
        let profiles = !closed && self.mix == Mix::Profiles;
        let roll = self.below(100);
        match roll {
            0..27 if !closed => self.press(),
            27..30 if !closed => self.pointer(),
            30..42 if !closed => self.key(),
            42..47 if !closed => self.paste(),
            47..52 if !closed => self.wheel(),
            52..56 if !closed => self.answer_dialog(),
            56..64 => self.messages(),
            64..70 => {
                self.h.deliver(Input::Read(ReadInput::ListFlush));
                "flush".to_owned()
            }
            70..78 => {
                self.h.tick();
                "tick".to_owned()
            }
            78 => {
                let now = self.h.now();
                self.h.deliver(Input::WindowClosed(now));
                "window closed".to_owned()
            }
            79..83 => {
                self.h
                    .deliver(Input::ConfigSaved(Err(LONG_FAILURE.to_owned())));
                "a long failure toasts".to_owned()
            }
            83..87 if profiles => self.pick_profile(),
            87..89 if profiles => {
                self.h.deliver(Input::ConnectPressed);
                self.h.deliver(Input::DisconnectPressed);
                "connect, then disconnect while preparing".to_owned()
            }
            _ => self.answer(),
        }
    }

    /// A pick through the keyboard and accessibility path: the launch folder or a profile.
    fn pick_profile(&mut self) -> String {
        let index = self.below(self.homes.len());
        let home = self.homes[index].clone();
        if self.h.state().data_folder_lock().is_some() {
            self.refused_picks += 1;
        }
        let label = crate::state::home_label(&home);
        self.h.deliver(Input::ProfilePicked(home));
        format!("pick {label}")
    }

    /// What the real load of `dir` would read: nothing in the launch folder, each profile's
    /// config, and now and then a failure.
    fn load_result(&mut self, dir: &std::path::Path) -> Result<ConfigLoad, String> {
        if self.below(6) == 0 {
            return Err(LONG_FAILURE.to_owned());
        }
        let named = |name: &str| {
            self.homes
                .iter()
                .any(|home| home.profile_name() == Some(name) && home.dir() == dir)
        };
        if named(BILLING) {
            Ok(ConfigLoad::Loaded(Box::new(billing_config())))
        } else if named(ORDERS_DEV) {
            Ok(ConfigLoad::Loaded(Box::new(orders_config())))
        } else {
            Ok(ConfigLoad::Missing)
        }
    }

    fn press(&mut self) -> String {
        let mut targets: Vec<_> = self
            .h
            .rt
            .elements()
            .into_iter()
            .filter(|e| e.key.is_some() && !e.exiting && !e.inert)
            .filter(|e| e.focusable || e.role == "listitem" || e.role == "button")
            .filter(|e| e.visible[2] > 0.0 && e.visible[3] > 0.0)
            .collect();
        if targets.is_empty() {
            return "press nothing".to_owned();
        }
        targets.sort_by(|a, b| a.path.cmp(&b.path));
        let target = targets.swap_remove(self.below(targets.len()));
        let [x, y, w, h] = target.visible;
        self.h.press_at(x + w / 2.0, y + h / 2.0);
        format!("press {}", target.path)
    }

    /// The pointer onto the toasts, or out of the window.
    fn pointer(&mut self) -> String {
        if self.below(2) == 0 && self.h.rt.find("toast-stack").is_some() {
            self.h.hover("toast-stack");
            "pointer over the toasts".to_owned()
        } else {
            self.h.pointer_away();
            "pointer away".to_owned()
        }
    }

    fn key(&mut self) -> String {
        let pick = self.below(11);
        let (name, key, command) = match pick {
            0 => ("Escape", Key::Escape, false),
            1 => ("Tab", Key::Tab, false),
            2 => ("Down", Key::Down, false),
            3 => ("Up", Key::Up, false),
            4 => ("mod+f", Key::Command('f'), true),
            5 => ("mod+1", Key::Command('1'), true),
            6 => ("mod+2", Key::Command('2'), true),
            7 => ("mod+3", Key::Command('3'), true),
            8 => ("mod+enter", Key::Enter, true),
            9 => {
                self.h.rt.key(KeyEvent {
                    key: Key::Text("t".into()),
                    mods: Mods {
                        alt: true,
                        ..Mods::default()
                    },
                });
                self.h.settle();
                return "key alt+t".to_owned();
            }
            _ => ("Enter", Key::Enter, false),
        };
        if command {
            self.h.command(key);
        } else {
            self.h.key(key);
        }
        format!("key {name}")
    }

    fn paste(&mut self) -> String {
        let body = BODIES[self.below(BODIES.len())];
        if !self.h.rt.has_focus() {
            return "paste with nothing focused".to_owned();
        }
        self.h.rt.paste(body);
        self.h.settle();
        format!("paste {body:?}")
    }

    fn wheel(&mut self) -> String {
        let down = self.below(2) == 0;
        let dy = if down { -1_500.0 } else { 1_500.0 };
        self.h.rt.wheel(640.0, 500.0, dy);
        self.h.settle();
        format!("wheel {}", if down { "down" } else { "up" })
    }

    fn answer_dialog(&mut self) -> String {
        if self.h.rt.file_dialog().is_none() {
            return "no dialog to answer".to_owned();
        }
        let path = self.dialogs.path().join("picked");
        if self.below(2) == 0 {
            self.h.answer_dialog(Some(&path));
            "dialog: temp path".to_owned()
        } else {
            self.h.answer_dialog(None);
            "dialog: none".to_owned()
        }
    }

    fn messages(&mut self) -> String {
        let Some(session) = self.h.state().connection.session().map(|s| s.id()) else {
            return "messages with no session".to_owned();
        };
        let count = 1 + self.below(200);
        let rows = (0..count)
            .map(|_| {
                let partition = self.below(4).to_string();
                let body = BODIES[self.below(BODIES.len())];
                self.next_sequence += 1;
                captured(&partition, self.next_sequence, body)
            })
            .collect();
        let at = self.h.now();
        self.h.deliver(Input::Delivery(crate::bridge::UiDelivery {
            at,
            session,
            events: rows_to_events(rows),
            stats: None,
        }));
        format!("{count} messages")
    }

    fn answer(&mut self) -> String {
        if self.open.is_empty() {
            return "nothing to answer".to_owned();
        }
        let index = self.below(self.open.len());
        let how = match self.below(10) {
            0 => Answer::Twice,
            1 => Answer::Stale,
            _ => Answer::InOrder,
        };
        let label = match how {
            Answer::Stale => self.stale_answer(index),
            Answer::InOrder | Answer::Twice => {
                let open = self.open.remove(index);
                self.deliver_answer(open, how)
            }
        };
        format!("answer {how:?} {label}")
    }

    /// Delivers an answer tagged with an id that is not the open call's (one before it, or for an
    /// export another owner), the shape of a reply to an earlier attempt, run, page, or folder;
    /// the call itself stays open.
    fn stale_answer(&mut self, index: usize) -> String {
        let now = self.h.now();
        let (input, label) = match &self.open[index] {
            Open::Prepare(attempt) => {
                let attempt = attempt.wrapping_sub(1);
                let (session, _) = detached_session(attempt, self.h.tokio().handle());
                let input = Input::SessionReady {
                    attempt,
                    result: Ok(session),
                };
                (input, format!("prepare {attempt}"))
            }
            Open::Session(session) => {
                let session = session.wrapping_sub(1);
                let input = Input::EngineDone(EngineDone {
                    session,
                    engine: Ok(stopped()),
                    finish: Ok(ExportOutcome::default()),
                });
                (input, format!("session {session} done"))
            }
            Open::ExportOne {
                section,
                owner,
                key,
            } => {
                let owner = owner.wrapping_add(1);
                let input = Input::inspector(
                    *section,
                    InspectorInput::ExportDone {
                        owner,
                        key: key.clone(),
                        result: Ok(()),
                        now,
                    },
                );
                (input, format!("export one for owner {owner}"))
            }
            Open::Page(generation) => {
                let generation = generation.wrapping_sub(1);
                let input = Input::Archive(ArchiveInput::PageLoaded {
                    generation,
                    elapsed_ms: 1,
                    result: Ok(page(messages(3), None)),
                });
                (input, format!("page {generation}"))
            }
            Open::Bulk(export) => {
                let export = export.wrapping_sub(1);
                let input = Input::Archive(ArchiveInput::BulkDone {
                    export,
                    elapsed_ms: 1,
                    result: Ok(ExportOutcome::default()),
                });
                (input, format!("bulk {export} done"))
            }
            Open::Send { run, .. } => {
                let run = run.wrapping_sub(1);
                let input = Input::Send(SendInput::Finished {
                    run,
                    summary: completed(),
                    close_error: None,
                });
                (input, format!("send {run} finished"))
            }
            Open::List(request) => {
                let request = request.wrapping_sub(1);
                let input = Input::ProfilesListed {
                    request,
                    result: Ok(self.entries.clone()),
                };
                (input, format!("list {request}"))
            }
            Open::Load { request, .. } => {
                let request = request.wrapping_sub(1);
                let input = Input::ProfileLoaded {
                    request,
                    result: Ok(ConfigLoad::Loaded(Box::new(orders_config()))),
                };
                (input, format!("load {request}"))
            }
            Open::Create { request, .. } => {
                let request = request.wrapping_sub(1);
                let input = Input::ProfileCreated {
                    request,
                    result: Err("disk full".to_owned()),
                };
                (input, format!("create {request}"))
            }
            Open::Save | Open::Rebuild(_) => return "no stale form of this call".to_owned(),
        };
        self.h.deliver(input);
        label
    }

    /// Delivers what the real task would post for `open`; puts back what is still open, and
    /// `open` itself once more when it is answered twice.
    fn deliver_answer(&mut self, open: Open, how: Answer) -> String {
        match open {
            Open::Prepare(attempt) => {
                let ok = self.below(5) != 0;
                let result = if ok {
                    Ok(detached_session(attempt, self.h.tokio().handle()).0)
                } else if self.below(2) == 0 {
                    Err(LONG_FAILURE.to_owned())
                } else {
                    Err("the archive is in use by another eh".to_owned())
                };
                self.h.deliver(Input::SessionReady { attempt, result });
                if ok {
                    let at = self.h.now();
                    self.h.deliver(Input::Delivery(crate::bridge::UiDelivery {
                        at,
                        session: attempt,
                        events: vec![EngineEvent::Started {
                            partitions: (0..4).map(|p| p.to_string()).collect(),
                        }],
                        stats: None,
                    }));
                    self.open.push(Open::Session(attempt));
                }
                self.reopen_if_twice(how, Open::Prepare(attempt));
                format!("prepare {attempt} ok={ok}")
            }
            Open::Session(session) => {
                self.h.deliver(Input::EngineDone(EngineDone {
                    session,
                    engine: Ok(stopped()),
                    finish: Ok(ExportOutcome::default()),
                }));
                self.reopen_if_twice(how, Open::Session(session));
                format!("session {session} done")
            }
            Open::Save => {
                let ok = self.below(2) == 0;
                let result = if ok {
                    Ok(())
                } else {
                    Err("disk full".to_owned())
                };
                self.h.deliver(Input::ConfigSaved(result));
                self.reopen_if_twice(how, Open::Save);
                format!("save ok={ok}")
            }
            Open::Rebuild(request) => {
                if let Some(rebuilt) = super::rebuilt(request) {
                    self.h
                        .deliver(Input::Read(ReadInput::SearchRebuilt(rebuilt)));
                }
                "rebuild".to_owned()
            }
            Open::ExportOne {
                section,
                owner,
                key,
            } => {
                let now = self.h.now();
                let label = format!("export one {}", key.sequence_number);
                self.h.deliver(Input::inspector(
                    section,
                    InspectorInput::ExportDone {
                        owner,
                        key: key.clone(),
                        result: Ok(()),
                        now,
                    },
                ));
                self.reopen_if_twice(
                    how,
                    Open::ExportOne {
                        section,
                        owner,
                        key,
                    },
                );
                label
            }
            Open::Page(generation) => {
                let more = self.below(2) == 0;
                let next = more.then(|| self.cursor.clone());
                let count = 1 + self.below(120);
                self.h.deliver(Input::Archive(ArchiveInput::PageLoaded {
                    generation,
                    elapsed_ms: 1,
                    result: Ok(page(messages(count), next)),
                }));
                self.reopen_if_twice(how, Open::Page(generation));
                format!("page {generation} of {count} more={more}")
            }
            Open::Bulk(export) => {
                self.h.deliver(Input::Archive(ArchiveInput::BulkDone {
                    export,
                    elapsed_ms: 1,
                    result: Ok(ExportOutcome {
                        exported: 2,
                        ..ExportOutcome::default()
                    }),
                }));
                self.reopen_if_twice(how, Open::Bulk(export));
                format!("bulk {export} done")
            }
            Open::Send { run, stage } => self.send_answer(run, stage, how),
            Open::List(request) => {
                self.h.deliver(Input::ProfilesListed {
                    request,
                    result: Ok(self.entries.clone()),
                });
                self.reopen_if_twice(how, Open::List(request));
                format!("list {request}")
            }
            Open::Load { request, dir } => {
                let result = self.load_result(&dir);
                let ok = result.is_ok();
                self.h.deliver(Input::ProfileLoaded { request, result });
                self.reopen_if_twice(how, Open::Load { request, dir });
                format!("load {request} ok={ok}")
            }
            Open::Create { request, name } => {
                let ok = self.below(2) == 0;
                let result = if ok {
                    Ok(profile(&name))
                } else {
                    Err("disk full".to_owned())
                };
                self.h.deliver(Input::ProfileCreated { request, result });
                self.reopen_if_twice(how, Open::Create { request, name });
                format!("create {request} ok={ok}")
            }
        }
    }

    fn send_answer(&mut self, run: u64, stage: u8, how: Answer) -> String {
        let input = match stage {
            0 => {
                let at = self.h.now();
                SendInput::Started {
                    run,
                    target: orders(),
                    at,
                }
            }
            1 => SendInput::Progress { run, sent: 1 },
            _ => SendInput::Finished {
                run,
                summary: completed(),
                close_error: None,
            },
        };
        self.h.deliver(Input::Send(input));
        let in_order = matches!(how, Answer::InOrder);
        if stage < 2 && in_order {
            self.open.push(Open::Send {
                run,
                stage: stage + 1,
            });
        }
        if stage >= 2 {
            self.unfinished_sends
                .retain(|unfinished| *unfinished != run);
        }
        self.reopen_if_twice(how, Open::Send { run, stage });
        format!("send {run} stage {stage}")
    }

    fn reopen_if_twice(&mut self, how: Answer, open: Open) {
        if matches!(how, Answer::Twice) {
            self.open.push(open);
        }
    }

    /// Takes the calls the step recorded into the open list and checks those the invariants
    /// name: one `Quit` per window, and no folder switch while the folder was locked. A `Quit`
    /// starts a fresh window once the step's calls are checked.
    fn collect(&mut self, finishing: bool, locked: bool) {
        let mut quits = 0;
        for call in self.h.take_calls() {
            match call {
                ServiceCall::PrepareSession { attempt, request } => {
                    assert_eq!(
                        request.paths,
                        self.h.state().data_paths(),
                        "seed={} step={}: a connect for another data folder",
                        self.seed,
                        self.step
                    );
                    self.open.push(Open::Prepare(attempt));
                }
                ServiceCall::SaveConfig(request) => {
                    assert_eq!(
                        request.dir,
                        self.h.state().home().dir(),
                        "seed={} step={}: a save to a folder that is not the home",
                        self.seed,
                        self.step
                    );
                    self.open.push(Open::Save);
                }
                ServiceCall::SwitchDataFolder(_) => {
                    assert!(
                        !locked,
                        "seed={} step={}: a folder switch while locked",
                        self.seed, self.step
                    );
                }
                ServiceCall::ListProfiles { request, .. } => self.open.push(Open::List(request)),
                ServiceCall::LoadProfile { request, dir } => {
                    self.open.push(Open::Load { request, dir });
                }
                ServiceCall::CreateProfile(request) => self.open.push(Open::Create {
                    request: request.request,
                    name: request.name.as_str().to_owned(),
                }),
                ServiceCall::ReopenWindow => {}
                ServiceCall::Quit => quits += 1,
                ServiceCall::RebuildSearch(request) => self.open.push(Open::Rebuild(request)),
                ServiceCall::ExportOne(request) => self.open.push(Open::ExportOne {
                    section: request.section,
                    owner: request.owner,
                    key: request.key,
                }),
                ServiceCall::SearchArchive(request) => {
                    self.open.push(Open::Page(request.generation))
                }
                ServiceCall::BulkExport(request) => self.open.push(Open::Bulk(request.export)),
                ServiceCall::StartSend(request) => {
                    self.unfinished_sends.push(request.run);
                    self.open.push(Open::Send {
                        run: request.run,
                        stage: 0,
                    });
                }
            }
        }
        if quits > 0 {
            self.quit(quits, finishing);
        }
    }

    /// Checks a step's `quits` (`finishing`: a closed window's session was still finishing, so
    /// Force quit may end the app with jobs running) and opens a fresh window.
    fn quit(&mut self, quits: usize, finishing: bool) {
        assert_eq!(
            quits, 1,
            "seed={} step={}: more than one Quit",
            self.seed, self.step
        );
        if !finishing {
            let s = self.h.state();
            assert!(
                s.window_is_closed() && !jobs_running(s),
                "seed={} step={}: Quit with the window open or a job running",
                self.seed,
                self.step
            );
        }
        self.open_window();
    }

    fn check(&self) {
        let s = self.h.state();
        let at = format!("seed={} step={}", self.seed, self.step);
        assert!(
            s.read.rows.displayed().len() <= ROW_CAP,
            "{at}: the list passed its cap"
        );
        let selected = match s.section {
            Section::Read => s.read.inspector.selected.is_some(),
            Section::Export => s.archive.inspector.selected.is_some(),
            Section::Send => false,
        };
        let inspector = self
            .h
            .rt
            .elements()
            .iter()
            .any(|e| e.key.as_deref() == Some("inspector") && !e.exiting);
        assert!(
            !selected || inspector,
            "{at}: a selection without its inspector"
        );
        assert!(
            !jobs_running(s) || s.data_folder_lock().is_some(),
            "{at}: a job runs on the data folder and the folder is free to change"
        );
        if s.window_is_closed() {
            let send_flag = match &s.send.run {
                SendRun::Preparing { cancel, .. } | SendRun::Sending { cancel, .. } => {
                    Some(cancel.load(std::sync::atomic::Ordering::Acquire))
                }
                SendRun::Idle | SendRun::Done { .. } => None,
            };
            let bulk_flag = match &s.archive.bulk {
                BulkExport::Running { cancel, .. } => {
                    Some(cancel.load(std::sync::atomic::Ordering::Acquire))
                }
                BulkExport::Idle | BulkExport::Done { .. } => None,
            };
            assert!(
                send_flag.unwrap_or(true) && bulk_flag.unwrap_or(true),
                "{at}: a job runs on with the window closed"
            );
        }
        if s.details().is_some() {
            assert!(!s.pill_open, "{at}: the details beside the pill popover");
            let mounted = self
                .h
                .rt
                .elements()
                .iter()
                .any(|e| e.key.as_deref() == Some("details-dialog") && !e.exiting);
            assert!(mounted, "{at}: details open with no modal mounted");
        }
        assert!(
            !s.window_is_closed() || s.details().is_none(),
            "{at}: details open with the window closed"
        );
        self.check_toasts(&at);
        assert!(self.unfinished_sends.len() <= 1, "{at}: two sends at once");
        if let Some(run) = running_send(&s.send.run) {
            assert_eq!(
                self.unfinished_sends,
                [run],
                "{at}: the running send is not the open one"
            );
        }
    }
}

impl Sequence {
    /// The toast invariants: the cap, a folded stack with the window closed, folded cards'
    /// controls inert, focus only on a shown card, and the hold's queue.
    fn check_toasts(&self, at: &str) {
        let s = self.h.state();
        let stack = s.toasts();
        assert!(stack.entries().len() <= MAX_TOASTS, "{at}: past the cap");
        assert!(
            !s.window_is_closed() || !stack.stack().expanded(),
            "{at}: fanned out with the window closed"
        );
        let front = stack.front().map(|entry| entry.id);
        if !stack.stack().expanded() {
            for entry in stack.entries().iter().filter(|e| Some(e.id) != front) {
                let close = format!("toast-{}-close", entry.id);
                let live =
                    self.h.rt.elements().into_iter().any(|e| {
                        e.key.as_deref() == Some(close.as_str()) && !e.exiting && !e.inert
                    });
                assert!(!live, "{at}: a folded card's Close takes presses");
            }
        }
        assert!(
            focus_on_a_shown_card(&self.h),
            "{at}: focus on a folded or missing card: {:?}",
            super::focused_key(&self.h.rt)
        );
        match s.toast_hold() {
            None => assert_eq!(s.held_toasts(), 0, "{at}: toasts held with no hold"),
            Some(id) => assert!(
                stack.entries().iter().any(|entry| entry.id == id),
                "{at}: the held card left"
            ),
        }
    }
}

fn rows_to_events(rows: Vec<shared_eventhub_core::models::CapturedMessage>) -> Vec<EngineEvent> {
    rows.into_iter()
        .map(|message| EngineEvent::Message(Box::new(message)))
        .collect()
}

#[rstest::rstest]
#[case::seed_64(64)]
#[case::seed_72(72)]
fn a_seeded_sequence_keeps_every_invariant(#[case] seed: u64) {
    let mut sequence = Sequence::new(seed, Mix::Base);

    sequence.run(STEPS);

    assert_eq!(sequence.step + 1, STEPS);
    assert!(
        sequence.held_steps > 0,
        "seed={seed}: no step held toasts under a card's details"
    );
    assert!(
        sequence.lone_export_steps > 0,
        "seed={seed}: no step ended with a message export as the only job"
    );
}

#[rstest::rstest]
#[case::seed_1(1)]
#[case::seed_3(3)]
fn a_seeded_sequence_with_profiles_keeps_every_invariant(#[case] seed: u64) {
    let mut sequence = Sequence::new(seed, Mix::Profiles);

    sequence.run(STEPS);

    assert_eq!(sequence.step + 1, STEPS);
    assert!(
        sequence.held_steps > 0,
        "seed={seed}: no step held toasts under a card's details"
    );
    assert!(
        sequence.switched_steps > 0,
        "seed={seed}: no step switched the profile"
    );
    assert!(
        sequence.refused_picks > 0,
        "seed={seed}: no pick was made while the data folder was held"
    );
    assert!(
        sequence.abandoned_steps > 0,
        "seed={seed}: no step ended with a cancelled connect holding the data folder"
    );
}

#[test]
fn a_seed_replays_the_same_actions() {
    let mut first = Sequence::new(1, Mix::Profiles);
    let mut second = Sequence::new(1, Mix::Profiles);

    first.run(REPLAY_STEPS);
    second.run(REPLAY_STEPS);

    assert_eq!(first.trace, second.trace);
}

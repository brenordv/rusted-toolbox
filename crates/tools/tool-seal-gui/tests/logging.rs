//! What seal-gui writes to its log: the worker's lines with their fields, and no text being
//! sealed or unsealed and no key, through a failed job, a successful unseal, and a key pasted by
//! mistake. This is the only test in its binary, so a global subscriber sees every event.

use common_gui_rune::test_harness::{
    a11y_nodes, deliver, press, runtime_with_keys, settle, type_into,
};
use rune::{FileDialogError, Runtime, TextEdit};
use seal_gui::config::{ConfigStore, RecipientEntry, SaveOutcome, SealConfig};
use seal_gui::design::WINDOW_SIZE;
use seal_gui::services::{ProdServices, Services, dialog_answered, dispatch};
use seal_gui::state::files::PathKind;
use seal_gui::state::notes::Note;
use seal_gui::state::text::TextInput;
use seal_gui::state::{AppState, DialogPurpose, Input, Tab};
use seal_gui::views::{INPUT_KEY, OTHER_KEY_KEY, TEXT_SUBMIT_KEY, view, window_key};
use seal_gui::worker::{
    self, JobKind, JobRequest, WorkerJoin, WorkerMsg, log_file_failed, log_job_failed, report_stop,
};
use shared_crypto::keys::{generate_identity, public_key_of, save_identity_file};
use shared_crypto::{Direction, engine};
use std::cell::RefCell;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const MARKER: &str = "marker-48151";
const KEY_MARKER: &str = "AGE-SECRET-KEY-1MARKER48151";
const WAIT: Duration = Duration::from_secs(10);

#[derive(Clone, Default)]
struct SharedBuffer(Arc<Mutex<Vec<u8>>>);

impl Write for SharedBuffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl SharedBuffer {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

/// The real services behind ones that keep each request's `Debug`.
struct Recorded {
    prod: ProdServices,
    seen: RefCell<Vec<String>>,
}

impl Services for Recorded {
    fn run(&self, request: JobRequest) -> anyhow::Result<()> {
        self.seen.borrow_mut().push(format!("{request:?}"));
        self.prod.run(request)
    }

    fn inspect(&self, paths: Vec<PathBuf>) -> Vec<(PathBuf, PathKind)> {
        self.prod.inspect(paths)
    }

    fn probe(&self, pairs: Vec<(PathBuf, PathBuf)>) -> Vec<(PathBuf, bool)> {
        self.prod.probe(pairs)
    }

    fn save_config(&self, snapshot: &SealConfig) -> SaveOutcome {
        self.prod.save_config(snapshot)
    }
}

struct Window {
    rt: Runtime<AppState>,
    services: Rc<Recorded>,
    posted: Receiver<WorkerMsg>,
    join: WorkerJoin,
    /// The `Debug` of every input delivered and every worker message.
    seen: Vec<String>,
}

impl Window {
    /// A window over `config` whose settings are saved nowhere.
    fn open(config: SealConfig) -> Self {
        let nowhere = ConfigStore::boot(None, |_| None);
        let (post, posted) = mpsc::channel::<WorkerMsg>();
        let (submit, join) = worker::spawn(move |message| {
            let _sent = post.send(message);
        })
        .unwrap();
        let services = Rc::new(Recorded {
            prod: ProdServices::new(submit, nowhere.store),
            seen: RefCell::new(Vec::new()),
        });
        let state = AppState::new(
            (config, nowhere.status),
            common_cli::app_logger::FileLogging::Off,
            Some(services.clone() as Rc<dyn Services>),
            None,
        );
        Self {
            rt: runtime_with_keys(state, view, window_key, WINDOW_SIZE),
            services,
            posted,
            join,
            seen: Vec::new(),
        }
    }

    fn deliver(&mut self, input: Input) {
        self.seen.push(format!("{input:?}"));
        deliver(&mut self.rt, move |s, tx| dispatch(s, tx, input));
    }

    /// Delivers the worker's messages up to the job's answer.
    fn deliver_answer(&mut self) {
        loop {
            let message = self.posted.recv_timeout(WAIT).unwrap();
            let last = matches!(
                &message,
                WorkerMsg::Job(worker::JobEvent::Finished(_) | worker::JobEvent::Failed(_))
                    | WorkerMsg::Stopped
            );
            self.seen.push(format!("{message:?}"));
            self.deliver(Input::from(message));
            if last {
                return;
            }
        }
    }

    /// Everything a reader of the window's Debug and recorded traffic would see.
    fn debug_text(&self) -> String {
        format!(
            "{:?} {:?} {:?}",
            self.rt.state,
            self.seen,
            self.services.seen.borrow()
        )
    }

    /// Every accessible description in the window, joined.
    fn descriptions(&self) -> String {
        let tree = self.rt.accessibility_json();
        a11y_nodes(&tree)
            .into_iter()
            .filter_map(|node| node["description"].as_str().map(str::to_owned))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn close(self) {
        self.join.finish();
    }
}

fn sealed_marker(recipient_key: &str) -> String {
    let recipient = recipient_key.parse().unwrap();
    let mut input = MARKER.as_bytes();
    let mut armored = Vec::new();
    engine::encrypt(&[recipient], &mut input, &mut armored, true).unwrap();
    String::from_utf8(armored).unwrap()
}

#[test]
fn the_log_holds_the_worker_lines_and_no_text_or_key() {
    let logs = SharedBuffer::default();
    let writer = logs.clone();
    tracing::subscriber::set_global_default(
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish(),
    )
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let identity = generate_identity();
    let identity_path = dir.path().join("identity.txt");
    save_identity_file(&identity, &identity_path, false).unwrap();

    // The guard's line, and the failure lines, with their fields.
    report_stop(&Mutex::new(Some(JobKind::Files)), &|_| {});
    log_job_failed(JobKind::Text, "the identity file is damaged");
    log_file_failed(Path::new("report.pdf"), "could not open input file");

    // (a) A marker typed, sealed to a key that does not parse in a hand-edited book: the start
    // is refused, then a marker sealed with no recipient reaches the worker as a failing unseal.
    let mut failing = Window::open(SealConfig {
        recipients: vec![RecipientEntry {
            label: "Alice".to_owned(),
            public_key: "hand-edited garbage".to_owned(),
        }],
        ..SealConfig::default()
    });
    type_into(&mut failing.rt, INPUT_KEY, MARKER);
    failing.deliver(Input::Text(TextInput::DirectionPicked(Direction::Decrypt)));
    failing.rt.state.identity_path = Some(dir.path().join("absent-identity.txt"));
    press(&mut failing.rt, TEXT_SUBMIT_KEY);
    failing.deliver_answer();
    settle(&mut failing.rt);
    let failed_status = failing
        .rt
        .state
        .status(Tab::Text)
        .map(|(_, words)| words)
        .unwrap_or_default();
    let failing_a11y = failing.descriptions();
    let failing_debug = failing.debug_text();
    failing.close();

    // (b) A marker unsealed successfully.
    let mut unsealing = Window::open(SealConfig::default());
    unsealing.deliver(Input::Text(TextInput::DirectionPicked(Direction::Decrypt)));
    unsealing.rt.state.identity_path = Some(identity_path.clone());
    unsealing.rt.state.text.input = TextEdit::new(sealed_marker(&public_key_of(&identity)));
    settle(&mut unsealing.rt);
    press(&mut unsealing.rt, TEXT_SUBMIT_KEY);
    unsealing.deliver_answer();
    let unsealed = unsealing.rt.state.text.output.clone();
    let unsealing_debug = unsealing.debug_text();
    unsealing.close();

    // (c) A secret key pasted into Other key, then Seal pressed.
    let mut pasted = Window::open(SealConfig::default());
    type_into(&mut pasted.rt, INPUT_KEY, "hello");
    type_into(&mut pasted.rt, OTHER_KEY_KEY, KEY_MARKER);
    pasted.deliver(Input::Text(TextInput::OtherKeyEdited));
    press(&mut pasted.rt, TEXT_SUBMIT_KEY);
    let pasted_status = pasted
        .rt
        .state
        .status(Tab::Text)
        .map(|(_, words)| words)
        .unwrap_or_default();
    let pasted_a11y = pasted.descriptions();
    let pasted_debug = pasted.debug_text();
    pasted.close();

    // (d) The add-file dialog fails to show.
    let mut dialog = Window::open(SealConfig::default());
    dialog.deliver(Input::TabPicked(Tab::Files));
    dialog.deliver(Input::DialogOpened(DialogPurpose::AddFile));
    deliver(&mut dialog.rt, |s, tx| {
        dialog_answered(
            s,
            tx,
            DialogPurpose::AddFile,
            Err(FileDialogError("no portal".to_owned())),
        )
    });
    let dialog_note = dialog.rt.state.note(Tab::Files).cloned();
    dialog.close();

    // (e) S1: a fresh profile whose config folder turns into a file after boot, so the save
    // fails. (A file in place before boot reads as unreadable on some systems, which refuses
    // saves instead.)
    let home = dir.path().join("profile");
    let toolbox = home.join(".rusted-toolbox");
    std::fs::create_dir_all(&toolbox).unwrap();
    let boot = ConfigStore::boot(Some(home), |_| None);
    std::fs::write(toolbox.join("seal-gui"), b"in the way").unwrap();
    let saved = boot.store.flush(&SealConfig::default());

    let log = logs.text();
    assert!(matches!(saved, SaveOutcome::Failed(_)), "{saved:?}");
    assert!(
        log.lines().any(|line| line.contains("config save failed")
            && line.contains("WARN")
            && line.contains("config.json")),
        "{log}"
    );
    assert_eq!(dialog_note, Some(Note::DialogError("no portal".to_owned())));
    assert!(
        log.contains("a file dialog failed")
            && log.contains("purpose=\"add-file\"")
            && log.contains("error=no portal"),
        "{log}"
    );
    assert!(
        log.contains("the worker thread stopped without being asked to")
            && log.contains("kind=\"files\""),
        "{log}"
    );
    assert!(
        log.contains("job failed") && log.contains("error=the identity file is damaged"),
        "{log}"
    );
    assert!(
        log.contains("a file in the batch failed") && log.contains("path=\"report.pdf\""),
        "{log}"
    );
    assert!(
        log.contains("job started") && log.contains("job finished"),
        "{log}"
    );
    assert!(log.contains("elapsed_ms="), "{log}");
    assert_eq!(unsealed, MARKER);
    assert!(!failed_status.is_empty());
    assert!(!pasted_status.is_empty());
    for (case, text) in [
        ("log", log.as_str()),
        ("failed status", failed_status.as_str()),
        ("failed a11y", failing_a11y.as_str()),
        ("failed debug", failing_debug.as_str()),
        ("unseal debug", unsealing_debug.as_str()),
        ("pasted status", pasted_status.as_str()),
        ("pasted a11y", pasted_a11y.as_str()),
        ("pasted debug", pasted_debug.as_str()),
    ] {
        assert!(!text.contains(MARKER), "{case} holds the text: {text}");
        assert!(
            !text.contains("MARKER48151"),
            "{case} holds the key: {text}"
        );
    }
}

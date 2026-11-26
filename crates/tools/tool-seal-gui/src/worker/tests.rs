use super::*;
use rstest::rstest;
use shared_crypto::Identity;
use std::fs;
use std::sync::mpsc::Receiver;
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(10);

struct Worker {
    submit: WorkerSubmit,
    join: WorkerJoin,
    posted: Receiver<WorkerMsg>,
}

fn start() -> Worker {
    let (post, posted) = mpsc::channel::<WorkerMsg>();
    let (submit, join) = spawn(move |msg| {
        // The receiver lives as long as the test; a failed send only means it ended.
        let _sent = post.send(msg);
    })
    .unwrap();
    Worker {
        submit,
        join,
        posted,
    }
}

impl Worker {
    fn next(&self) -> WorkerMsg {
        self.posted
            .recv_timeout(WAIT)
            .expect("the worker posted nothing")
    }

    /// Every message up to and including the job's final answer.
    fn until_answer(&self) -> Vec<WorkerMsg> {
        let mut messages = Vec::new();
        loop {
            let message = self.next();
            let last = matches!(
                message,
                WorkerMsg::Job(JobEvent::Finished(_) | JobEvent::Failed(_)) | WorkerMsg::Stopped
            );
            messages.push(message);
            if last {
                return messages;
            }
        }
    }

    fn answer(&self) -> JobEvent {
        match self.until_answer().pop() {
            Some(WorkerMsg::Job(event)) => event,
            other => panic!("expected a job answer, got {other:?}"),
        }
    }
}

fn outcome(event: JobEvent) -> JobOutcome {
    match event {
        JobEvent::Finished(outcome) => outcome,
        other => panic!("expected Finished, got {other:?}"),
    }
}

fn failure(event: JobEvent) -> String {
    match event {
        JobEvent::Failed(text) => text,
        other => panic!("expected Failed, got {other:?}"),
    }
}

/// A fresh identity file and the matching recipients.
fn identity_fixture(dir: &Path) -> (PathBuf, Vec<Recipient>) {
    let identity = keys::generate_identity();
    let path = dir.join("identity.txt");
    keys::save_identity_file(&identity, &path, false).unwrap();
    (path, vec![identity.to_public()])
}

#[test]
fn a_job_that_panics_posts_stopped_once_and_finish_still_returns() {
    let worker = start();

    worker.submit.run(JobRequest::PanicForTests).unwrap();
    let messages = worker.until_answer();

    assert!(matches!(messages.as_slice(), [WorkerMsg::Stopped]));
    assert_eq!(worker.join.running(), None);
    worker.join.finish();
    assert!(
        worker
            .posted
            .recv_timeout(Duration::from_millis(200))
            .is_err()
    );
}

#[test]
fn a_file_batch_that_panics_posts_its_events_before_stopped() {
    let dir = tempfile::tempdir().unwrap();
    let present = dir.path().join("present.txt");
    fs::write(&present, b"present").unwrap();
    let worker = start();

    worker
        .submit
        .run(JobRequest::EncryptFiles {
            files: vec![present],
            recipients: vec![Identity::generate().to_public()],
            out_dir: None,
            overwrite: false,
        })
        .unwrap();
    worker.answer();
    worker.submit.run(JobRequest::PanicForTests).unwrap();

    assert!(matches!(worker.next(), WorkerMsg::Stopped));
}

#[test]
fn finish_on_an_idle_worker_posts_nothing_and_returns() {
    let worker = start();

    worker.join.finish();

    assert!(
        worker
            .posted
            .recv_timeout(Duration::from_millis(200))
            .is_err()
    );
}

/// Runs a job that holds the worker; returns once it runs, with the sender that releases it.
fn hold(worker: &Worker) -> mpsc::Sender<()> {
    let (started, started_rx) = mpsc::channel::<()>();
    let (release, held) = mpsc::channel::<()>();
    worker
        .submit
        .run(JobRequest::HoldForTests {
            started,
            release: held,
        })
        .unwrap();
    started_rx.recv_timeout(WAIT).unwrap();
    release
}

#[test]
fn finish_with_a_job_running_returns_after_the_jobs_answer() {
    let worker = start();
    let release = hold(&worker);
    let (finished, finished_rx) = mpsc::channel::<()>();
    let join = worker.join;

    let waiter = thread::spawn(move || {
        join.finish();
        let _sent = finished.send(());
    });
    let early = finished_rx.recv_timeout(Duration::from_millis(200));
    release.send(()).unwrap();
    let answer = worker.posted.recv_timeout(WAIT).unwrap();

    assert!(early.is_err());
    assert!(matches!(
        answer,
        WorkerMsg::Job(JobEvent::Finished(JobOutcome::TextRedirect))
    ));
    assert!(finished_rx.recv_timeout(WAIT).is_ok());
    waiter.join().unwrap();
    assert!(
        worker
            .posted
            .recv_timeout(Duration::from_millis(200))
            .is_err()
    );
}

#[test]
fn a_stopped_worker_posts_nothing_more() {
    let worker = start();
    let submit = worker.submit.clone();

    worker.join.finish();

    assert!(submit.run(JobRequest::FailForTests).is_err());
    assert!(
        worker
            .posted
            .recv_timeout(Duration::from_millis(200))
            .is_err()
    );
}

#[test]
fn run_after_the_thread_stopped_answers_that_the_worker_is_gone() {
    let worker = start();
    worker.submit.run(JobRequest::PanicForTests).unwrap();
    worker.until_answer();
    worker.join.finish();

    let error = worker.submit.run(JobRequest::FailForTests).unwrap_err();

    assert_eq!(error.to_string(), WORKER_GONE);
}

#[test]
fn running_names_the_job_in_progress() {
    let worker = start();

    let release = hold(&worker);
    let running = worker.join.running();
    release.send(()).unwrap();
    worker.answer();

    assert_eq!(running, Some(JobKind::Text));
    assert_eq!(worker.join.running(), None);
}

#[test]
fn a_failing_job_answers_failed_with_the_rendered_error() {
    let worker = start();

    worker.submit.run(JobRequest::FailForTests).unwrap();

    assert!(failure(worker.answer()).contains("always fails"));
}

#[test]
fn keygen_without_replace_onto_an_existing_file_leaves_it_and_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("id.txt");
    fs::write(&path, "kept\n").unwrap();
    let worker = start();

    worker
        .submit
        .run(JobRequest::Keygen {
            path: path.clone(),
            replace: false,
        })
        .unwrap();

    assert!(matches!(
        outcome(worker.answer()),
        JobOutcome::KeygenExists { path: answered } if answered == path
    ));
    assert_eq!(fs::read_to_string(&path).unwrap(), "kept\n");
}

#[test]
fn keygen_without_replace_onto_a_new_path_writes_the_identity() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("new-identity.txt");
    let worker = start();

    worker
        .submit
        .run(JobRequest::Keygen {
            path: path.clone(),
            replace: false,
        })
        .unwrap();

    let JobOutcome::Keygen {
        public_key,
        path: saved,
    } = outcome(worker.answer())
    else {
        panic!("expected a keygen outcome");
    };
    assert_eq!(saved, path);
    assert!(public_key.starts_with("age1"));
    let loaded = keys::load_identities(&[path]).unwrap();
    assert_eq!(keys::public_key_of(&loaded[0]), public_key);
}

#[test]
fn keygen_with_replace_writes_over_an_existing_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("id.txt");
    fs::write(&path, "old\n").unwrap();
    let worker = start();

    worker
        .submit
        .run(JobRequest::Keygen {
            path: path.clone(),
            replace: true,
        })
        .unwrap();

    let JobOutcome::Keygen { public_key, .. } = outcome(worker.answer()) else {
        panic!("expected a keygen outcome");
    };
    let loaded = keys::load_identities(&[path]).unwrap();
    assert_eq!(keys::public_key_of(&loaded[0]), public_key);
}

#[test]
fn keygen_that_cannot_write_answers_failed() {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("not-a-folder");
    fs::write(&blocker, "file").unwrap();
    let worker = start();

    worker
        .submit
        .run(JobRequest::Keygen {
            path: blocker.join("id.txt"),
            replace: false,
        })
        .unwrap();

    assert!(failure(worker.answer()).contains("not-a-folder"));
}

#[test]
fn encrypt_then_decrypt_text_round_trips_through_the_worker() {
    let dir = tempfile::tempdir().unwrap();
    let (identity_path, recipients) = identity_fixture(dir.path());
    let worker = start();

    worker
        .submit
        .run(JobRequest::EncryptText {
            text: "a secret note".to_owned(),
            recipients,
        })
        .unwrap();
    let JobOutcome::Text(armored) = outcome(worker.answer()) else {
        panic!("expected armored text");
    };
    worker
        .submit
        .run(JobRequest::DecryptText {
            text: format!("\n  {armored}\n\n"),
            identity_path,
        })
        .unwrap();

    assert!(armored.starts_with("-----BEGIN AGE ENCRYPTED FILE-----"));
    assert!(matches!(
        outcome(worker.answer()),
        JobOutcome::Text(text) if text == "a secret note"
    ));
}

#[test]
fn tampered_armor_fails_with_the_corruption_wording() {
    let dir = tempfile::tempdir().unwrap();
    let (identity_path, recipients) = identity_fixture(dir.path());
    let worker = start();
    worker
        .submit
        .run(JobRequest::EncryptText {
            text: "payload long enough to have a body line".to_owned(),
            recipients,
        })
        .unwrap();
    let JobOutcome::Text(armored) = outcome(worker.answer()) else {
        panic!("expected armored text");
    };
    let end_marker = armored.rfind("-----END").unwrap();
    let target = end_marker - 10;
    let swapped = if armored.as_bytes()[target] == b'A' {
        b'B'
    } else {
        b'A'
    };
    let mut bytes = armored.into_bytes();
    bytes[target] = swapped;
    let tampered = String::from_utf8(bytes).unwrap();

    worker
        .submit
        .run(JobRequest::DecryptText {
            text: tampered,
            identity_path,
        })
        .unwrap();

    let error = failure(worker.answer());
    assert!(error.contains("damaged"), "unexpected wording: {error}");
}

#[test]
fn passphrase_armor_fails_with_the_refusal_wording() {
    const SCRYPT_ARMOR: &str = "-----BEGIN AGE ENCRYPTED FILE-----
YWdlLWVuY3J5cHRpb24ub3JnL3YxCi0+IHNjcnlwdCBpckNobUpjVytTSXA5RzZD
blBVQVNBIDIKbjY2UUhIaDgxSFBUaG1LODdqMUMxNVBhNzdiVCtIbEp3SzZkbE1P
OTB3cwotLS0gVUpyOG5yQmJKUXBmNTVHUHNjQ1pLRDVOSjhhTUFETi9POE16UHBl
dzBFRQo1vRm4dvYl+6UsoqrFf3viXkKqt5Tjr46bijEDnHehgJU4qwywmU4P/hvT
K1nPtvZv1tfOhPEtCjVY
-----END AGE ENCRYPTED FILE-----
";
    let dir = tempfile::tempdir().unwrap();
    let (identity_path, _) = identity_fixture(dir.path());
    let worker = start();

    worker
        .submit
        .run(JobRequest::DecryptText {
            text: SCRYPT_ARMOR.to_owned(),
            identity_path,
        })
        .unwrap();

    let error = failure(worker.answer());
    assert!(
        error.contains("passphrase-encrypted"),
        "unexpected wording: {error}"
    );
}

#[test]
fn binary_plaintext_redirects_to_the_files_tab() {
    assert!(matches!(
        text_or_redirect(vec![0xFF, 0xFE, 0x00]),
        JobOutcome::TextRedirect
    ));
    assert!(matches!(
        text_or_redirect(b"plain text".to_vec()),
        JobOutcome::Text(text) if text == "plain text"
    ));
}

#[test]
fn file_batch_skips_existing_outputs_when_overwrite_is_off_and_seals_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    let fresh = dir.path().join("fresh.txt");
    let blocked = dir.path().join("blocked.txt");
    fs::write(&fresh, b"fresh contents").unwrap();
    fs::write(&blocked, b"blocked contents").unwrap();
    fs::write(dir.path().join("blocked.txt.age"), b"already here").unwrap();
    let worker = start();

    worker
        .submit
        .run(JobRequest::EncryptFiles {
            files: vec![fresh.clone(), blocked.clone()],
            recipients: vec![Identity::generate().to_public()],
            out_dir: None,
            overwrite: false,
        })
        .unwrap();
    let messages = worker.until_answer();

    let finished: Vec<(PathBuf, FileResult)> = messages
        .iter()
        .filter_map(|message| match message {
            WorkerMsg::Job(JobEvent::FileFinished { path, result }) => {
                Some((path.clone(), result.clone()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(messages.len(), 5);
    assert!(matches!(&finished[0], (path, FileResult::Done(size)) if *path == fresh && *size > 0));
    assert_eq!(finished[1], (blocked, FileResult::Skipped));
    assert!(matches!(
        messages.last(),
        Some(WorkerMsg::Job(JobEvent::Finished(JobOutcome::Files {
            direction: Direction::Encrypt,
            done: 1,
            skipped: 1,
            failed: 0,
        })))
    ));
    assert_eq!(
        fs::read(dir.path().join("blocked.txt.age")).unwrap(),
        b"already here"
    );
}

#[test]
fn file_batch_failures_do_not_stop_the_remaining_files() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing.txt");
    let present = dir.path().join("present.txt");
    fs::write(&present, b"present").unwrap();
    let worker = start();

    worker
        .submit
        .run(JobRequest::EncryptFiles {
            files: vec![missing, present],
            recipients: vec![Identity::generate().to_public()],
            out_dir: None,
            overwrite: false,
        })
        .unwrap();
    let finished: Vec<FileResult> = worker
        .until_answer()
        .into_iter()
        .filter_map(|message| match message {
            WorkerMsg::Job(JobEvent::FileFinished { result, .. }) => Some(result),
            _ => None,
        })
        .collect();

    assert!(matches!(&finished[0], FileResult::Failed(msg) if msg.contains("could not open")));
    assert!(matches!(finished[1], FileResult::Done(_)));
}

#[test]
fn decrypt_batch_round_trips_into_a_chosen_folder() {
    let dir = tempfile::tempdir().unwrap();
    let out_dir = dir.path().join("out");
    fs::create_dir(&out_dir).unwrap();
    let (identity_path, recipients) = identity_fixture(dir.path());
    let plain = dir.path().join("note.txt");
    fs::write(&plain, b"round trip me").unwrap();
    let worker = start();
    worker
        .submit
        .run(JobRequest::EncryptFiles {
            files: vec![plain],
            recipients,
            out_dir: None,
            overwrite: false,
        })
        .unwrap();
    worker.answer();

    worker
        .submit
        .run(JobRequest::DecryptFiles {
            files: vec![dir.path().join("note.txt.age")],
            identity_path,
            out_dir: Some(out_dir.clone()),
            overwrite: false,
        })
        .unwrap();

    assert!(matches!(
        outcome(worker.answer()),
        JobOutcome::Files {
            direction: Direction::Decrypt,
            done: 1,
            skipped: 0,
            failed: 0,
        }
    ));
    assert_eq!(
        fs::read(out_dir.join("note.txt")).unwrap(),
        b"round trip me"
    );
}

#[test]
fn a_bad_identity_file_fails_the_whole_decrypt_batch_before_any_file_event() {
    let dir = tempfile::tempdir().unwrap();
    let worker = start();

    worker
        .submit
        .run(JobRequest::DecryptFiles {
            files: vec![dir.path().join("whatever.age")],
            identity_path: dir.path().join("absent-identity.txt"),
            out_dir: None,
            overwrite: false,
        })
        .unwrap();
    let messages = worker.until_answer();

    assert_eq!(messages.len(), 1);
    let Some(WorkerMsg::Job(event)) = messages.into_iter().next() else {
        panic!("expected one job answer");
    };
    assert!(failure(event).contains("could not read identity file"));
}

#[rstest]
#[case::encrypt_next_to_input(Direction::Encrypt, None, "dir/report.pdf.age")]
#[case::decrypt_next_to_input(Direction::Decrypt, None, "dir/report.pdf")]
#[case::encrypt_into_folder(Direction::Encrypt, Some("safe"), "safe/report.pdf.age")]
#[case::decrypt_into_folder(Direction::Decrypt, Some("safe"), "safe/report.pdf")]
fn resolve_output_path_places_the_derived_name(
    #[case] direction: Direction,
    #[case] out_dir: Option<&str>,
    #[case] expected: &str,
) {
    let input = match direction {
        Direction::Encrypt => Path::new("dir/report.pdf"),
        Direction::Decrypt => Path::new("dir/report.pdf.age"),
    };

    let output = resolve_output_path(input, direction, out_dir.map(Path::new)).unwrap();

    assert_eq!(output, Path::new(expected));
}

#[test]
fn job_kinds_route_requests_and_outcomes_to_their_tab() {
    let identity = Identity::generate();

    assert_eq!(
        JobRequest::EncryptText {
            text: String::new(),
            recipients: vec![identity.to_public()],
        }
        .kind(),
        JobKind::Text
    );
    assert_eq!(
        JobRequest::DecryptFiles {
            files: Vec::new(),
            identity_path: PathBuf::new(),
            out_dir: None,
            overwrite: false,
        }
        .kind(),
        JobKind::Files
    );
    assert_eq!(
        JobRequest::Keygen {
            path: PathBuf::new(),
            replace: false,
        }
        .kind(),
        JobKind::Keygen
    );
    assert_eq!(
        JobOutcome::KeygenExists {
            path: PathBuf::new()
        }
        .kind(),
        JobKind::Keygen
    );
}

#[test]
fn debug_output_carries_lengths_never_text() {
    const MARKER: &str = "marker-31337";
    let identity = Identity::generate();
    let request = JobRequest::EncryptText {
        text: MARKER.to_owned(),
        recipients: vec![identity.to_public()],
    };
    let events = [
        JobEvent::Finished(JobOutcome::Text(MARKER.to_owned())),
        JobEvent::Failed(MARKER.to_owned()),
        JobEvent::FileFinished {
            path: PathBuf::from("a.txt"),
            result: FileResult::Failed(MARKER.to_owned()),
        },
    ];

    let shown = format!("{request:?} {events:?}");

    assert!(!shown.contains(MARKER), "{shown}");
    assert!(shown.contains("text_len: 12"), "{shown}");
}

#[test]
fn panic_text_reads_str_and_string_payloads() {
    let from_str: Box<dyn Any + Send> = Box::new("stopped");
    let from_string: Box<dyn Any + Send> = Box::new(String::from("stopped too"));
    let other: Box<dyn Any + Send> = Box::new(3_u8);

    assert_eq!(panic_text(from_str.as_ref()), "stopped");
    assert_eq!(panic_text(from_string.as_ref()), "stopped too");
    assert_eq!(panic_text(other.as_ref()), "a panic with no text message");
}

use anyhow::Context as _;
use common_file_utils::atomic_write::write_via_temp;
use shared_crypto::{Direction, Recipient, engine, keys, output_name_for};
use std::fs::File;
use std::io::{BufReader, BufWriter, Write as _};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;

/// A unit of background work. Carries everything the job needs, already
/// validated by the submitting view; the worker never reads UI state.
///
/// No `Debug`: [`Recipient`] does not implement it (the age crate keeps its
/// key types out of format machinery), and nothing logs whole requests.
pub enum JobRequest {
    /// Round-trips the full event sequence without doing any real work, so
    /// the bridge tests can prove the pipeline end to end.
    #[cfg(test)]
    Probe,
    /// Seals `text` to `recipients` as ASCII armor.
    EncryptText {
        text: String,
        recipients: Vec<Recipient>,
    },
    /// Unseals armored `text` with the identities in `identity_path`.
    DecryptText {
        text: String,
        identity_path: PathBuf,
    },
    /// Seals every file to `recipients`; one job, per-file events.
    EncryptFiles {
        files: Vec<PathBuf>,
        recipients: Vec<Recipient>,
        out_dir: Option<PathBuf>,
        overwrite: bool,
    },
    /// Unseals every file with the identities in `identity_path`.
    DecryptFiles {
        files: Vec<PathBuf>,
        identity_path: PathBuf,
        out_dir: Option<PathBuf>,
        overwrite: bool,
    },
    /// Generates an identity and saves it to `path`. The secret is created,
    /// written, and dropped inside the worker; only the public key returns.
    Keygen { path: PathBuf },
    #[cfg(test)]
    FailForTests,
    #[cfg(test)]
    PanicForTests,
}

/// Which feature a job belongs to, so failure cleanup can route to the right
/// tab's state without inspecting the request again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobKind {
    #[cfg(test)]
    Probe,
    Text,
    Files,
    Keygen,
}

impl JobRequest {
    pub fn kind(&self) -> JobKind {
        match self {
            Self::EncryptText { .. } | Self::DecryptText { .. } => JobKind::Text,
            Self::EncryptFiles { .. } | Self::DecryptFiles { .. } => JobKind::Files,
            Self::Keygen { .. } => JobKind::Keygen,
            #[cfg(test)]
            Self::Probe | Self::FailForTests | Self::PanicForTests => JobKind::Probe,
        }
    }
}

/// What a finished job produced.
#[derive(Debug)]
pub enum JobOutcome {
    /// The probe round-trip completed.
    #[cfg(test)]
    Probe,
    /// The text operation's result: armored ciphertext or UTF-8 plaintext.
    Text(String),
    /// Unsealed plaintext was not UTF-8; binary content belongs in the Files
    /// tab, not mojibake in a text box.
    TextRedirect,
    /// The identity was saved; only its public half crosses the channel.
    Keygen { public_key: String },
    /// Per-file batch totals, for the summary toast.
    Files {
        direction: Direction,
        done: usize,
        skipped: usize,
        failed: usize,
    },
}

/// How one file of a batch ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileResult {
    /// Written; carries the output file's size in bytes.
    Done(u64),
    Failed(String),
    /// The output already existed and the overwrite gate was off.
    Skipped,
}

/// One message from the worker back to the UI. Every send is followed by a
/// repaint request, because an on-demand renderer never notices state that
/// changed on another thread.
#[derive(Debug)]
pub enum JobEvent {
    Started {
        job_id: u64,
    },
    FileProgress {
        job_id: u64,
        path: PathBuf,
        done: usize,
        total: usize,
    },
    FileFinished {
        job_id: u64,
        path: PathBuf,
        result: FileResult,
    },
    Done {
        job_id: u64,
        outcome: JobOutcome,
    },
    Failed {
        job_id: u64,
        error_text: String,
    },
}

/// Owns the single background worker thread and the two channels bridging it
/// to the UI. One job runs at a time; the UI disables job-starting actions
/// while one is in flight.
pub struct WorkerHandle {
    /// Option so Drop can take and drop the sender, which is what ends the
    /// worker loop.
    requests: Option<mpsc::Sender<JobRequest>>,
    events: mpsc::Receiver<JobEvent>,
    thread: Option<thread::JoinHandle<()>>,
}

impl WorkerHandle {
    /// Spawns the worker thread. The `ctx` clone points at the live UI
    /// context (egui contexts are refcounted), so the worker can wake the
    /// window after each event it sends.
    ///
    /// # Errors
    /// Fails when the OS refuses to spawn the thread.
    pub fn spawn(ctx: egui::Context) -> std::io::Result<Self> {
        let (request_tx, request_rx) = mpsc::channel::<JobRequest>();
        let (event_tx, event_rx) = mpsc::channel::<JobEvent>();
        let thread = thread::Builder::new()
            .name("seal-worker".to_string())
            .spawn(move || worker_loop(&request_rx, &event_tx, &ctx))?;

        Ok(Self {
            requests: Some(request_tx),
            events: event_rx,
            thread: Some(thread),
        })
    }

    /// Queues a job. Returns false when the worker is gone (its channel
    /// closed), so the caller can surface the failure instead of silently
    /// dropping the job.
    pub fn submit(&self, job: JobRequest) -> bool {
        self.requests
            .as_ref()
            .is_some_and(|requests| requests.send(job).is_ok())
    }

    /// Collects up to `budget` pending events without blocking; the bound
    /// keeps a pathological burst from stretching one frame. The second
    /// value is true when the worker side has disconnected (the thread is
    /// gone).
    pub fn drain(&self, budget: usize) -> (Vec<JobEvent>, bool) {
        drain_events(&self.events, budget)
    }
}

impl Drop for WorkerHandle {
    /// Dropping the sender first is what ends the thread: the worker's
    /// `recv` fails and its loop exits, so the join cannot hang.
    fn drop(&mut self) {
        drop(self.requests.take());
        if let Some(thread) = self.thread.take() {
            // A panicked worker already surfaced through the drain's
            // disconnected flag, so the join result carries nothing new.
            let _ = thread.join();
        }
    }
}

fn drain_events(events: &mpsc::Receiver<JobEvent>, budget: usize) -> (Vec<JobEvent>, bool) {
    let mut collected = Vec::new();
    for _ in 0..budget {
        match events.try_recv() {
            Ok(event) => collected.push(event),
            Err(mpsc::TryRecvError::Empty) => return (collected, false),
            Err(mpsc::TryRecvError::Disconnected) => return (collected, true),
        }
    }
    (collected, false)
}

/// Runs jobs until the request channel closes (the handle was dropped).
fn worker_loop(
    requests: &mpsc::Receiver<JobRequest>,
    events: &mpsc::Sender<JobEvent>,
    ctx: &egui::Context,
) {
    let mut job_id: u64 = 0;
    while let Ok(request) = requests.recv() {
        job_id += 1;
        send_event(events, ctx, JobEvent::Started { job_id });
        match run_job(request, job_id, events, ctx) {
            Ok(outcome) => send_event(events, ctx, JobEvent::Done { job_id, outcome }),
            Err(error) => send_event(
                events,
                ctx,
                JobEvent::Failed {
                    job_id,
                    error_text: format!("{error:#}"),
                },
            ),
        }
    }
}

/// Sends one event and wakes the sleeping UI to render it. A failed send
/// means the UI side is shutting down; the loop ends on the next `recv`.
fn send_event(events: &mpsc::Sender<JobEvent>, ctx: &egui::Context, event: JobEvent) {
    if events.send(event).is_ok() {
        ctx.request_repaint();
    }
}

/// Runs one job to completion on the worker thread. Touches egui only
/// through the repaint wake in [`send_event`].
fn run_job(
    request: JobRequest,
    job_id: u64,
    events: &mpsc::Sender<JobEvent>,
    ctx: &egui::Context,
) -> anyhow::Result<JobOutcome> {
    match request {
        #[cfg(test)]
        JobRequest::Probe => {
            send_event(
                events,
                ctx,
                JobEvent::FileProgress {
                    job_id,
                    path: PathBuf::new(),
                    done: 1,
                    total: 1,
                },
            );
            Ok(JobOutcome::Probe)
        }
        JobRequest::EncryptText { text, recipients } => run_encrypt_text(&text, &recipients),
        JobRequest::DecryptText {
            text,
            identity_path,
        } => run_decrypt_text(&text, &identity_path),
        JobRequest::EncryptFiles {
            files,
            recipients,
            out_dir,
            overwrite,
        } => {
            let spec = BatchSpec {
                files,
                direction: Direction::Encrypt,
                out_dir,
                overwrite,
            };
            run_file_batch(&spec, job_id, events, ctx, |input, output| {
                engine::encrypt(&recipients, input, output, false)
            })
        }
        JobRequest::DecryptFiles {
            files,
            identity_path,
            out_dir,
            overwrite,
        } => {
            // A bad identity file fails the whole job before any file event,
            // so no row is left mid-flight.
            let identities = keys::load_identities(&[identity_path])?;
            let spec = BatchSpec {
                files,
                direction: Direction::Decrypt,
                out_dir,
                overwrite,
            };
            run_file_batch(&spec, job_id, events, ctx, |input, output| {
                engine::decrypt(&identities, input, output)
            })
        }
        JobRequest::Keygen { path } => run_keygen(&path),
        #[cfg(test)]
        JobRequest::FailForTests => anyhow::bail!("this test job always fails"),
        #[cfg(test)]
        JobRequest::PanicForTests => panic!("this test job always panics"),
    }
}

fn run_encrypt_text(text: &str, recipients: &[Recipient]) -> anyhow::Result<JobOutcome> {
    let mut input = text.as_bytes();
    let mut armored = Vec::new();
    engine::encrypt(recipients, &mut input, &mut armored, true)?;
    // Armor is ASCII by construction; a non-UTF-8 result would be an engine
    // bug, reported instead of unwrapped.
    let armored = String::from_utf8(armored).context("armored output was not valid UTF-8")?;
    Ok(JobOutcome::Text(armored))
}

fn run_decrypt_text(text: &str, identity_path: &Path) -> anyhow::Result<JobOutcome> {
    let identities = keys::load_identities(&[identity_path.to_path_buf()])?;
    // Pasted armor routinely carries surrounding whitespace; the armor
    // reader wants the BEGIN marker at the start.
    let mut input = text.trim().as_bytes();
    let mut plaintext = Vec::new();
    engine::decrypt(&identities, &mut input, &mut plaintext)?;
    Ok(text_or_redirect(plaintext))
}

/// The unseal-to-text decision: UTF-8 plaintext goes to the output box,
/// anything else is binary and belongs in the Files tab.
pub fn text_or_redirect(plaintext: Vec<u8>) -> JobOutcome {
    match String::from_utf8(plaintext) {
        Ok(text) => JobOutcome::Text(text),
        Err(_) => JobOutcome::TextRedirect,
    }
}

fn run_keygen(path: &Path) -> anyhow::Result<JobOutcome> {
    let identity = keys::generate_identity();
    // The save dialog's own replace-existing confirmation already guarded
    // overwriting, so the save is unconditional.
    keys::save_identity_file(&identity, path, true)?;
    Ok(JobOutcome::Keygen {
        public_key: keys::public_key_of(&identity),
    })
}

/// Everything a file batch shares across its rows.
struct BatchSpec {
    files: Vec<PathBuf>,
    direction: Direction,
    out_dir: Option<PathBuf>,
    overwrite: bool,
}

/// Runs every file of the batch, emitting a progress and a finished event
/// per file. One file's failure never aborts the rest; the totals land in
/// the outcome for the summary toast.
fn run_file_batch(
    spec: &BatchSpec,
    job_id: u64,
    events: &mpsc::Sender<JobEvent>,
    ctx: &egui::Context,
    op: impl Fn(&mut BufReader<File>, &mut BufWriter<File>) -> anyhow::Result<u64>,
) -> anyhow::Result<JobOutcome> {
    let total = spec.files.len();
    let (mut done, mut skipped, mut failed) = (0usize, 0usize, 0usize);

    for (index, path) in spec.files.iter().enumerate() {
        send_event(
            events,
            ctx,
            JobEvent::FileProgress {
                job_id,
                path: path.clone(),
                done: index,
                total,
            },
        );

        let result = match process_file(path, spec, &op) {
            Ok(Some(size)) => {
                done += 1;
                FileResult::Done(size)
            }
            Ok(None) => {
                skipped += 1;
                FileResult::Skipped
            }
            Err(error) => {
                failed += 1;
                FileResult::Failed(format!("{error:#}"))
            }
        };
        send_event(
            events,
            ctx,
            JobEvent::FileFinished {
                job_id,
                path: path.clone(),
                result,
            },
        );
    }

    Ok(JobOutcome::Files {
        direction: spec.direction,
        done,
        skipped,
        failed,
    })
}

/// Processes one file through a temp-file-plus-rename write. `Ok(None)` is
/// the overwrite-gated skip; `Ok(Some(size))` carries the written output's
/// size.
fn process_file(
    path: &Path,
    spec: &BatchSpec,
    op: impl Fn(&mut BufReader<File>, &mut BufWriter<File>) -> anyhow::Result<u64>,
) -> anyhow::Result<Option<u64>> {
    let output = resolve_output_path(path, spec.direction, spec.out_dir.as_deref())?;
    if !spec.overwrite && output.exists() {
        return Ok(None);
    }

    write_via_temp(&output, |temp| {
        let file = File::open(path)
            .with_context(|| format!("could not open input file {}", path.display()))?;
        let mut reader = BufReader::new(file);
        let created = File::create(temp)
            .with_context(|| format!("could not create output file {}", temp.display()))?;
        let mut writer = BufWriter::new(created);
        op(&mut reader, &mut writer)?;
        writer.flush().context("failed to flush the output file")
    })?;

    let size = std::fs::metadata(&output)
        .with_context(|| format!("could not stat output file {}", output.display()))?
        .len();
    Ok(Some(size))
}

/// Where a batch file's output lands: the shared naming rule next to the
/// input, or the derived file name inside the chosen folder.
pub fn resolve_output_path(
    input: &Path,
    direction: Direction,
    out_dir: Option<&Path>,
) -> anyhow::Result<PathBuf> {
    let derived = output_name_for(input, direction)?;
    match out_dir {
        None => Ok(derived),
        Some(dir) => {
            // The derived path was built from a file name, so this misses
            // only if the naming rule itself changes shape.
            let name = derived
                .file_name()
                .context("derived output path has no file name")?;
            Ok(dir.join(name))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use shared_crypto::Identity;
    use std::fs;
    use std::time::{Duration, Instant};

    fn drain_until(handle: &WorkerHandle, expected: usize) -> Vec<JobEvent> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut events = Vec::new();
        while events.len() < expected && Instant::now() < deadline {
            let (mut batch, _) = handle.drain(16);
            events.append(&mut batch);
            thread::sleep(Duration::from_millis(2));
        }
        events
    }

    fn spawn_worker() -> WorkerHandle {
        WorkerHandle::spawn(egui::Context::default()).unwrap()
    }

    /// Writes a fresh identity file and returns its path plus the matching
    /// recipient set.
    fn identity_fixture(dir: &Path) -> (PathBuf, Vec<Recipient>) {
        let identity = keys::generate_identity();
        let path = dir.join("identity.txt");
        keys::save_identity_file(&identity, &path, false).unwrap();
        (path, vec![identity.to_public()])
    }

    fn done_outcome(event: JobEvent) -> JobOutcome {
        match event {
            JobEvent::Done { outcome, .. } => outcome,
            other => panic!("expected Done, got {other:?}"),
        }
    }

    fn failed_text(event: JobEvent) -> String {
        match event {
            JobEvent::Failed { error_text, .. } => error_text,
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn probe_round_trips_started_progress_and_done() {
        let handle = spawn_worker();

        assert!(handle.submit(JobRequest::Probe));
        let events = drain_until(&handle, 3);

        assert_eq!(events.len(), 3);
        assert!(matches!(events[0], JobEvent::Started { job_id: 1 }));
        assert!(matches!(
            events[1],
            JobEvent::FileProgress {
                job_id: 1,
                done: 1,
                total: 1,
                ..
            }
        ));
        assert!(matches!(
            events[2],
            JobEvent::Done {
                job_id: 1,
                outcome: JobOutcome::Probe
            }
        ));
    }

    #[test]
    fn encrypt_then_decrypt_text_round_trips_through_the_worker() {
        let dir = tempfile::tempdir().unwrap();
        let (identity_path, recipients) = identity_fixture(dir.path());
        let handle = spawn_worker();

        assert!(handle.submit(JobRequest::EncryptText {
            text: "a secret note".to_string(),
            recipients,
        }));
        let armored = match done_outcome(drain_until(&handle, 2).remove(1)) {
            JobOutcome::Text(text) => text,
            other => panic!("expected armored text, got {other:?}"),
        };
        assert!(armored.starts_with("-----BEGIN AGE ENCRYPTED FILE-----"));

        // Surrounding whitespace mimics a clipboard paste.
        assert!(handle.submit(JobRequest::DecryptText {
            text: format!("\n  {armored}\n\n"),
            identity_path,
        }));
        let outcome = done_outcome(drain_until(&handle, 2).remove(1));
        assert!(matches!(outcome, JobOutcome::Text(text) if text == "a secret note"));
    }

    #[test]
    fn tampered_armor_fails_with_the_corruption_wording() {
        let dir = tempfile::tempdir().unwrap();
        let (identity_path, recipients) = identity_fixture(dir.path());
        let handle = spawn_worker();

        assert!(handle.submit(JobRequest::EncryptText {
            text: "payload long enough to have a body line".to_string(),
            recipients,
        }));
        let armored = match done_outcome(drain_until(&handle, 2).remove(1)) {
            JobOutcome::Text(text) => text,
            other => panic!("expected armored text, got {other:?}"),
        };

        // Swap one base64 character near the end of the body: still valid
        // armor, but the final payload chunk no longer authenticates.
        let end_marker = armored.rfind("-----END").unwrap();
        let target = end_marker - 10;
        let original = armored.as_bytes()[target];
        let swapped = if original == b'A' { b'B' } else { b'A' };
        let mut bytes = armored.into_bytes();
        bytes[target] = swapped;
        let tampered = String::from_utf8(bytes).unwrap();

        assert!(handle.submit(JobRequest::DecryptText {
            text: tampered,
            identity_path,
        }));
        let error = failed_text(drain_until(&handle, 2).remove(1));
        assert!(error.contains("damaged"), "unexpected wording: {error}");
    }

    #[test]
    fn passphrase_armor_fails_with_the_refusal_wording() {
        // The armored, passphrase-encrypted (scrypt) fixture shared with the
        // CLI's tests; seal refuses passphrase files by policy.
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
        let handle = spawn_worker();

        assert!(handle.submit(JobRequest::DecryptText {
            text: SCRYPT_ARMOR.to_string(),
            identity_path,
        }));
        let error = failed_text(drain_until(&handle, 2).remove(1));
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
    fn keygen_saves_the_identity_and_returns_only_the_public_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new-identity.txt");
        let handle = spawn_worker();

        assert!(handle.submit(JobRequest::Keygen { path: path.clone() }));
        let outcome = done_outcome(drain_until(&handle, 2).remove(1));

        let JobOutcome::Keygen { public_key } = outcome else {
            panic!("expected a keygen outcome, got {outcome:?}");
        };
        assert!(public_key.starts_with("age1"));
        let loaded = keys::load_identities(&[path]).unwrap();
        assert_eq!(keys::public_key_of(&loaded[0]), public_key);
    }

    #[test]
    fn file_batch_skips_existing_outputs_when_the_gate_is_off_and_seals_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let identity = keys::generate_identity();
        let fresh = dir.path().join("fresh.txt");
        let blocked = dir.path().join("blocked.txt");
        fs::write(&fresh, b"fresh contents").unwrap();
        fs::write(&blocked, b"blocked contents").unwrap();
        fs::write(dir.path().join("blocked.txt.age"), b"already here").unwrap();
        let handle = spawn_worker();

        assert!(handle.submit(JobRequest::EncryptFiles {
            files: vec![fresh.clone(), blocked.clone()],
            recipients: vec![identity.to_public()],
            out_dir: None,
            overwrite: false,
        }));
        // Started + 2 * (progress + finished) + Done.
        let events = drain_until(&handle, 6);
        assert_eq!(events.len(), 6);

        let results: Vec<(PathBuf, FileResult)> = events
            .into_iter()
            .filter_map(|event| match event {
                JobEvent::FileFinished { path, result, .. } => Some((path, result)),
                JobEvent::Done { outcome, .. } => {
                    assert!(matches!(
                        outcome,
                        JobOutcome::Files {
                            direction: Direction::Encrypt,
                            done: 1,
                            skipped: 1,
                            failed: 0,
                        }
                    ));
                    None
                }
                _ => None,
            })
            .collect();

        assert!(
            matches!(&results[0], (path, FileResult::Done(size)) if *path == fresh && *size > 0)
        );
        assert_eq!(results[1], (blocked, FileResult::Skipped));
        // The pre-existing output was not touched.
        assert_eq!(
            fs::read(dir.path().join("blocked.txt.age")).unwrap(),
            b"already here"
        );
        assert!(fresh.with_file_name("fresh.txt.age").exists());
    }

    #[test]
    fn file_batch_failures_do_not_abort_the_remaining_files() {
        let dir = tempfile::tempdir().unwrap();
        let identity = keys::generate_identity();
        let missing = dir.path().join("missing.txt");
        let present = dir.path().join("present.txt");
        fs::write(&present, b"present").unwrap();
        let handle = spawn_worker();

        assert!(handle.submit(JobRequest::EncryptFiles {
            files: vec![missing.clone(), present.clone()],
            recipients: vec![identity.to_public()],
            out_dir: None,
            overwrite: false,
        }));
        let events = drain_until(&handle, 6);
        assert_eq!(events.len(), 6);

        let finished: Vec<FileResult> = events
            .iter()
            .filter_map(|event| match event {
                JobEvent::FileFinished { result, .. } => Some(result.clone()),
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
        let handle = spawn_worker();

        assert!(handle.submit(JobRequest::EncryptFiles {
            files: vec![plain.clone()],
            recipients,
            out_dir: None,
            overwrite: false,
        }));
        drain_until(&handle, 4);

        let sealed = dir.path().join("note.txt.age");
        assert!(handle.submit(JobRequest::DecryptFiles {
            files: vec![sealed],
            identity_path,
            out_dir: Some(out_dir.clone()),
            overwrite: false,
        }));
        let events = drain_until(&handle, 4);

        assert!(matches!(
            events.last(),
            Some(JobEvent::Done {
                outcome: JobOutcome::Files {
                    direction: Direction::Decrypt,
                    done: 1,
                    skipped: 0,
                    failed: 0,
                },
                ..
            })
        ));
        assert_eq!(
            fs::read(out_dir.join("note.txt")).unwrap(),
            b"round trip me"
        );
    }

    #[test]
    fn a_bad_identity_file_fails_the_whole_decrypt_batch_before_any_file_event() {
        let dir = tempfile::tempdir().unwrap();
        let identity_path = dir.path().join("absent-identity.txt");
        let handle = spawn_worker();

        assert!(handle.submit(JobRequest::DecryptFiles {
            files: vec![dir.path().join("whatever.age")],
            identity_path,
            out_dir: None,
            overwrite: false,
        }));
        let events = drain_until(&handle, 2);

        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], JobEvent::Started { .. }));
        let error = failed_text(events.into_iter().nth(1).unwrap());
        assert!(error.contains("could not read identity file"));
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
    fn job_kinds_route_requests_to_their_feature() {
        let identity = Identity::generate();
        assert_eq!(JobRequest::Probe.kind(), JobKind::Probe);
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
                path: PathBuf::new()
            }
            .kind(),
            JobKind::Keygen
        );
    }

    #[test]
    fn drain_events_respects_its_budget() {
        let (sender, receiver) = mpsc::channel::<JobEvent>();
        for job_id in 1..=10 {
            sender.send(JobEvent::Started { job_id }).unwrap();
        }

        let (first, disconnected_first) = drain_events(&receiver, 4);
        let (rest, disconnected_rest) = drain_events(&receiver, 16);

        assert_eq!(first.len(), 4);
        assert!(!disconnected_first);
        assert_eq!(rest.len(), 6);
        assert!(!disconnected_rest);
    }

    #[test]
    fn drain_events_returns_pending_events_and_the_disconnect_together() {
        let (sender, receiver) = mpsc::channel::<JobEvent>();
        sender.send(JobEvent::Started { job_id: 1 }).unwrap();
        sender.send(JobEvent::Started { job_id: 2 }).unwrap();
        drop(sender);

        let (events, disconnected) = drain_events(&receiver, 16);

        assert_eq!(events.len(), 2);
        assert!(disconnected);
    }

    #[test]
    fn dropping_the_handle_ends_the_worker_without_hanging() {
        let handle = spawn_worker();
        let (done_tx, done_rx) = mpsc::channel::<()>();

        thread::spawn(move || {
            drop(handle);
            let _ = done_tx.send(());
        });

        assert!(done_rx.recv_timeout(Duration::from_secs(5)).is_ok());
    }

    #[test]
    fn a_failing_job_emits_failed_with_the_rendered_error() {
        let handle = spawn_worker();

        assert!(handle.submit(JobRequest::FailForTests));
        let events = drain_until(&handle, 2);

        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], JobEvent::Started { job_id: 1 }));
        let JobEvent::Failed { job_id, error_text } = &events[1] else {
            panic!("expected the failed event, got {:?}", events[1]);
        };
        assert_eq!(*job_id, 1);
        assert!(error_text.contains("always fails"));
    }

    #[test]
    fn a_panicking_job_surfaces_as_the_disconnected_flag() {
        let handle = spawn_worker();

        assert!(handle.submit(JobRequest::PanicForTests));

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut disconnected = false;
        while !disconnected && Instant::now() < deadline {
            (_, disconnected) = handle.drain(16);
            thread::sleep(Duration::from_millis(2));
        }
        assert!(disconnected);
    }
}

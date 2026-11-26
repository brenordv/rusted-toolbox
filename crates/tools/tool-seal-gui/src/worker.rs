//! The background worker: one thread that runs one crypto job at a time and reports every
//! event through a `post` callback. It never reads window state; a job carries everything it
//! needs.

use anyhow::{Context as _, anyhow};
use common_file_utils::atomic_write::write_via_temp;
use seal_crypto::{Direction, Recipient, engine, keys, output_name_for};
use std::any::Any;
use std::fmt;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write as _};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::Instant;
use tracing::{debug, error, info, warn};

/// What [`WorkerSubmit::run`] answers once the worker thread is gone.
pub const WORKER_GONE: &str = "The background worker has stopped. Restart seal-gui to run jobs.";

/// A unit of background work, already validated; the worker never reads window state.
pub enum JobRequest {
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
    /// Seals every file to `recipients`; one job, events per file.
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
    /// Generates an identity and saves it to `path`. With `replace` false the file is created
    /// only if none is there; an existing one answers [`JobOutcome::KeygenExists`] and is left
    /// as it was. The secret never leaves the worker.
    Keygen { path: PathBuf, replace: bool },
    #[cfg(test)]
    FailForTests,
    #[cfg(test)]
    PanicForTests,
    /// Says it started on `started`, then holds the worker until `release` sends or hangs up.
    #[cfg(test)]
    HoldForTests {
        started: mpsc::Sender<()>,
        release: mpsc::Receiver<()>,
    },
}

impl fmt::Debug for JobRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EncryptText { text, recipients } => f
                .debug_struct("EncryptText")
                .field("text_len", &text.len())
                .field("recipients", &recipients.len())
                .finish(),
            Self::DecryptText {
                text,
                identity_path,
            } => f
                .debug_struct("DecryptText")
                .field("text_len", &text.len())
                .field("identity_path", identity_path)
                .finish(),
            Self::EncryptFiles {
                files,
                recipients,
                out_dir,
                overwrite,
            } => f
                .debug_struct("EncryptFiles")
                .field("files", &files.len())
                .field("recipients", &recipients.len())
                .field("out_dir", out_dir)
                .field("overwrite", overwrite)
                .finish(),
            Self::DecryptFiles {
                files,
                identity_path,
                out_dir,
                overwrite,
            } => f
                .debug_struct("DecryptFiles")
                .field("files", &files.len())
                .field("identity_path", identity_path)
                .field("out_dir", out_dir)
                .field("overwrite", overwrite)
                .finish(),
            Self::Keygen { path, replace } => f
                .debug_struct("Keygen")
                .field("path", path)
                .field("replace", replace)
                .finish(),
            #[cfg(test)]
            Self::FailForTests => f.write_str("FailForTests"),
            #[cfg(test)]
            Self::PanicForTests => f.write_str("PanicForTests"),
            #[cfg(test)]
            Self::HoldForTests { .. } => f.write_str("HoldForTests"),
        }
    }
}

/// Which tab a job belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobKind {
    Text,
    Files,
    Keygen,
}

impl JobKind {
    /// The kind's name in a log line.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Files => "files",
            Self::Keygen => "keygen",
        }
    }
}

impl JobRequest {
    /// The tab the job belongs to.
    pub fn kind(&self) -> JobKind {
        match self {
            Self::EncryptText { .. } | Self::DecryptText { .. } => JobKind::Text,
            Self::EncryptFiles { .. } | Self::DecryptFiles { .. } => JobKind::Files,
            Self::Keygen { .. } => JobKind::Keygen,
            #[cfg(test)]
            Self::FailForTests | Self::PanicForTests | Self::HoldForTests { .. } => JobKind::Text,
        }
    }

    /// How many files the job works through; 0 for a text or keygen job.
    pub fn file_count(&self) -> usize {
        match self {
            Self::EncryptFiles { files, .. } | Self::DecryptFiles { files, .. } => files.len(),
            Self::EncryptText { .. } | Self::DecryptText { .. } | Self::Keygen { .. } => 0,
            #[cfg(test)]
            Self::FailForTests | Self::PanicForTests | Self::HoldForTests { .. } => 0,
        }
    }
}

/// What a finished job produced.
pub enum JobOutcome {
    /// The text operation's result: armored ciphertext or UTF-8 plaintext.
    Text(String),
    /// Unsealed plaintext that is not UTF-8; binary content belongs in the Files tab.
    TextRedirect,
    /// The identity was saved at `path`; only its public half leaves the worker.
    Keygen { public_key: String, path: PathBuf },
    /// `path` already held a file and the request did not say to replace it; nothing was
    /// written.
    KeygenExists { path: PathBuf },
    /// A file batch's totals.
    Files {
        direction: Direction,
        done: usize,
        skipped: usize,
        failed: usize,
    },
}

impl JobOutcome {
    /// The tab the outcome answers.
    pub fn kind(&self) -> JobKind {
        match self {
            Self::Text(_) | Self::TextRedirect => JobKind::Text,
            Self::Keygen { .. } | Self::KeygenExists { .. } => JobKind::Keygen,
            Self::Files { .. } => JobKind::Files,
        }
    }

    /// `(done, skipped, failed)` for the job-finished log line.
    fn counts(&self) -> (usize, usize, usize) {
        match self {
            Self::Text(_) | Self::TextRedirect | Self::Keygen { .. } => (1, 0, 0),
            Self::KeygenExists { .. } => (0, 1, 0),
            Self::Files {
                done,
                skipped,
                failed,
                ..
            } => (*done, *skipped, *failed),
        }
    }
}

impl fmt::Debug for JobOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text(text) => f.debug_struct("Text").field("len", &text.len()).finish(),
            Self::TextRedirect => f.write_str("TextRedirect"),
            Self::Keygen { path, .. } => f.debug_struct("Keygen").field("path", path).finish(),
            Self::KeygenExists { path } => {
                f.debug_struct("KeygenExists").field("path", path).finish()
            }
            Self::Files {
                direction,
                done,
                skipped,
                failed,
            } => f
                .debug_struct("Files")
                .field("direction", direction)
                .field("done", done)
                .field("skipped", skipped)
                .field("failed", failed)
                .finish(),
        }
    }
}

/// How one file of a batch ended.
#[derive(Clone, PartialEq, Eq)]
pub enum FileResult {
    /// Written; the output file's size in bytes.
    Done(u64),
    /// The failure's text.
    Failed(String),
    /// The output already existed and overwriting was off.
    Skipped,
}

impl fmt::Debug for FileResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Done(size) => f.debug_tuple("Done").field(size).finish(),
            Self::Failed(text) => f.debug_struct("Failed").field("len", &text.len()).finish(),
            Self::Skipped => f.write_str("Skipped"),
        }
    }
}

/// One event of a running job.
pub enum JobEvent {
    /// The batch started on `path`.
    FileProgress { path: PathBuf },
    /// The batch finished `path`.
    FileFinished { path: PathBuf, result: FileResult },
    /// The job's one final answer when it worked.
    Finished(JobOutcome),
    /// The job's one final answer when it failed: the error chain's text.
    Failed(String),
}

impl fmt::Debug for JobEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FileProgress { path } => {
                f.debug_struct("FileProgress").field("path", path).finish()
            }
            Self::FileFinished { path, result } => f
                .debug_struct("FileFinished")
                .field("path", path)
                .field("result", result)
                .finish(),
            Self::Finished(outcome) => f.debug_tuple("Finished").field(outcome).finish(),
            Self::Failed(text) => f.debug_struct("Failed").field("len", &text.len()).finish(),
        }
    }
}

/// What the worker thread posts.
#[derive(Debug)]
pub enum WorkerMsg {
    /// An event of the running job.
    Job(JobEvent),
    /// The thread ended without being asked to; no job can run again.
    Stopped,
}

enum Request {
    Job(JobRequest),
    Stop,
}

/// The half of the worker a window keeps to start jobs; cheap to clone.
#[derive(Clone)]
pub struct WorkerSubmit {
    requests: mpsc::Sender<Request>,
}

impl WorkerSubmit {
    /// Queues `request` behind any running job.
    ///
    /// # Errors
    /// [`WORKER_GONE`] when the worker thread has ended.
    pub fn run(&self, request: JobRequest) -> anyhow::Result<()> {
        self.requests
            .send(Request::Job(request))
            .map_err(|_| anyhow!(WORKER_GONE))
    }
}

/// The half of the worker the entry point keeps, to wait for it at the end.
pub struct WorkerJoin {
    requests: mpsc::Sender<Request>,
    running: Arc<Mutex<Option<JobKind>>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl WorkerJoin {
    /// The kind of job the worker is running now, if any.
    pub fn running(&self) -> Option<JobKind> {
        *self.running.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Asks the worker to stop once the running job, if any, ends, and waits for the thread.
    /// A worker that already ended takes no request; that is expected and the wait still
    /// happens. A thread that ended by panicking is logged.
    pub fn finish(mut self) {
        if self.requests.send(Request::Stop).is_err() {
            debug!("the worker had already stopped");
        }
        if let Some(thread) = self.thread.take()
            && let Err(payload) = thread.join()
        {
            error!(error = %panic_text(payload.as_ref()), "the worker could not be joined");
        }
    }
}

/// The text a thread's panic payload carries, when it carries text.
fn panic_text(payload: &(dyn Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic with no text message".to_owned())
}

/// Starts the worker thread, which hands every event to `post`.
///
/// # Errors
/// When the OS refuses to start the thread.
pub fn spawn(
    post: impl Fn(WorkerMsg) + Send + 'static,
) -> std::io::Result<(WorkerSubmit, WorkerJoin)> {
    let (requests, receiver) = mpsc::channel::<Request>();
    let running = Arc::new(Mutex::new(None));
    let worker_running = Arc::clone(&running);
    let thread = thread::Builder::new()
        .name("seal-worker".to_owned())
        .spawn(move || worker_loop(&receiver, &post, &worker_running))?;
    Ok((
        WorkerSubmit {
            requests: requests.clone(),
        },
        WorkerJoin {
            requests,
            running,
            thread: Some(thread),
        },
    ))
}

/// Reports a worker thread that ends without a `Stop`: from a panic in a job, or with every
/// sender gone. Clears the running job, logs the stop with the kind that was running, and
/// posts [`WorkerMsg::Stopped`].
struct StopGuard<'a> {
    running: &'a Mutex<Option<JobKind>>,
    post: &'a dyn Fn(WorkerMsg),
    asked_to_stop: bool,
}

impl Drop for StopGuard<'_> {
    fn drop(&mut self) {
        if !self.asked_to_stop {
            report_stop(self.running, self.post);
        }
    }
}

/// What the worker does when its thread ends without being asked to: clears the running job,
/// logs the stop at `error` with the kind of job that was running, and posts
/// [`WorkerMsg::Stopped`].
pub fn report_stop(running: &Mutex<Option<JobKind>>, post: &dyn Fn(WorkerMsg)) {
    let kind = running
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take();
    error!(
        kind = kind.map(JobKind::as_str),
        "the worker thread stopped without being asked to"
    );
    post(WorkerMsg::Stopped);
}

fn worker_loop(
    requests: &mpsc::Receiver<Request>,
    post: &dyn Fn(WorkerMsg),
    running: &Mutex<Option<JobKind>>,
) {
    let mut guard = StopGuard {
        running,
        post,
        asked_to_stop: false,
    };
    while let Ok(request) = requests.recv() {
        match request {
            Request::Stop => {
                guard.asked_to_stop = true;
                return;
            }
            Request::Job(job) => run_one(job, post, running),
        }
    }
}

/// Runs `job` and posts its one final answer, logging its start and its end first.
fn run_one(job: JobRequest, post: &dyn Fn(WorkerMsg), running: &Mutex<Option<JobKind>>) {
    let kind = job.kind();
    *running.lock().unwrap_or_else(PoisonError::into_inner) = Some(kind);
    log_job_started(kind, job.file_count());
    let started = Instant::now();
    let answer = match run_job(job, post) {
        Ok(outcome) => {
            log_job_finished(kind, &outcome, elapsed_ms(started));
            JobEvent::Finished(outcome)
        }
        Err(error) => {
            let text = format!("{error:#}");
            log_job_failed(kind, &text);
            JobEvent::Failed(text)
        }
    };
    *running.lock().unwrap_or_else(PoisonError::into_inner) = None;
    post(WorkerMsg::Job(answer));
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// The job-started log line.
pub fn log_job_started(kind: JobKind, files: usize) {
    info!(kind = kind.as_str(), files, "job started");
}

/// The job-finished log line, with the outcome's counts.
pub fn log_job_finished(kind: JobKind, outcome: &JobOutcome, elapsed_ms: u64) {
    let (done, skipped, failed) = outcome.counts();
    info!(
        kind = kind.as_str(),
        done, failed, skipped, elapsed_ms, "job finished"
    );
}

/// The job-failed log line, with the error chain.
pub fn log_job_failed(kind: JobKind, error: &str) {
    error!(kind = kind.as_str(), error = %error, "job failed");
}

/// The line for one file of a batch that failed.
pub fn log_file_failed(path: &Path, error: &str) {
    warn!(path = ?path, error = %error, "a file in the batch failed");
}

fn run_job(request: JobRequest, post: &dyn Fn(WorkerMsg)) -> anyhow::Result<JobOutcome> {
    match request {
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
            Ok(run_file_batch(&spec, post, |input, output| {
                engine::encrypt(&recipients, input, output, false)
            }))
        }
        JobRequest::DecryptFiles {
            files,
            identity_path,
            out_dir,
            overwrite,
        } => {
            // A bad identity file fails the whole job before any file event, so no row is left
            // mid-flight.
            let identities = keys::load_identities(&[identity_path])?;
            let spec = BatchSpec {
                files,
                direction: Direction::Decrypt,
                out_dir,
                overwrite,
            };
            Ok(run_file_batch(&spec, post, |input, output| {
                engine::decrypt(&identities, input, output)
            }))
        }
        JobRequest::Keygen { path, replace } => run_keygen(&path, replace),
        #[cfg(test)]
        JobRequest::FailForTests => anyhow::bail!("this test job always fails"),
        #[cfg(test)]
        JobRequest::PanicForTests => panic!("this test job always panics"),
        #[cfg(test)]
        JobRequest::HoldForTests { started, release } => {
            // The test may have stopped listening; either a send or a hang-up releases the job.
            let _told = started.send(());
            let _released = release.recv();
            Ok(JobOutcome::TextRedirect)
        }
    }
}

fn run_encrypt_text(text: &str, recipients: &[Recipient]) -> anyhow::Result<JobOutcome> {
    let mut input = text.as_bytes();
    let mut armored = Vec::new();
    engine::encrypt(recipients, &mut input, &mut armored, true)?;
    // Armor is ASCII by construction; a non-UTF-8 result would be an engine bug, reported
    // instead of unwrapped.
    let armored = String::from_utf8(armored).context("armored output was not valid UTF-8")?;
    Ok(JobOutcome::Text(armored))
}

fn run_decrypt_text(text: &str, identity_path: &Path) -> anyhow::Result<JobOutcome> {
    let identities = keys::load_identities(&[identity_path.to_path_buf()])?;
    // Pasted armor routinely carries surrounding whitespace; the armor reader wants the BEGIN
    // marker at the start.
    let mut input = text.trim().as_bytes();
    let mut plaintext = Vec::new();
    engine::decrypt(&identities, &mut input, &mut plaintext)?;
    Ok(text_or_redirect(plaintext))
}

/// The unseal-to-text decision: UTF-8 plaintext goes to the output box, anything else is
/// binary and belongs in the Files tab.
pub fn text_or_redirect(plaintext: Vec<u8>) -> JobOutcome {
    match String::from_utf8(plaintext) {
        Ok(text) => JobOutcome::Text(text),
        Err(_) => JobOutcome::TextRedirect,
    }
}

fn run_keygen(path: &Path, replace: bool) -> anyhow::Result<JobOutcome> {
    let identity = keys::generate_identity();
    let public_key = keys::public_key_of(&identity);
    if replace {
        keys::save_identity_file(&identity, path, true)?;
    } else if !keys::create_identity_file(&identity, path)? {
        return Ok(JobOutcome::KeygenExists {
            path: path.to_path_buf(),
        });
    }
    Ok(JobOutcome::Keygen {
        public_key,
        path: path.to_path_buf(),
    })
}

/// Everything a file batch shares across its rows.
struct BatchSpec {
    files: Vec<PathBuf>,
    direction: Direction,
    out_dir: Option<PathBuf>,
    overwrite: bool,
}

/// Runs every file of the batch, posting a progress and a finished event per file. One file's
/// failure never stops the rest; the totals land in the outcome.
fn run_file_batch(
    spec: &BatchSpec,
    post: &dyn Fn(WorkerMsg),
    op: impl Fn(&mut BufReader<File>, &mut BufWriter<File>) -> anyhow::Result<u64>,
) -> JobOutcome {
    let (mut done, mut skipped, mut failed) = (0usize, 0usize, 0usize);
    for path in &spec.files {
        post(WorkerMsg::Job(JobEvent::FileProgress {
            path: path.clone(),
        }));
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
                let text = format!("{error:#}");
                log_file_failed(path, &text);
                FileResult::Failed(text)
            }
        };
        post(WorkerMsg::Job(JobEvent::FileFinished {
            path: path.clone(),
            result,
        }));
    }
    JobOutcome::Files {
        direction: spec.direction,
        done,
        skipped,
        failed,
    }
}

/// Processes one file through a temp-file-plus-rename write. `Ok(None)` is the skip when the
/// output exists and overwriting is off; `Ok(Some(size))` carries the written output's size.
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

/// Where a batch file's output lands: the shared naming rule next to the input, or the derived
/// file name inside the chosen folder.
///
/// # Errors
/// When the naming rule cannot derive a name for `input` in `direction`.
pub fn resolve_output_path(
    input: &Path,
    direction: Direction,
    out_dir: Option<&Path>,
) -> anyhow::Result<PathBuf> {
    let derived = output_name_for(input, direction)?;
    match out_dir {
        None => Ok(derived),
        Some(dir) => {
            // The derived path was built from a file name, so this misses only if the naming
            // rule itself changes shape.
            let name = derived
                .file_name()
                .context("derived output path has no file name")?;
            Ok(dir.join(name))
        }
    }
}

#[cfg(test)]
mod tests;

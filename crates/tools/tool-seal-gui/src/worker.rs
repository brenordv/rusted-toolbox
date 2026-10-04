use std::path::PathBuf;
use std::sync::mpsc;

pub enum JobRequest { /* phase 4 fills: EncryptText, DecryptText, EncryptFiles,
                        DecryptFiles, Keygen */ }

pub enum JobOutcome {
    /* also filled in phase4. */
}
pub enum JobEvent {
    Started      { job_id: u64 },
    FileProgress { job_id: u64, path: PathBuf, done: usize, total: usize },
    Done         { job_id: u64, outcome: JobOutcome },
    Failed       { job_id: u64, error_text: String },
}

pub struct WorkerHandle {
    requests: Option<mpsc::Sender<JobRequest>>, // Option so Drop can take + drop it
    events: mpsc::Receiver<JobEvent>,
    thread: Option<std::thread::JoinHandle<()>>,
    ctx: egui::Context,
}

impl WorkerHandle {
    pub fn spawn(ctx: egui::Context) -> Self {
        let (req_tx, req_rx) = mpsc::channel::<JobRequest>();
        let (evt_tx, evt_rx) = mpsc::channel::<JobEvent>();
        let worker_ctx = ctx.clone();                  // same Context, refcounted
        let thread = std::thread::Builder::new()
            .name("seal-worker".into())
            .spawn(move || {
                while let Ok(req) = req_rx.recv() {    // Err = sender dropped = shutdown
                    run_job(req, &evt_tx, &worker_ctx); // sends events, repaints after each
                }
            })
            .expect("spawning the worker thread");     // at boot, before any job
        Self { requests: Some(req_tx), events: evt_rx, thread: Some(thread), ctx }
    }

    pub fn submit(&self, job: JobRequest) -> bool {
        self.requests.as_ref().is_some_and(|tx| tx.send(job).is_ok())
    }

    /// Bounded drain for App::logic. `None` events + disconnected flag.
    pub fn drain(&mut self, budget: usize) -> (Vec<JobEvent>, bool) {
        let mut out = Vec::new();
        for _ in 0..budget {
            match self.events.try_recv() {
                Ok(ev) => out.push(ev),
                Err(mpsc::TryRecvError::Empty) => return (out, false),
                Err(mpsc::TryRecvError::Disconnected) => return (out, true),
            }
        }
        (out, false)
    }
}

impl Drop for WorkerHandle {
    fn drop(&mut self) {
        drop(self.requests.take());              // close the channel: recv() errors, loop ends
        if let Some(t) = self.thread.take() {
            let _ = t.join();                    // jobs are short; no flush protocol needed
        }
    }
}

fn run_job(job: JobRequest, evt_tx: &mpsc::Sender<JobEvent>, worker_ctx: &egui::Context) {
    /* phase4 solves this */
}
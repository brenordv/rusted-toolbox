use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

/// A unit of background work. `Probe` is the only job the shell runs: it
/// round-trips the full event sequence so the UI can prove the bridge end to
/// end without doing any real work.
#[derive(Debug)]
pub enum JobRequest {
    Probe,
    #[cfg(test)]
    FailForTests,
    #[cfg(test)]
    PanicForTests,
}

/// What a finished job produced.
#[derive(Debug)]
pub enum JobOutcome {
    /// The probe round-trip completed.
    Probe,
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
        match run_job(&request, job_id, events, ctx) {
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
    request: &JobRequest,
    job_id: u64,
    events: &mpsc::Sender<JobEvent>,
    ctx: &egui::Context,
) -> anyhow::Result<JobOutcome> {
    match request {
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
        #[cfg(test)]
        JobRequest::FailForTests => anyhow::bail!("this test job always fails"),
        #[cfg(test)]
        JobRequest::PanicForTests => panic!("this test job always panics"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    #[test]
    fn probe_round_trips_started_progress_and_done() {
        let handle = WorkerHandle::spawn(egui::Context::default()).unwrap();

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
        let handle = WorkerHandle::spawn(egui::Context::default()).unwrap();
        let (done_tx, done_rx) = mpsc::channel::<()>();

        thread::spawn(move || {
            drop(handle);
            let _ = done_tx.send(());
        });

        assert!(done_rx.recv_timeout(Duration::from_secs(5)).is_ok());
    }

    #[test]
    fn a_failing_job_emits_failed_with_the_rendered_error() {
        let handle = WorkerHandle::spawn(egui::Context::default()).unwrap();

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
        let handle = WorkerHandle::spawn(egui::Context::default()).unwrap();

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

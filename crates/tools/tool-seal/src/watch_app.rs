//! The I/O shell for `seal watch`: argument validation, the debounced
//! watcher, the startup sweep, the per-file seal with open retries, and the
//! run summary. Decisions live in `watch_pipeline`; this module moves bytes
//! and prints results.

use crate::models::WatchJob;
use crate::seal_app::RunOutcome;
use crate::watch_pipeline::{
    DEBOUNCE, OPEN_ATTEMPTS, OpenOutcome, SkipReason, WatchDecision, confine, decide, needs_seal,
    open_with_retry, sealed_output_in,
};
use anyhow::{Context, Result, bail};
use common_cli::broken_pipe::write_out;
use common_file_utils::atomic_write::write_via_temp;
use common_utils::string_utils::format_bytes_to_string;
use notify::RecursiveMode;
use notify_debouncer_full::{DebounceEventResult, DebouncedEvent, new_debouncer};
use shared_crypto::{Recipient, engine, keys};
use std::collections::BTreeSet;
use std::fs::{DirEntry, File};
use std::io::{self, BufReader, BufWriter, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::thread;
use std::time::Duration;
use tracing::{debug, warn};

/// How often the run loop wakes between event deliveries to check Ctrl+C.
const SHUTDOWN_POLL: Duration = Duration::from_millis(200);

/// Lifetime counters for the run summary. `skipped` counts name-rule skips
/// and up-to-date sweep entries; non-files and vanished sources are debug
/// noise, not results, and stay uncounted.
#[derive(Debug, Default, Clone, Copy)]
struct RunTotals {
    encrypted: u64,
    skipped: u64,
    failed: u64,
}

/// Everything the seal paths need. The directories stay as the user gave
/// them (outputs and result lines then speak the user's own paths);
/// [`confine`] proves their resolved forms disjoint at startup and again
/// before every recovery sweep.
struct WatchContext {
    watch_dir: std::path::PathBuf,
    safe_dir: std::path::PathBuf,
    recipients: Vec<Recipient>,
}

/// One per-file outcome inside a sweep or an event batch.
enum SealOutcome {
    Sealed,
    Vanished,
    Failed,
    Aborted,
}

/// Per-sweep counters for the sweep's own result line; vanished sources
/// touch none of them.
#[derive(Debug, Default)]
struct SweepCounts {
    seen: u64,
    encrypted: u64,
    up_to_date: u64,
    failed: u64,
}

/// Runs `seal watch` until Ctrl+C ([`RunOutcome::Interrupted`]) or a fatal
/// watcher failure (an error, after the summary prints). The watcher arms
/// before the startup sweep so a file landing mid-sweep still produces an
/// event; the sweep then closes the gap for files changed while the tool
/// was down.
///
/// # Errors
/// Fails when the recipients cannot be parsed, a folder is missing or
/// confined inside the other, the watcher cannot start, the watcher dies
/// mid-run, or stdout closes.
pub fn run_watch(job: &WatchJob, shutdown: &Arc<AtomicBool>) -> Result<RunOutcome> {
    let recipients = keys::parse_recipients(&job.recipients, &job.recipient_files)?;
    confine(&job.watch_dir, &job.safe_dir)?;
    let mut context = WatchContext {
        watch_dir: job.watch_dir.clone(),
        safe_dir: job.safe_dir.clone(),
        recipients,
    };

    let (sender, receiver) = channel();
    let mut debouncer = new_debouncer(DEBOUNCE, None, move |result: DebounceEventResult| {
        // The handler runs on the debouncer's thread and only forwards;
        // encrypting here would stall the watcher and tangle Ctrl+C.
        let _ = sender.send(result);
    })
    .context("failed to start the folder watcher")?;
    debouncer
        .watch(&context.watch_dir, RecursiveMode::NonRecursive)
        .with_context(|| format!("failed to watch {}", context.watch_dir.display()))?;

    let mut totals = RunTotals::default();
    let outcome = match run_startup(&context, &mut totals, shutdown) {
        Ok(()) => event_loop(&receiver, &mut context, &mut totals, shutdown),
        Err(error) => Err(error),
    };
    print_summary(&totals);
    outcome
}

/// Prints the startup line and runs the catch-up sweep.
fn run_startup(
    context: &WatchContext,
    totals: &mut RunTotals,
    shutdown: &Arc<AtomicBool>,
) -> Result<()> {
    let line = format!(
        "watching {} -> {} ({} recipient(s))\n",
        context.watch_dir.display(),
        context.safe_dir.display(),
        context.recipients.len()
    );
    write_out(&mut io::stdout(), line.as_bytes())?;
    sweep(context, totals, shutdown)
}

/// Receives debounced batches until Ctrl+C or a fatal watcher failure.
/// Error batches may mean lost events, so each one re-proves confinement
/// and re-sweeps; a dead event channel means the watcher thread died and
/// nothing is being watched, which must end the run loudly rather than
/// idle forever.
fn event_loop(
    receiver: &Receiver<DebounceEventResult>,
    context: &mut WatchContext,
    totals: &mut RunTotals,
    shutdown: &Arc<AtomicBool>,
) -> Result<RunOutcome> {
    loop {
        if shutdown.load(Ordering::Relaxed) {
            return Ok(RunOutcome::Interrupted);
        }
        match receiver.recv_timeout(SHUTDOWN_POLL) {
            Ok(Ok(events)) => handle_events(&events, context, totals, shutdown)?,
            Ok(Err(errors)) => {
                for error in errors {
                    warn!(error = %error, "the watcher reported an error; events may have been lost");
                }
                recover(context, totals, shutdown)?;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                bail!("the folder watcher stopped unexpectedly; nothing is being watched anymore");
            }
        }
    }
}

/// The lost-events recovery path: re-prove confinement (a junction created
/// mid-run changes what the arguments resolve to), then re-sweep. Any
/// failure here is fatal; a watcher running against a missing or
/// self-feeding folder must die loudly.
fn recover(
    context: &mut WatchContext,
    totals: &mut RunTotals,
    shutdown: &Arc<AtomicBool>,
) -> Result<()> {
    confine(&context.watch_dir, &context.safe_dir)?;
    sweep(context, totals, shutdown)
}

/// Seals every unique path named by a debounced batch. Events always
/// encrypt (no freshness check): the debouncer already merged the burst,
/// and re-sealing an unchanged file is wasted work, not damage.
fn handle_events(
    events: &[DebouncedEvent],
    context: &WatchContext,
    totals: &mut RunTotals,
    shutdown: &Arc<AtomicBool>,
) -> Result<()> {
    let mut paths = BTreeSet::new();
    for event in events {
        for path in &event.event.paths {
            paths.insert(path.clone());
        }
    }

    for path in paths {
        if shutdown.load(Ordering::Relaxed) {
            return Ok(());
        }
        process_path(&path, context, totals, shutdown)?;
    }
    Ok(())
}

/// Applies the eligibility rules to one event path against the current
/// state of the filesystem, then seals it. The event kind is deliberately
/// ignored: what matters is what the path is now, after the debounce
/// settled.
fn process_path(
    path: &Path,
    context: &WatchContext,
    totals: &mut RunTotals,
    shutdown: &Arc<AtomicBool>,
) -> Result<()> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            debug!(path = %path.display(), "event path no longer exists");
            return Ok(());
        }
        Err(error) => {
            warn!(path = %path.display(), error = %error, "cannot inspect an event path");
            totals.failed += 1;
            return Ok(());
        }
    };

    let Some(name) = path.file_name().map(|name| name.to_string_lossy()) else {
        debug!(path = %path.display(), "event path has no file name");
        return Ok(());
    };

    match decide(&name, metadata.is_file()) {
        WatchDecision::Skip(reason) => {
            debug!(path = %path.display(), %reason, "skipped");
            record_skip(totals, reason);
        }
        WatchDecision::Seal => match seal_one(path, context, shutdown)? {
            SealOutcome::Sealed => totals.encrypted += 1,
            SealOutcome::Failed => totals.failed += 1,
            SealOutcome::Vanished | SealOutcome::Aborted => {}
        },
    }
    Ok(())
}

/// One pass over the watch folder: seals every eligible file whose output
/// is missing or older than its source. Doubles as the recovery tool after
/// the watcher reports lost events. Fails only fatally (the watch folder
/// cannot be listed, or stdout closed); per-file problems are counted into
/// the sweep's result line.
fn sweep(context: &WatchContext, totals: &mut RunTotals, shutdown: &Arc<AtomicBool>) -> Result<()> {
    let mut counts = SweepCounts::default();

    let entries = std::fs::read_dir(&context.watch_dir).with_context(|| {
        format!(
            "cannot list the watch folder {}",
            context.watch_dir.display()
        )
    })?;

    for entry in entries {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }
        sweep_entry(entry, context, totals, &mut counts, shutdown)?;
    }

    let line = format!(
        "swept {} file(s): {} encrypted, {} up to date, {} failed\n",
        counts.seen, counts.encrypted, counts.up_to_date, counts.failed
    );
    write_out(&mut io::stdout(), line.as_bytes())?;
    totals.encrypted += counts.encrypted;
    totals.skipped += counts.up_to_date;
    totals.failed += counts.failed;
    Ok(())
}

/// Handles one directory entry of a sweep: eligibility, the freshness rule,
/// then the seal. Vanished sources leave every counter untouched.
fn sweep_entry(
    entry: io::Result<DirEntry>,
    context: &WatchContext,
    totals: &mut RunTotals,
    counts: &mut SweepCounts,
    shutdown: &Arc<AtomicBool>,
) -> Result<()> {
    let entry = match entry {
        Ok(entry) => entry,
        Err(error) => {
            warn!(error = %error, "failed to read a watch-folder entry");
            counts.seen += 1;
            counts.failed += 1;
            return Ok(());
        }
    };

    let path = entry.path();
    let name = entry.file_name().to_string_lossy().into_owned();
    let is_file = entry
        .file_type()
        .map(|file_type| file_type.is_file())
        .unwrap_or(false);

    match decide(&name, is_file) {
        WatchDecision::Skip(reason) => {
            debug!(path = %path.display(), %reason, "sweep: skipped");
            record_skip(totals, reason);
            return Ok(());
        }
        WatchDecision::Seal => {}
    }

    let source_mtime = match entry.metadata().and_then(|metadata| metadata.modified()) {
        Ok(mtime) => mtime,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            debug!(path = %path.display(), "sweep: source vanished");
            return Ok(());
        }
        Err(error) => {
            warn!(path = %path.display(), error = %error, "sweep: cannot read the source's modified time");
            counts.seen += 1;
            counts.failed += 1;
            return Ok(());
        }
    };

    let output = match sealed_output_in(&context.safe_dir, &path) {
        Ok(output) => output,
        Err(error) => {
            let rendered = format!("{error:#}");
            warn!(path = %path.display(), error = %rendered, "sweep: cannot derive the output name");
            counts.seen += 1;
            counts.failed += 1;
            return Ok(());
        }
    };
    let output_mtime = std::fs::metadata(&output)
        .and_then(|metadata| metadata.modified())
        .ok();

    if !needs_seal(source_mtime, output_mtime) {
        debug!(path = %path.display(), "sweep: output is up to date");
        counts.seen += 1;
        counts.up_to_date += 1;
        return Ok(());
    }

    match seal_one(&path, context, shutdown)? {
        SealOutcome::Sealed => {
            counts.seen += 1;
            counts.encrypted += 1;
        }
        SealOutcome::Failed => {
            counts.seen += 1;
            counts.failed += 1;
        }
        SealOutcome::Vanished | SealOutcome::Aborted => {}
    }
    Ok(())
}

/// Seals one source into the safe folder through the shared temp+rename
/// write, so an existing output is replaced atomically and a failed seal
/// leaves no partial file. The open retries with doubling backoff because
/// editors and sync clients hold short-lived locks; the backoff sleep
/// aborts when Ctrl+C arrives.
///
/// # Errors
/// Fails only for run-ending conditions (stdout closed while reporting);
/// every per-file problem becomes a counted [`SealOutcome`].
fn seal_one(
    source: &Path,
    context: &WatchContext,
    shutdown: &Arc<AtomicBool>,
) -> Result<SealOutcome> {
    let name = match source.file_name() {
        Some(name) => name.to_string_lossy().into_owned(),
        None => source.display().to_string(),
    };

    let output = match sealed_output_in(&context.safe_dir, source) {
        Ok(output) => output,
        Err(error) => {
            let rendered = format!("{error:#}");
            warn!(path = %source.display(), error = %rendered, "cannot derive the output name");
            return Ok(SealOutcome::Failed);
        }
    };

    let opened = open_with_retry(
        || File::open(source),
        |delay| {
            thread::sleep(delay);
            !shutdown.load(Ordering::Relaxed)
        },
    );
    let file = match opened {
        OpenOutcome::Opened(file) => file,
        OpenOutcome::Vanished => {
            debug!(path = %source.display(), "source vanished before it could be opened");
            return Ok(SealOutcome::Vanished);
        }
        OpenOutcome::GaveUp(error) => {
            warn!(
                path = %source.display(),
                error = %error,
                attempts = OPEN_ATTEMPTS,
                "giving up on this source; it will be retried on its next change event"
            );
            return Ok(SealOutcome::Failed);
        }
        OpenOutcome::Aborted => return Ok(SealOutcome::Aborted),
    };

    let mut reader = BufReader::new(file);
    let sealed = write_via_temp(&output, |temp| {
        let staged = File::create(temp)
            .with_context(|| format!("could not create output file {}", temp.display()))?;
        let mut writer = BufWriter::new(staged);
        engine::encrypt(&context.recipients, &mut reader, &mut writer, false)?;
        writer.flush().context("failed to flush the output file")
    });

    match sealed {
        Ok(()) => {
            let size = std::fs::metadata(&output)
                .map(|metadata| format_bytes_to_string(metadata.len()))
                .unwrap_or_else(|_| "size unknown".to_string());
            let line = format!("sealed {name} -> {} ({size})\n", output.display());
            write_out(&mut io::stdout(), line.as_bytes())?;
            Ok(SealOutcome::Sealed)
        }
        Err(error) => {
            let rendered = format!("{error:#}");
            warn!(path = %source.display(), error = %rendered, "failed to seal");
            Ok(SealOutcome::Failed)
        }
    }
}

/// Folds a skip into the run totals. Non-files are left uncounted: the
/// summary counts files that were skipped, and a directory or symlink is
/// not a file that could have been sealed.
fn record_skip(totals: &mut RunTotals, reason: SkipReason) {
    if reason != SkipReason::NotARegularFile {
        totals.skipped += 1;
    }
}

/// The shutdown summary; best-effort because a closed stdout must not mask
/// the outcome being reported.
fn print_summary(totals: &RunTotals) {
    let line = format!(
        "run summary: {} encrypted, {} skipped, {} failed\n",
        totals.encrypted, totals.skipped, totals.failed
    );
    let _ = write_out(&mut io::stdout(), line.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared_crypto::Identity;
    use shared_crypto::keys::{generate_identity, public_key_of};
    use std::path::PathBuf;

    fn fixture_dirs(base: &Path) -> (PathBuf, PathBuf) {
        let watch = base.join("watch");
        let safe = base.join("safe");
        std::fs::create_dir(&watch).unwrap();
        std::fs::create_dir(&safe).unwrap();
        (watch, safe)
    }

    fn make_context(watch: &Path, safe: &Path, identity: &Identity) -> WatchContext {
        WatchContext {
            watch_dir: watch.to_path_buf(),
            safe_dir: safe.to_path_buf(),
            recipients: vec![identity.to_public()],
        }
    }

    fn unset_shutdown() -> Arc<AtomicBool> {
        Arc::new(AtomicBool::new(false))
    }

    fn decrypt_file(path: &Path, identity: &Identity) -> Vec<u8> {
        let mut input = BufReader::new(File::open(path).unwrap());
        let mut plaintext = Vec::new();
        engine::decrypt(std::slice::from_ref(identity), &mut input, &mut plaintext).unwrap();
        plaintext
    }

    fn set_mtime(path: &Path, offset_from_now: std::time::Duration, in_the_past: bool) {
        let now = std::time::SystemTime::now();
        let target = if in_the_past {
            now - offset_from_now
        } else {
            now + offset_from_now
        };
        filetime::set_file_mtime(path, filetime::FileTime::from_system_time(target)).unwrap();
    }

    #[test]
    fn sweep_seals_missing_and_stale_outputs_and_skips_fresh_ones() {
        let base = tempfile::tempdir().unwrap();
        let (watch, safe) = fixture_dirs(base.path());
        let identity = generate_identity();
        let context = make_context(&watch, &safe, &identity);
        std::fs::write(watch.join("new.txt"), b"new content").unwrap();
        std::fs::write(watch.join("fresh.txt"), b"fresh content").unwrap();
        std::fs::write(safe.join("fresh.txt.age"), b"fresh marker").unwrap();
        set_mtime(&watch.join("fresh.txt"), Duration::from_secs(3600), true);
        std::fs::write(watch.join("stale.txt"), b"stale got rewritten").unwrap();
        std::fs::write(safe.join("stale.txt.age"), b"stale marker").unwrap();
        set_mtime(&safe.join("stale.txt.age"), Duration::from_secs(3600), true);
        let mut totals = RunTotals::default();

        sweep(&context, &mut totals, &unset_shutdown()).unwrap();

        assert_eq!(totals.encrypted, 2);
        assert_eq!(totals.skipped, 1);
        assert_eq!(totals.failed, 0);
        assert_eq!(
            decrypt_file(&safe.join("new.txt.age"), &identity),
            b"new content"
        );
        assert_eq!(
            std::fs::read(safe.join("fresh.txt.age")).unwrap(),
            b"fresh marker"
        );
        assert_eq!(
            decrypt_file(&safe.join("stale.txt.age"), &identity),
            b"stale got rewritten"
        );
    }

    #[cfg(windows)]
    fn make_unopenable(path: &Path) -> Option<File> {
        use std::os::windows::fs::OpenOptionsExt;
        Some(
            std::fs::OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(path)
                .unwrap(),
        )
    }

    #[cfg(unix)]
    fn make_unopenable(path: &Path) -> Option<File> {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o000)).unwrap();
        None
    }

    #[test]
    fn sweep_counts_an_unopenable_source_as_failed_and_continues() {
        let base = tempfile::tempdir().unwrap();
        let (watch, safe) = fixture_dirs(base.path());
        let identity = generate_identity();
        let context = make_context(&watch, &safe, &identity);
        std::fs::write(watch.join("locked.txt"), b"locked").unwrap();
        std::fs::write(watch.join("open.txt"), b"open").unwrap();
        let _lock_guard = make_unopenable(&watch.join("locked.txt"));
        let mut totals = RunTotals::default();

        sweep(&context, &mut totals, &unset_shutdown()).unwrap();

        assert_eq!(totals.encrypted, 1);
        assert_eq!(totals.failed, 1);
        assert_eq!(decrypt_file(&safe.join("open.txt.age"), &identity), b"open");
        assert!(!safe.join("locked.txt.age").exists());
    }

    #[test]
    fn seal_one_replaces_an_existing_output_without_temp_residue() {
        let base = tempfile::tempdir().unwrap();
        let (watch, safe) = fixture_dirs(base.path());
        let identity = generate_identity();
        let context = make_context(&watch, &safe, &identity);
        let source = watch.join("doc.txt");
        std::fs::write(&source, b"first version").unwrap();
        std::fs::write(safe.join("doc.txt.age"), b"previous ciphertext").unwrap();
        std::fs::write(&source, b"second version").unwrap();

        let outcome = seal_one(&source, &context, &unset_shutdown()).unwrap();

        assert!(matches!(outcome, SealOutcome::Sealed));
        assert_eq!(
            decrypt_file(&safe.join("doc.txt.age"), &identity),
            b"second version"
        );
        assert_eq!(std::fs::read_dir(&safe).unwrap().count(), 1);
    }

    #[test]
    fn process_path_leaves_counters_untouched_for_a_vanished_source() {
        let base = tempfile::tempdir().unwrap();
        let (watch, safe) = fixture_dirs(base.path());
        let identity = generate_identity();
        let context = make_context(&watch, &safe, &identity);
        let mut totals = RunTotals::default();

        process_path(
            &watch.join("gone.txt"),
            &context,
            &mut totals,
            &unset_shutdown(),
        )
        .unwrap();

        assert_eq!(totals.encrypted, 0);
        assert_eq!(totals.skipped, 0);
        assert_eq!(totals.failed, 0);
    }

    #[test]
    fn process_path_seals_an_eligible_file_and_counts_name_rule_skips() {
        let base = tempfile::tempdir().unwrap();
        let (watch, safe) = fixture_dirs(base.path());
        let identity = generate_identity();
        let context = make_context(&watch, &safe, &identity);
        std::fs::write(watch.join("report.txt"), b"report body").unwrap();
        std::fs::write(watch.join(".hidden"), b"secret notes").unwrap();
        let shutdown = unset_shutdown();
        let mut totals = RunTotals::default();

        process_path(&watch.join("report.txt"), &context, &mut totals, &shutdown).unwrap();
        process_path(&watch.join(".hidden"), &context, &mut totals, &shutdown).unwrap();

        assert_eq!(totals.encrypted, 1);
        assert_eq!(totals.skipped, 1);
        assert_eq!(totals.failed, 0);
        assert_eq!(
            decrypt_file(&safe.join("report.txt.age"), &identity),
            b"report body"
        );
        assert!(!safe.join(".hidden.age").exists());
    }

    #[test]
    fn event_loop_fails_when_the_watcher_channel_disconnects() {
        let base = tempfile::tempdir().unwrap();
        let (watch, safe) = fixture_dirs(base.path());
        let identity = generate_identity();
        let mut context = make_context(&watch, &safe, &identity);
        let (sender, receiver) = channel::<DebounceEventResult>();
        drop(sender);
        let mut totals = RunTotals::default();

        let error =
            event_loop(&receiver, &mut context, &mut totals, &unset_shutdown()).unwrap_err();

        assert!(format!("{error:#}").contains("stopped unexpectedly"));
    }

    #[test]
    fn event_loop_recovery_fails_when_the_watch_dir_is_gone() {
        let base = tempfile::tempdir().unwrap();
        let (watch, safe) = fixture_dirs(base.path());
        let identity = generate_identity();
        let mut context = make_context(&watch, &safe, &identity);
        std::fs::remove_dir(&watch).unwrap();
        let (sender, receiver) = channel::<DebounceEventResult>();
        sender
            .send(Err(vec![notify::Error::generic("simulated overflow")]))
            .unwrap();
        let mut totals = RunTotals::default();

        let error =
            event_loop(&receiver, &mut context, &mut totals, &unset_shutdown()).unwrap_err();

        assert!(format!("{error:#}").contains("watch folder"));
    }

    #[test]
    fn watch_end_to_end_seals_a_dropped_file_and_stops_on_the_shutdown_flag() {
        let base = tempfile::tempdir().unwrap();
        let (watch, safe) = fixture_dirs(base.path());
        let identity = generate_identity();
        let job = WatchJob {
            recipients: vec![public_key_of(&identity)],
            recipient_files: vec![],
            watch_dir: watch.clone(),
            safe_dir: safe.clone(),
        };
        let shutdown = unset_shutdown();
        let worker = {
            let job = job.clone();
            let shutdown = Arc::clone(&shutdown);
            thread::spawn(move || run_watch(&job, &shutdown))
        };
        thread::sleep(Duration::from_millis(500));

        std::fs::write(watch.join("dropped.txt"), b"dropped payload").unwrap();
        let output = safe.join("dropped.txt.age");
        for _ in 0..150 {
            if output.exists() {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
        shutdown.store(true, Ordering::Relaxed);
        let outcome = worker.join().unwrap().unwrap();

        assert!(matches!(outcome, RunOutcome::Interrupted));
        assert_eq!(decrypt_file(&output, &identity), b"dropped payload");
    }
}

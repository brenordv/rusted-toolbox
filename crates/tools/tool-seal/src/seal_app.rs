use crate::models::{DecryptJob, EncryptJob, KeygenJob, OutputSpec, SealCommand};
use crate::watch_app::run_watch;
use anyhow::{Context, Result, bail};
use common_cli::broken_pipe::{BrokenPipe, flush_out, write_out};
use common_file_utils::atomic_write::write_via_temp;
use common_utils::string_utils::format_bytes_to_string;
use shared_crypto::{Direction, engine, keys, output_name_for};
use std::fs::File;
use std::io::{self, BufReader, BufWriter, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

const MSG_BINARY_TO_TERMINAL: &str =
    "refusing to write binary ciphertext to a terminal; use --armor or redirect";

/// How a run ended when it did not fail: normally, or cut short by Ctrl+C.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    Completed,
    Interrupted,
}

/// Runs the resolved job. A failure with the shutdown flag set reports as
/// [`RunOutcome::Interrupted`] rather than an error: the input wrapper aborts
/// the stream when the flag flips, so the underlying error is the interrupt.
pub fn run(command: SealCommand, shutdown: &Arc<AtomicBool>) -> Result<RunOutcome> {
    let result = match command {
        SealCommand::Keygen(job) => run_keygen(&job),
        SealCommand::Encrypt(job) => run_encrypt(&job, shutdown),
        SealCommand::Decrypt(job) => run_decrypt(&job, shutdown),
        // Watch owns its whole lifecycle (it only ends on Ctrl+C or a fatal
        // watcher failure), so it reports its own outcome.
        SealCommand::Watch(job) => return run_watch(&job, shutdown),
    };

    match result {
        Err(error) if shutdown.load(Ordering::Relaxed) && !error.is::<BrokenPipe>() => {
            Ok(RunOutcome::Interrupted)
        }
        Err(error) => Err(error),
        Ok(()) => Ok(RunOutcome::Completed),
    }
}

/// `seal keygen`: with `-o`, saves the identity file and prints just the public key; without it,
/// dumps the full identity file to stdout with a notice on stderr.
fn run_keygen(job: &KeygenJob) -> Result<()> {
    let identity = keys::generate_identity();

    match &job.output {
        Some(path) => {
            keys::save_identity_file(&identity, path, job.force)?;
            write_out(
                &mut io::stdout(),
                format!("{}\n", keys::public_key_of(&identity)).as_bytes(),
            )?;
        }
        None => {
            // The notice is part of the keygen contract (age-keygen parity),
            // printed straight to stderr so no log-level setting can silence
            // it; best-effort, because a dead stderr must not stop the
            // identity from reaching stdout.
            let _ = writeln!(
                io::stderr(),
                "{}",
                keygen_notice(io::stdout().is_terminal())
            );
            let mut stdout = io::stdout().lock();
            keys::write_identity(&identity, &mut stdout).map_err(mark_broken_pipe)?;
        }
    }

    Ok(())
}

/// `seal encrypt`: seals the input (file or stdin) to the merged recipient
/// set, writing through a temp file when the output is a path.
fn run_encrypt(job: &EncryptJob, shutdown: &Arc<AtomicBool>) -> Result<()> {
    let recipients = keys::parse_recipients(&job.recipients, &job.recipient_files)?;
    let output = resolve_output(&job.output, job.input.as_deref(), Direction::Encrypt)?;
    let mut input = open_input(job.input.as_deref(), shutdown)?;

    match &output {
        OutputTarget::Stdout => {
            if refuses_terminal_ciphertext(job.armor, io::stdout().is_terminal()) {
                bail!(MSG_BINARY_TO_TERMINAL);
            }
            let mut stdout = io::stdout().lock();
            engine::encrypt(&recipients, &mut input, &mut stdout, job.armor)
                .map_err(mark_broken_pipe)?;
            flush_out(&mut stdout)?;
            drop(stdout);
            report_result("sealed", job.input.as_deref(), &output)?;
        }
        OutputTarget::Path(path) => {
            refuse_existing_output(path, job.force)?;
            write_via_temp(path, |temp| {
                let mut writer = open_output(temp)?;
                engine::encrypt(&recipients, &mut input, &mut writer, job.armor)?;
                writer.flush().context("failed to flush the output file")
            })?;
            report_result("sealed", job.input.as_deref(), &output)?;
        }
    }

    Ok(())
}

/// `seal decrypt`: opens the input with the loaded identities. On any engine
/// failure the output is poisoned; the temp-file write makes sure no partial
/// file survives, and stdout consumers are told to trust only exit code 0.
fn run_decrypt(job: &DecryptJob, shutdown: &Arc<AtomicBool>) -> Result<()> {
    let identities = keys::load_identities(&job.identity_files)?;
    let output = resolve_output(&job.output, job.input.as_deref(), Direction::Decrypt)?;
    let mut input = open_input(job.input.as_deref(), shutdown)?;

    match &output {
        OutputTarget::Stdout => {
            let mut stdout = io::stdout().lock();
            engine::decrypt(&identities, &mut input, &mut stdout).map_err(mark_broken_pipe)?;
            flush_out(&mut stdout)?;
            drop(stdout);
            report_result("opened", job.input.as_deref(), &output)?;
        }
        OutputTarget::Path(path) => {
            refuse_existing_output(path, job.force)?;
            write_via_temp(path, |temp| {
                let mut writer = open_output(temp)?;
                engine::decrypt(&identities, &mut input, &mut writer)?;
                writer.flush().context("failed to flush the output file")
            })?;
            report_result("opened", job.input.as_deref(), &output)?;
        }
    }

    Ok(())
}

/// Where an operation actually writes, after the derived default is resolved.
#[derive(Debug)]
enum OutputTarget {
    Stdout,
    Path(PathBuf),
}

/// Resolves the output spec against the input: an explicit path or `-` wins;
/// otherwise a file input derives its name through `shared-crypto`'s naming
/// rule and a stdin input streams to stdout.
fn resolve_output(
    spec: &OutputSpec,
    input: Option<&Path>,
    direction: Direction,
) -> Result<OutputTarget> {
    match spec {
        OutputSpec::Stdout => Ok(OutputTarget::Stdout),
        OutputSpec::Path(path) => Ok(OutputTarget::Path(path.clone())),
        OutputSpec::Derived => match input {
            Some(path) => Ok(OutputTarget::Path(output_name_for(path, direction)?)),
            None => Ok(OutputTarget::Stdout),
        },
    }
}

/// One overwrite rule for every output path, the derived `.age` default
/// included. Keygen's identity file is guarded separately (and atomically)
/// inside `shared-crypto`.
fn refuse_existing_output(path: &Path, force: bool) -> Result<()> {
    if !force && path.exists() {
        bail!(
            "output file {} already exists; pass --force to overwrite it",
            path.display()
        );
    }
    Ok(())
}

/// The binary-terminal guard's decision: armored output is text and may hit a
/// terminal; binary ciphertext may not.
fn refuses_terminal_ciphertext(armor: bool, stdout_is_terminal: bool) -> bool {
    !armor && stdout_is_terminal
}

/// The keygen stderr notice: a terminal is shown a secret-key warning, a
/// redirect is warned that the receiving file is not permission-tightened.
fn keygen_notice(stdout_is_terminal: bool) -> &'static str {
    if stdout_is_terminal {
        "warning: you are displaying a secret key"
    } else {
        "warning: the receiving file gets your umask's default permissions, not 0600; prefer -o"
    }
}

/// Opens the input stream (file or stdin), wrapped so Ctrl+C aborts the next
/// read instead of waiting for the stream to end.
fn open_input(
    path: Option<&Path>,
    shutdown: &Arc<AtomicBool>,
) -> Result<InterruptibleReader<Box<dyn Read>>> {
    let inner: Box<dyn Read> = match path {
        Some(path) => {
            Box::new(BufReader::new(File::open(path).with_context(|| {
                format!("could not open input file {}", path.display())
            })?))
        }
        None => Box::new(io::stdin().lock()),
    };

    Ok(InterruptibleReader {
        inner,
        shutdown: Arc::clone(shutdown),
    })
}

/// Opens the staged output file for a path-targeted operation.
fn open_output(path: &Path) -> Result<BufWriter<File>> {
    let file = File::create(path)
        .with_context(|| format!("could not create output file {}", path.display()))?;
    Ok(BufWriter::new(file))
}

/// Prints the one result line per operation: to stdout normally, to stderr
/// when the payload itself went to stdout. The size shown is the written
/// file's, by stat; a stdout target has no file to stat and gets no size.
fn report_result(verb: &str, input: Option<&Path>, output: &OutputTarget) -> Result<()> {
    let input_name = match input {
        Some(path) => path.display().to_string(),
        None => "stdin".to_string(),
    };

    match output {
        OutputTarget::Stdout => {
            let line = format!("{verb} {input_name} -> stdout\n");
            write_out(&mut io::stderr(), line.as_bytes())
        }
        OutputTarget::Path(path) => {
            let size = std::fs::metadata(path)
                .with_context(|| format!("could not stat output file {}", path.display()))?
                .len();
            let line = format!(
                "{verb} {input_name} -> {} ({})\n",
                path.display(),
                format_bytes_to_string(size)
            );
            write_out(&mut io::stdout(), line.as_bytes())
        }
    }
}

/// Re-marks an error whose cause chain holds a closed-pipe I/O failure as the
/// shared [`BrokenPipe`] marker, so the entrypoint exits 0 the way every
/// streaming tool here does.
fn mark_broken_pipe(error: anyhow::Error) -> anyhow::Error {
    let closed_pipe = error.chain().any(|cause| {
        cause
            .downcast_ref::<io::Error>()
            .is_some_and(|io_error| io_error.kind() == io::ErrorKind::BrokenPipe)
    });

    if closed_pipe {
        anyhow::Error::new(BrokenPipe)
    } else {
        error
    }
}

/// Wraps the input stream so a flipped shutdown flag fails the next read.
/// The error deliberately avoids `ErrorKind::Interrupted`: `io::copy` retries
/// that kind, which would spin instead of aborting.
struct InterruptibleReader<R> {
    inner: R,
    shutdown: Arc<AtomicBool>,
}

impl<R: Read> Read for InterruptibleReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.shutdown.load(Ordering::Relaxed) {
            return Err(io::Error::other("interrupted by the user"));
        }
        self.inner.read(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn target_path(target: OutputTarget) -> PathBuf {
        match target {
            OutputTarget::Path(path) => path,
            OutputTarget::Stdout => panic!("expected a path target"),
        }
    }

    #[test]
    fn derived_output_appends_age_on_encrypt_and_strips_it_on_decrypt() {
        let sealed = resolve_output(
            &OutputSpec::Derived,
            Some(Path::new("file.txt")),
            Direction::Encrypt,
        )
        .unwrap();
        assert_eq!(target_path(sealed), Path::new("file.txt.age"));

        let opened = resolve_output(
            &OutputSpec::Derived,
            Some(Path::new("file.txt.age")),
            Direction::Decrypt,
        )
        .unwrap();
        assert_eq!(target_path(opened), Path::new("file.txt"));
    }

    #[test]
    fn derived_output_for_stdin_input_is_stdout() {
        let target = resolve_output(&OutputSpec::Derived, None, Direction::Encrypt).unwrap();
        assert!(matches!(target, OutputTarget::Stdout));
    }

    #[test]
    fn explicit_output_beats_the_derived_default() {
        let target = resolve_output(
            &OutputSpec::Path(PathBuf::from("elsewhere.bin")),
            Some(Path::new("file.txt")),
            Direction::Encrypt,
        )
        .unwrap();
        assert_eq!(target_path(target), Path::new("elsewhere.bin"));

        let target = resolve_output(
            &OutputSpec::Stdout,
            Some(Path::new("file.txt")),
            Direction::Encrypt,
        )
        .unwrap();
        assert!(matches!(target, OutputTarget::Stdout));
    }

    #[test]
    fn derived_decrypt_output_without_an_age_suffix_asks_for_an_explicit_one() {
        let error = resolve_output(
            &OutputSpec::Derived,
            Some(Path::new("file.enc")),
            Direction::Decrypt,
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("-o <output>"));
    }

    #[test]
    fn existing_output_is_refused_without_force_and_allowed_with_it() {
        let dir = tempfile::tempdir().unwrap();
        let existing = dir.path().join("out.age");
        std::fs::write(&existing, b"present").unwrap();

        let error = refuse_existing_output(&existing, false).unwrap_err();
        assert!(format!("{error:#}").contains("--force"));

        refuse_existing_output(&existing, true).unwrap();
        refuse_existing_output(&dir.path().join("absent.age"), false).unwrap();
    }

    #[rstest]
    #[case::binary_to_terminal(false, true, true)]
    #[case::armor_to_terminal(true, true, false)]
    #[case::binary_redirected(false, false, false)]
    #[case::armor_redirected(true, false, false)]
    fn terminal_ciphertext_guard_blocks_only_binary_on_a_terminal(
        #[case] armor: bool,
        #[case] terminal: bool,
        #[case] refused: bool,
    ) {
        assert_eq!(refuses_terminal_ciphertext(armor, terminal), refused);
    }

    #[test]
    fn keygen_notice_warns_about_display_or_permissions() {
        assert!(keygen_notice(true).contains("displaying a secret key"));
        assert!(keygen_notice(false).contains("prefer -o"));
    }

    #[test]
    fn a_set_shutdown_flag_fails_the_next_read_without_the_retried_kind() {
        let shutdown = Arc::new(AtomicBool::new(false));
        let mut reader = InterruptibleReader {
            inner: &b"payload"[..],
            shutdown: Arc::clone(&shutdown),
        };
        let mut buf = [0u8; 4];

        assert_eq!(reader.read(&mut buf).unwrap(), 4);

        shutdown.store(true, Ordering::Relaxed);
        let error = reader.read(&mut buf).unwrap_err();
        assert_ne!(error.kind(), io::ErrorKind::Interrupted);
    }

    #[test]
    fn broken_pipe_io_failures_are_remarked_and_others_pass_through() {
        let pipe = anyhow::Error::new(io::Error::new(io::ErrorKind::BrokenPipe, "pipe closed"))
            .context("I/O failure while encrypting");
        assert!(mark_broken_pipe(pipe).is::<BrokenPipe>());

        let disk = anyhow::Error::new(io::Error::new(io::ErrorKind::StorageFull, "disk full"));
        assert!(!mark_broken_pipe(disk).is::<BrokenPipe>());
    }
}

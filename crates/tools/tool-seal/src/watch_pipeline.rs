//! Pure decision logic for `seal watch`: which directory entries get sealed,
//! where their outputs go, the watch/safe confinement rule, the sweep's
//! freshness rule, and the open-retry schedule. Everything here is testable
//! without a real watcher; `watch_app` is the I/O shell around it.

use anyhow::{Context, Result, bail};
use shared_crypto::{Direction, output_name_for};
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// How long the watcher lets a burst of events settle before delivering it.
/// Two seconds covers editors' write-then-rename save dances; a flag joins
/// only when a real case breaks this default.
pub const DEBOUNCE: Duration = Duration::from_millis(2000);

/// Name suffixes of in-flight artifacts other programs write and rename away
/// (browser downloads, editor swap files); matched ASCII case-insensitively.
const TEMP_SUFFIXES: [&str; 4] = [".tmp", ".part", ".crdownload", ".swp"];

/// Office-style lock/temp file prefix.
const TEMP_PREFIX: &str = "~$";

/// Total open attempts before a source is given up on until its next event.
pub const OPEN_ATTEMPTS: u32 = 5;

/// First open-retry delay; each later attempt doubles it.
const RETRY_BASE_DELAY: Duration = Duration::from_millis(100);

/// Why a directory entry is left alone; rendered into the debug log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    NotARegularFile,
    TempArtifact,
    HiddenFile,
    AlreadySealed,
}

impl fmt::Display for SkipReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            SkipReason::NotARegularFile => "not a regular file",
            SkipReason::TempArtifact => "temporary artifact name",
            SkipReason::HiddenFile => "hidden dotfile",
            SkipReason::AlreadySealed => "already an .age file",
        };
        f.write_str(text)
    }
}

/// What to do with one watch-folder entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchDecision {
    Seal,
    Skip(SkipReason),
}

/// Decides whether a watch-folder entry gets sealed. Directories and
/// symlinks never qualify (the watcher is non-recursive and follows
/// nothing), temp artifacts are left to settle, hidden dotfiles stay
/// hidden, and `.age` files are refused so ciphertext dropped into the
/// watch folder can never loop back through the encryptor.
pub fn decide(name: &str, is_regular_file: bool) -> WatchDecision {
    if !is_regular_file {
        return WatchDecision::Skip(SkipReason::NotARegularFile);
    }
    if name.starts_with(TEMP_PREFIX) || has_suffix_in(name, &TEMP_SUFFIXES) {
        return WatchDecision::Skip(SkipReason::TempArtifact);
    }
    if name.starts_with('.') {
        return WatchDecision::Skip(SkipReason::HiddenFile);
    }
    if has_suffix_in(name, &[".age"]) {
        return WatchDecision::Skip(SkipReason::AlreadySealed);
    }
    WatchDecision::Seal
}

/// Whether `name` ends in one of `suffixes`, ASCII case-insensitively. A
/// name that is nothing but the suffix does not count (those fall to the
/// hidden-dotfile rule), and the split point is checked as a char boundary
/// so a multi-byte final character can never panic the slice.
fn has_suffix_in(name: &str, suffixes: &[&str]) -> bool {
    suffixes.iter().any(|suffix| {
        name.len() > suffix.len()
            && name.is_char_boundary(name.len() - suffix.len())
            && name[name.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
    })
}

/// Maps `<watch>/<name>` onto `<safe>/<name>.age` through the same naming
/// rule the CLI and GUI use.
pub fn sealed_output_in(safe_dir: &Path, source: &Path) -> Result<PathBuf> {
    let sealed = output_name_for(source, Direction::Encrypt)?;
    let name = sealed
        .file_name()
        .context("the sealed name has no file-name component")?;
    Ok(safe_dir.join(name))
}

/// Canonicalizes both directories and refuses any arrangement where one
/// contains the other (or they are the same). The compare runs on resolved
/// paths because a junction or symlink can place the safe folder inside the
/// watch folder while the argument strings look disjoint; a watch folder
/// that sees its own outputs re-encrypts them forever.
pub fn confine(watch_dir: &Path, safe_dir: &Path) -> Result<()> {
    let watch = resolve_dir(watch_dir, "watch")?;
    let safe = resolve_dir(safe_dir, "safe")?;

    if watch == safe {
        bail!(
            "the watch and safe folders resolve to the same directory ({})",
            watch.display()
        );
    }
    if safe.starts_with(&watch) {
        bail!(
            "the safe folder resolves to inside the watch folder ({} is under {}); every output would be re-encrypted forever",
            safe.display(),
            watch.display()
        );
    }
    if watch.starts_with(&safe) {
        bail!(
            "the watch folder resolves to inside the safe folder ({} is under {})",
            watch.display(),
            safe.display()
        );
    }
    Ok(())
}

/// Resolves one folder argument physically, requiring it to exist and be a
/// directory.
fn resolve_dir(path: &Path, role: &str) -> Result<PathBuf> {
    let resolved = std::fs::canonicalize(path).with_context(|| {
        format!(
            "the {role} folder {} does not exist or cannot be resolved",
            path.display()
        )
    })?;
    if !resolved.is_dir() {
        bail!("the {role} folder {} is not a directory", path.display());
    }
    Ok(resolved)
}

/// The sweep's freshness rule: a source needs sealing when its output is
/// missing or older than the source.
pub fn needs_seal(source_mtime: SystemTime, output_mtime: Option<SystemTime>) -> bool {
    match output_mtime {
        None => true,
        Some(output) => output < source_mtime,
    }
}

/// One terminal state of the open-retry loop.
pub enum OpenOutcome<T> {
    Opened(T),
    /// The source disappeared; temp files do this constantly, so callers
    /// log it at debug and move on.
    Vanished,
    /// Every attempt failed with something other than not-found.
    GaveUp(io::Error),
    /// The sleep callback reported a shutdown request mid-backoff.
    Aborted,
}

/// Retries `attempt` up to [`OPEN_ATTEMPTS`] times with doubling backoff
/// starting at 100 ms. `sleep` performs the wait and returns `false` to
/// abort (a shutdown request). A not-found failure stops the retries
/// immediately: the source vanished and only its next event brings it back.
pub fn open_with_retry<T>(
    mut attempt: impl FnMut() -> io::Result<T>,
    mut sleep: impl FnMut(Duration) -> bool,
) -> OpenOutcome<T> {
    let mut delay = RETRY_BASE_DELAY;
    let mut last_error = None;

    for tries_left in (0..OPEN_ATTEMPTS).rev() {
        match attempt() {
            Ok(value) => return OpenOutcome::Opened(value),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return OpenOutcome::Vanished;
            }
            Err(error) => last_error = Some(error),
        }
        if tries_left > 0 {
            if !sleep(delay) {
                return OpenOutcome::Aborted;
            }
            delay *= 2;
        }
    }

    match last_error {
        Some(error) => OpenOutcome::GaveUp(error),
        // OPEN_ATTEMPTS is positive, so at least one attempt ran and
        // recorded its error; this arm is unreachable but handled.
        None => OpenOutcome::GaveUp(io::Error::other("no open attempt was made")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use std::time::Duration;

    #[rstest]
    #[case::plain_document("report.pdf")]
    #[case::multi_extension("x.tar.gz")]
    #[case::age_in_the_middle("x.age.txt")]
    #[case::multibyte_near_suffix("x\u{20ac}age")]
    fn regular_files_with_ordinary_names_are_sealed(#[case] name: &str) {
        assert_eq!(decide(name, true), WatchDecision::Seal);
    }

    #[rstest]
    #[case::office_lock("~$doc.docx", SkipReason::TempArtifact)]
    #[case::tmp("x.tmp", SkipReason::TempArtifact)]
    #[case::tmp_uppercase("X.TMP", SkipReason::TempArtifact)]
    #[case::partial_download("y.part", SkipReason::TempArtifact)]
    #[case::chrome_download("z.crdownload", SkipReason::TempArtifact)]
    #[case::vim_swap("w.swp", SkipReason::TempArtifact)]
    #[case::hidden(".hidden", SkipReason::HiddenFile)]
    #[case::bare_suffix_is_hidden(".tmp", SkipReason::HiddenFile)]
    #[case::sealed("y.age", SkipReason::AlreadySealed)]
    #[case::sealed_uppercase("Y.AGE", SkipReason::AlreadySealed)]
    fn artifact_hidden_and_sealed_names_are_skipped(
        #[case] name: &str,
        #[case] reason: SkipReason,
    ) {
        assert_eq!(decide(name, true), WatchDecision::Skip(reason));
    }

    #[test]
    fn non_regular_files_are_skipped_before_any_name_rule() {
        assert_eq!(
            decide("report.pdf", false),
            WatchDecision::Skip(SkipReason::NotARegularFile)
        );
    }

    #[test]
    fn skip_reasons_render_for_the_debug_log() {
        assert_eq!(
            SkipReason::NotARegularFile.to_string(),
            "not a regular file"
        );
        assert_eq!(
            SkipReason::TempArtifact.to_string(),
            "temporary artifact name"
        );
        assert_eq!(SkipReason::HiddenFile.to_string(), "hidden dotfile");
        assert_eq!(
            SkipReason::AlreadySealed.to_string(),
            "already an .age file"
        );
    }

    #[rstest]
    #[case::plain("report.pdf", "report.pdf.age")]
    #[case::multi_extension("x.tar.gz", "x.tar.gz.age")]
    fn sealed_output_lands_in_the_safe_dir_with_the_age_suffix(
        #[case] source_name: &str,
        #[case] expected_name: &str,
    ) {
        let output =
            sealed_output_in(Path::new("safe"), &Path::new("watch").join(source_name)).unwrap();

        assert_eq!(output, Path::new("safe").join(expected_name));
    }

    #[test]
    fn confine_accepts_sibling_directories() {
        let base = tempfile::tempdir().unwrap();
        let watch = base.path().join("watch");
        let safe = base.path().join("safe");
        std::fs::create_dir(&watch).unwrap();
        std::fs::create_dir(&safe).unwrap();

        confine(&watch, &safe).unwrap();
    }

    #[test]
    fn confine_refuses_the_same_directory() {
        let base = tempfile::tempdir().unwrap();

        let error = confine(base.path(), base.path()).unwrap_err();

        assert!(format!("{error:#}").contains("same directory"));
    }

    #[test]
    fn confine_refuses_a_safe_dir_inside_the_watch_dir() {
        let base = tempfile::tempdir().unwrap();
        let safe = base.path().join("safe");
        std::fs::create_dir(&safe).unwrap();

        let error = confine(base.path(), &safe).unwrap_err();

        assert!(format!("{error:#}").contains("inside the watch folder"));
    }

    #[test]
    fn confine_refuses_a_watch_dir_inside_the_safe_dir() {
        let base = tempfile::tempdir().unwrap();
        let watch = base.path().join("watch");
        std::fs::create_dir(&watch).unwrap();

        let error = confine(&watch, base.path()).unwrap_err();

        assert!(format!("{error:#}").contains("inside the safe folder"));
    }

    #[test]
    fn confine_refuses_a_missing_directory() {
        let base = tempfile::tempdir().unwrap();
        let watch = base.path().join("watch");
        std::fs::create_dir(&watch).unwrap();

        let error = confine(&watch, &base.path().join("absent")).unwrap_err();

        assert!(format!("{error:#}").contains("safe folder"));
    }

    #[cfg(windows)]
    #[test]
    fn confine_refuses_a_safe_dir_that_resolves_into_the_watch_dir_via_a_junction() {
        let base = tempfile::tempdir().unwrap();
        let watch = base.path().join("watch");
        std::fs::create_dir(&watch).unwrap();
        std::fs::create_dir(watch.join("out")).unwrap();
        let link = base.path().join("link");

        let created = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&watch)
            .output();
        let Ok(output) = created else {
            eprintln!("skipping: cmd/mklink is unavailable in this environment");
            return;
        };
        if !output.status.success() {
            eprintln!("skipping: junction creation was refused in this environment");
            return;
        }

        let error = confine(&watch, &link.join("out")).unwrap_err();

        assert!(format!("{error:#}").contains("inside the watch folder"));
    }

    #[test]
    fn needs_seal_when_the_output_is_missing_or_older() {
        let source = SystemTime::now();
        let older = source - Duration::from_secs(60);
        let newer = source + Duration::from_secs(60);

        assert!(needs_seal(source, None));
        assert!(needs_seal(source, Some(older)));
        assert!(!needs_seal(source, Some(source)));
        assert!(!needs_seal(source, Some(newer)));
    }

    #[test]
    fn open_with_retry_succeeds_after_transient_failures_with_doubling_backoff() {
        let mut attempts = 0;
        let mut sleeps = Vec::new();

        let outcome = open_with_retry(
            || {
                attempts += 1;
                if attempts < 3 {
                    Err(io::Error::other("still locked"))
                } else {
                    Ok(attempts)
                }
            },
            |delay| {
                sleeps.push(delay);
                true
            },
        );

        assert!(matches!(outcome, OpenOutcome::Opened(3)));
        assert_eq!(
            sleeps,
            [Duration::from_millis(100), Duration::from_millis(200)]
        );
    }

    #[test]
    fn open_with_retry_gives_up_after_all_attempts() {
        let mut attempts = 0;
        let mut sleeps = Vec::new();

        let outcome = open_with_retry::<()>(
            || {
                attempts += 1;
                Err(io::Error::other("still locked"))
            },
            |delay| {
                sleeps.push(delay);
                true
            },
        );

        assert!(matches!(outcome, OpenOutcome::GaveUp(_)));
        assert_eq!(attempts, OPEN_ATTEMPTS);
        assert_eq!(
            sleeps,
            [
                Duration::from_millis(100),
                Duration::from_millis(200),
                Duration::from_millis(400),
                Duration::from_millis(800)
            ]
        );
    }

    #[test]
    fn open_with_retry_treats_not_found_as_vanished_without_retrying() {
        let mut attempts = 0;

        let outcome = open_with_retry::<()>(
            || {
                attempts += 1;
                Err(io::Error::new(io::ErrorKind::NotFound, "gone"))
            },
            |_| panic!("a vanished source must not be retried"),
        );

        assert!(matches!(outcome, OpenOutcome::Vanished));
        assert_eq!(attempts, 1);
    }

    #[test]
    fn open_with_retry_aborts_when_the_sleep_reports_shutdown() {
        let outcome = open_with_retry::<()>(|| Err(io::Error::other("still locked")), |_| false);

        assert!(matches!(outcome, OpenOutcome::Aborted));
    }
}

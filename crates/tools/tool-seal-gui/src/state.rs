use crate::app::Tab;
use crate::config::RecipientEntry;
use crate::worker::{FileResult, JobEvent, JobKind, JobOutcome, JobRequest, resolve_output_path};
use common_gui::widgets::{Toast, ToastKind};
use shared_crypto::{Direction, Recipient, output_name_for};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::{debug, error};

/// How long a toast stays on screen.
pub const TOAST_TTL: Duration = Duration::from_secs(4);

pub const MSG_BAD_KEY: &str = "not a valid age public key (expected an age1... value)";
pub const MSG_PICK_RECIPIENTS: &str = "choose at least one recipient or paste a key";
pub const MSG_PICK_IDENTITY: &str = "choose an identity file first";
pub const MSG_EMPTY_TEXT: &str = "enter some text first";
pub const MSG_NO_FILES: &str = "add at least one file";
pub const MSG_INVALID_ROWS: &str = "remove the invalid rows first";
pub const MSG_PICK_FOLDER: &str = "choose an output folder";
pub const MSG_ROW_NEEDS_AGE: &str = "Unseal needs a .age file";
pub const MSG_ROW_UNNAMEABLE: &str = "cannot derive an output name for this path";
pub const MSG_NOT_TEXT: &str =
    "the unsealed content is not text; unseal the file in the Files tab instead";
pub const MSG_DIRECTORY_REJECTED: &str = "folders cannot be added; pick the files inside instead";

/// Everything the views render. This struct is the single source of truth:
/// views bind widgets straight to these fields, and egui's own widget memory
/// is never authoritative (eframe persistence is off).
pub struct AppState {
    pub active_tab: Tab,
    pub toasts: Vec<Toast>,
    /// A job is in flight; job-starting actions are disabled while set.
    pub busy: bool,
    /// Per-file progress of the running job, as (done, total).
    pub progress: Option<(usize, usize)>,
    /// The worker thread is gone; jobs cannot run again this session.
    pub worker_gone: bool,
    /// The in-flight job's feature, so failure cleanup can route to the
    /// right tab's state.
    pub active_job: Option<JobKind>,
    /// The recipient book: loaded from config, edited in Keys, consumed by
    /// the Text and Files pickers.
    pub book: Vec<RecipientEntry>,
    /// The identity file used to unseal, shared by Text and Files and
    /// remembered in the config.
    pub identity_path: Option<PathBuf>,
    pub text: TextTabState,
    pub files: FilesTabState,
    pub keys: KeysTabState,
    /// Config-backed state changed; the app flushes on tab switch and exit.
    pub config_dirty: bool,
    /// Standing status-bar note (foreign or unreadable config file).
    pub status_note: Option<String>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            active_tab: Tab::Text,
            toasts: Vec::new(),
            busy: false,
            progress: None,
            worker_gone: false,
            active_job: None,
            book: Vec::new(),
            identity_path: None,
            text: TextTabState::default(),
            files: FilesTabState::default(),
            keys: KeysTabState::default(),
            config_dirty: false,
            status_note: None,
        }
    }
}

/// Per-run recipient selection: which book entries are checked, plus a
/// one-off pasted key. View state, deliberately not part of the book, so
/// config save/load never touches selection and each tab holds its own.
#[derive(Default)]
pub struct RecipientPicker {
    /// Parallel to the book's entries.
    pub selected: Vec<bool>,
    /// A one-off `age1...` paste.
    pub free_text: String,
    /// Inline validation result for the free text.
    pub free_error: Option<String>,
}

impl RecipientPicker {
    /// Realigns the selection vector after the book changed; new entries
    /// start unchecked.
    pub fn sync_with_book(&mut self, book_len: usize) {
        self.selected.resize(book_len, false);
    }

    /// Validates the free text for keystroke-live feedback. Advisory only:
    /// [`effective_recipients`] re-validates at submit time regardless.
    pub fn validate_free_text(&mut self) {
        let trimmed = self.free_text.trim();
        self.free_error = if trimmed.is_empty() || trimmed.parse::<Recipient>().is_ok() {
            None
        } else {
            Some(MSG_BAD_KEY.to_string())
        };
    }
}

/// Merges the checked book entries with the free-text key into the job's
/// recipient set. Errors on an empty selection, an invalid free key, or a
/// book entry that no longer parses (a hand-edited config).
pub fn effective_recipients(
    book: &[RecipientEntry],
    picker: &RecipientPicker,
) -> Result<Vec<Recipient>, String> {
    let mut recipients = Vec::new();
    for (entry, selected) in book.iter().zip(&picker.selected) {
        if !selected {
            continue;
        }
        let recipient = entry.public_key.parse::<Recipient>().map_err(|_| {
            format!(
                "the saved key for \"{}\" is not a valid age public key",
                entry.label
            )
        })?;
        recipients.push(recipient);
    }

    let free = picker.free_text.trim();
    if !free.is_empty() {
        recipients.push(
            free.parse::<Recipient>()
                .map_err(|_| MSG_BAD_KEY.to_string())?,
        );
    }

    if recipients.is_empty() {
        return Err(MSG_PICK_RECIPIENTS.to_string());
    }
    Ok(recipients)
}

/// The Text tab's state.
pub struct TextTabState {
    pub direction: Direction,
    pub input: String,
    pub output: String,
    pub picker: RecipientPicker,
}

impl Default for TextTabState {
    fn default() -> Self {
        Self {
            direction: Direction::Encrypt,
            input: String::new(),
            output: String::new(),
            picker: RecipientPicker::default(),
        }
    }
}

/// Builds the Text tab's job, or the reason the action button is disabled.
pub fn text_job(state: &AppState) -> Result<JobRequest, String> {
    let text = &state.text;
    if text.input.trim().is_empty() {
        return Err(MSG_EMPTY_TEXT.to_string());
    }
    match text.direction {
        Direction::Encrypt => {
            let recipients = effective_recipients(&state.book, &text.picker)?;
            Ok(JobRequest::EncryptText {
                text: text.input.clone(),
                recipients,
            })
        }
        Direction::Decrypt => {
            let identity_path = state
                .identity_path
                .clone()
                .ok_or_else(|| MSG_PICK_IDENTITY.to_string())?;
            Ok(JobRequest::DecryptText {
                text: text.input.clone(),
                identity_path,
            })
        }
    }
}

/// One listed file and where it stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowStatus {
    Pending,
    Invalid(String),
    Running,
    /// Written; carries the output's size in bytes.
    Done(u64),
    Failed(String),
    /// The output existed and the overwrite gate was off.
    Skipped,
}

#[derive(Debug, Clone)]
pub struct FileRow {
    pub path: PathBuf,
    pub size: u64,
    pub status: RowStatus,
    /// The computed output already exists; with the overwrite gate off the
    /// run will skip this row.
    pub output_exists: bool,
}

/// Where the Files tab writes its outputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputChoice {
    /// Next to each input, via the shared naming rule.
    NextToInput,
    /// Into the chosen folder, keeping the derived file name.
    Folder,
}

/// What the intake classified a path as; the caller probes the filesystem,
/// so this module stays pure.
pub enum PathKind {
    File { size: u64 },
    Directory,
}

#[derive(Debug, PartialEq, Eq)]
pub enum AddOutcome {
    Added,
    DuplicateIgnored,
    DirectoryRejected,
}

/// The Files tab's state.
pub struct FilesTabState {
    pub mode: Direction,
    pub rows: Vec<FileRow>,
    pub out_choice: OutputChoice,
    pub out_dir: Option<PathBuf>,
    pub overwrite: bool,
    pub picker: RecipientPicker,
    /// A drag is hovering the window; the drop zone highlights.
    pub drop_hover: bool,
    /// The computed outputs changed; the app refreshes the output_exists
    /// flags outside the render pass.
    pub exists_refresh_pending: bool,
}

impl Default for FilesTabState {
    fn default() -> Self {
        Self {
            mode: Direction::Encrypt,
            rows: Vec::new(),
            out_choice: OutputChoice::NextToInput,
            out_dir: None,
            overwrite: false,
            picker: RecipientPicker::default(),
            drop_hover: false,
            exists_refresh_pending: false,
        }
    }
}

impl FilesTabState {
    /// Adds one picked or dropped path: directories are rejected, duplicate
    /// paths ignored, everything else enters validated against the current
    /// mode.
    pub fn add_path(&mut self, path: PathBuf, kind: PathKind) -> AddOutcome {
        let size = match kind {
            PathKind::Directory => return AddOutcome::DirectoryRejected,
            PathKind::File { size } => size,
        };
        if self.rows.iter().any(|row| row.path == path) {
            return AddOutcome::DuplicateIgnored;
        }
        let status = row_status_for(&path, self.mode);
        self.rows.push(FileRow {
            path,
            size,
            status,
            output_exists: false,
        });
        self.exists_refresh_pending = true;
        AddOutcome::Added
    }

    /// Re-marks every row against the current mode, dropping stale results.
    pub fn revalidate(&mut self) {
        for row in &mut self.rows {
            row.status = row_status_for(&row.path, self.mode);
        }
        self.exists_refresh_pending = true;
    }

    /// Resets results so the new run's events paint on a clean slate. Only
    /// callable through a validated job, which excludes invalid rows.
    pub fn begin_run(&mut self) {
        for row in &mut self.rows {
            row.status = RowStatus::Pending;
        }
    }

    pub fn has_invalid_rows(&self) -> bool {
        self.rows
            .iter()
            .any(|row| matches!(row.status, RowStatus::Invalid(_)))
    }
}

/// A row is valid when the shared naming rule can derive its output: any
/// named file in Seal mode, a `.age` file (any casing) in Unseal mode.
fn row_status_for(path: &Path, mode: Direction) -> RowStatus {
    if output_name_for(path, mode).is_ok() {
        RowStatus::Pending
    } else {
        let message = match mode {
            Direction::Decrypt => MSG_ROW_NEEDS_AGE,
            Direction::Encrypt => MSG_ROW_UNNAMEABLE,
        };
        RowStatus::Invalid(message.to_string())
    }
}

/// Refreshes each row's output_exists flag through an injected existence
/// probe, so the check is testable without touching a filesystem.
pub fn mark_existing_outputs(files: &mut FilesTabState, exists: impl Fn(&Path) -> bool) {
    let out_dir = match files.out_choice {
        OutputChoice::Folder => files.out_dir.clone(),
        OutputChoice::NextToInput => None,
    };
    for row in &mut files.rows {
        row.output_exists = resolve_output_path(&row.path, files.mode, out_dir.as_deref())
            .map(|output| exists(&output))
            .unwrap_or(false);
    }
}

/// Builds the Files tab's batch job, or the reason the Run button is
/// disabled.
pub fn files_job(state: &AppState) -> Result<JobRequest, String> {
    let files = &state.files;
    if files.rows.is_empty() {
        return Err(MSG_NO_FILES.to_string());
    }
    if files.has_invalid_rows() {
        return Err(MSG_INVALID_ROWS.to_string());
    }
    let out_dir = match files.out_choice {
        OutputChoice::NextToInput => None,
        OutputChoice::Folder => Some(
            files
                .out_dir
                .clone()
                .ok_or_else(|| MSG_PICK_FOLDER.to_string())?,
        ),
    };
    let paths: Vec<PathBuf> = files.rows.iter().map(|row| row.path.clone()).collect();

    match files.mode {
        Direction::Encrypt => {
            let recipients = effective_recipients(&state.book, &files.picker)?;
            Ok(JobRequest::EncryptFiles {
                files: paths,
                recipients,
                out_dir,
                overwrite: files.overwrite,
            })
        }
        Direction::Decrypt => {
            let identity_path = state
                .identity_path
                .clone()
                .ok_or_else(|| MSG_PICK_IDENTITY.to_string())?;
            Ok(JobRequest::DecryptFiles {
                files: paths,
                identity_path,
                out_dir,
                overwrite: files.overwrite,
            })
        }
    }
}

/// The Keys tab's state.
#[derive(Default)]
pub struct KeysTabState {
    /// The last generated key pair's public half, shown until replaced.
    pub generated_public_key: Option<String>,
    /// Where the last keygen saved; read for the Done toast and the
    /// add-to-book label prefill.
    pub saved_identity_path: Option<PathBuf>,
    /// The armed row of the two-click delete confirm.
    pub confirm_delete: Option<usize>,
    pub add_label: String,
    pub add_key: String,
    pub add_error: Option<String>,
}

impl KeysTabState {
    /// The two-click delete: the first click arms the row, a second click on
    /// the same row confirms. Returns true when the delete should happen.
    pub fn press_delete(&mut self, index: usize) -> bool {
        if self.confirm_delete == Some(index) {
            self.confirm_delete = None;
            true
        } else {
            self.confirm_delete = Some(index);
            false
        }
    }

    /// Any interaction other than the confirming click disarms the pending
    /// delete.
    pub fn disarm(&mut self) {
        self.confirm_delete = None;
    }
}

/// Validates and appends a book entry: non-empty unique label, parseable
/// key. The book is trusted input, so "Alice" must mean exactly one key.
pub fn add_book_entry(
    book: &mut Vec<RecipientEntry>,
    label: &str,
    key: &str,
) -> Result<(), String> {
    let label = label.trim();
    let key = key.trim();
    if label.is_empty() {
        return Err("the label cannot be empty".to_string());
    }
    if book.iter().any(|entry| entry.label == label) {
        return Err(format!(
            "\"{label}\" is already in the book; labels must be unique"
        ));
    }
    if key.parse::<Recipient>().is_err() {
        return Err(MSG_BAD_KEY.to_string());
    }
    book.push(RecipientEntry {
        label: label.to_string(),
        public_key: key.to_string(),
    });
    Ok(())
}

/// Shortens an `age1...` key for list rows; the full key stays one Copy
/// click away.
pub fn truncated_key(key: &str) -> String {
    const HEAD: usize = 12;
    const TAIL: usize = 6;
    if key.len() <= HEAD + TAIL + 3 || !key.is_ascii() {
        return key.to_string();
    }
    format!("{}...{}", &key[..HEAD], &key[key.len() - TAIL..])
}

/// The Files run summary toast: wording and severity from the batch totals.
pub fn files_summary(
    direction: Direction,
    done: usize,
    skipped: usize,
    failed: usize,
) -> (ToastKind, String) {
    let verb = match direction {
        Direction::Encrypt => "sealed",
        Direction::Decrypt => "opened",
    };
    let kind = if failed > 0 {
        ToastKind::Err
    } else if skipped > 0 {
        ToastKind::Warn
    } else {
        ToastKind::Ok
    };
    (
        kind,
        format!("{done} {verb}, {skipped} skipped, {failed} failed"),
    )
}

/// Applies one worker event to the state: the only place job events mutate
/// the UI model.
pub fn apply_event(state: &mut AppState, event: JobEvent) {
    match event {
        JobEvent::Started { job_id } => {
            debug!(job_id, "background job started");
            state.busy = true;
            state.progress = None;
        }
        JobEvent::FileProgress {
            job_id,
            path,
            done,
            total,
        } => {
            debug!(job_id, path = %path.display(), done, total, "background job progress");
            state.progress = Some((done, total));
            set_row_status(&mut state.files, &path, RowStatus::Running);
        }
        JobEvent::FileFinished {
            job_id,
            path,
            result,
        } => {
            let status = match result {
                FileResult::Done(size) => RowStatus::Done(size),
                FileResult::Skipped => RowStatus::Skipped,
                FileResult::Failed(error_text) => {
                    error!(job_id, path = %path.display(), error = %error_text, "a file failed in the background job");
                    RowStatus::Failed(error_text)
                }
            };
            set_row_status(&mut state.files, &path, status);
        }
        JobEvent::Done { job_id, outcome } => {
            debug!(job_id, "background job finished");
            state.busy = false;
            state.progress = None;
            state.active_job = None;
            apply_outcome(state, outcome);
        }
        JobEvent::Failed { job_id, error_text } => {
            state.busy = false;
            state.progress = None;
            error!(job_id, error = %error_text, "background job failed");
            match state.active_job.take() {
                // Poisoned output: plaintext may already have streamed into
                // the box before the failure was detected.
                Some(JobKind::Text) => state.text.output.clear(),
                // A job-level failure (not per-file) means the batch never
                // ran to completion; un-stick any row left mid-flight.
                Some(JobKind::Files) => {
                    for row in &mut state.files.rows {
                        if row.status == RowStatus::Running {
                            row.status = RowStatus::Pending;
                        }
                    }
                }
                _ => {}
            }
            state
                .toasts
                .push(Toast::new(ToastKind::Err, error_text, TOAST_TTL));
        }
    }
}

/// Routes a finished job's outcome into the tab that asked for it.
fn apply_outcome(state: &mut AppState, outcome: JobOutcome) {
    match outcome {
        #[cfg(test)]
        JobOutcome::Probe => {
            state.toasts.push(Toast::new(
                ToastKind::Ok,
                "The worker replied; the job pipeline works.",
                TOAST_TTL,
            ));
        }
        JobOutcome::Text(text) => {
            state.text.output = text;
        }
        JobOutcome::TextRedirect => {
            state.text.output.clear();
            state
                .toasts
                .push(Toast::new(ToastKind::Warn, MSG_NOT_TEXT, TOAST_TTL));
        }
        JobOutcome::Keygen { public_key } => {
            state.keys.generated_public_key = Some(public_key);
            let saved_to = state.keys.saved_identity_path.as_ref().map_or_else(
                || "the chosen file".to_string(),
                |p| p.display().to_string(),
            );
            state.toasts.push(Toast::new(
                ToastKind::Ok,
                format!("identity saved to {saved_to}"),
                TOAST_TTL,
            ));
        }
        JobOutcome::Files {
            direction,
            done,
            skipped,
            failed,
        } => {
            state.files.exists_refresh_pending = true;
            let (kind, text) = files_summary(direction, done, skipped, failed);
            state.toasts.push(Toast::new(kind, text, TOAST_TTL));
        }
    }
}

fn set_row_status(files: &mut FilesTabState, path: &Path, status: RowStatus) {
    if let Some(row) = files.rows.iter_mut().find(|row| row.path == path) {
        row.status = status;
    }
}

/// Marks the worker as gone after its event channel disconnected (the thread
/// panicked or exited). Idempotent, because the drain reports the disconnect
/// on every following frame while the user should hear about it once.
pub fn on_worker_gone(state: &mut AppState) {
    if state.worker_gone {
        return;
    }
    state.worker_gone = true;
    state.busy = false;
    state.progress = None;
    state.active_job = None;
    error!("the background worker stopped; jobs need an app restart");
    state.toasts.push(Toast::new(
        ToastKind::Err,
        "The background worker stopped. Restart the app to run jobs.",
        TOAST_TTL,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use shared_crypto::keys::{generate_identity, public_key_of};

    fn fresh_key() -> String {
        public_key_of(&generate_identity())
    }

    fn book_of(labels: &[&str]) -> Vec<RecipientEntry> {
        labels
            .iter()
            .map(|label| RecipientEntry {
                label: (*label).to_string(),
                public_key: fresh_key(),
            })
            .collect()
    }

    fn file_row(path: &str) -> FileRow {
        FileRow {
            path: PathBuf::from(path),
            size: 0,
            status: RowStatus::Pending,
            output_exists: false,
        }
    }

    mod events {
        use super::*;

        #[test]
        fn started_sets_busy_and_clears_stale_progress() {
            let mut state = AppState {
                progress: Some((1, 2)),
                ..AppState::default()
            };

            apply_event(&mut state, JobEvent::Started { job_id: 1 });

            assert!(state.busy);
            assert_eq!(state.progress, None);
            assert!(state.toasts.is_empty());
        }

        #[test]
        fn file_progress_updates_the_counters_and_marks_the_row_running() {
            let mut state = AppState::default();
            state.files.rows.push(file_row("a.txt"));
            state.files.rows.push(file_row("b.txt"));

            apply_event(
                &mut state,
                JobEvent::FileProgress {
                    job_id: 1,
                    path: PathBuf::from("b.txt"),
                    done: 1,
                    total: 2,
                },
            );

            assert_eq!(state.progress, Some((1, 2)));
            assert_eq!(state.files.rows[0].status, RowStatus::Pending);
            assert_eq!(state.files.rows[1].status, RowStatus::Running);
        }

        #[test]
        fn file_finished_routes_each_result_to_its_row_by_path() {
            let mut state = AppState::default();
            state.files.rows.push(file_row("done.txt"));
            state.files.rows.push(file_row("failed.txt"));
            state.files.rows.push(file_row("skipped.txt"));

            apply_event(
                &mut state,
                JobEvent::FileFinished {
                    job_id: 1,
                    path: PathBuf::from("done.txt"),
                    result: FileResult::Done(42),
                },
            );
            apply_event(
                &mut state,
                JobEvent::FileFinished {
                    job_id: 1,
                    path: PathBuf::from("failed.txt"),
                    result: FileResult::Failed("boom".to_string()),
                },
            );
            apply_event(
                &mut state,
                JobEvent::FileFinished {
                    job_id: 1,
                    path: PathBuf::from("skipped.txt"),
                    result: FileResult::Skipped,
                },
            );

            assert_eq!(state.files.rows[0].status, RowStatus::Done(42));
            assert_eq!(
                state.files.rows[1].status,
                RowStatus::Failed("boom".to_string())
            );
            assert_eq!(state.files.rows[2].status, RowStatus::Skipped);
        }

        #[test]
        fn a_text_outcome_fills_the_output_box_without_a_toast() {
            let mut state = AppState {
                busy: true,
                active_job: Some(JobKind::Text),
                ..AppState::default()
            };

            apply_event(
                &mut state,
                JobEvent::Done {
                    job_id: 1,
                    outcome: JobOutcome::Text("armored".to_string()),
                },
            );

            assert!(!state.busy);
            assert_eq!(state.active_job, None);
            assert_eq!(state.text.output, "armored");
            assert!(state.toasts.is_empty());
        }

        #[test]
        fn a_redirect_outcome_clears_the_output_and_warns() {
            let mut state = AppState::default();
            state.text.output = "stale".to_string();

            apply_event(
                &mut state,
                JobEvent::Done {
                    job_id: 1,
                    outcome: JobOutcome::TextRedirect,
                },
            );

            assert!(state.text.output.is_empty());
            assert_eq!(state.toasts.len(), 1);
            assert!(matches!(state.toasts[0].kind, ToastKind::Warn));
            assert_eq!(state.toasts[0].text, MSG_NOT_TEXT);
        }

        #[test]
        fn a_keygen_outcome_shows_the_key_and_names_the_saved_path() {
            let mut state = AppState::default();
            state.keys.saved_identity_path = Some(PathBuf::from("id.txt"));

            apply_event(
                &mut state,
                JobEvent::Done {
                    job_id: 1,
                    outcome: JobOutcome::Keygen {
                        public_key: "age1newkey".to_string(),
                    },
                },
            );

            assert_eq!(
                state.keys.generated_public_key,
                Some("age1newkey".to_string())
            );
            assert_eq!(state.toasts.len(), 1);
            assert!(matches!(state.toasts[0].kind, ToastKind::Ok));
            assert!(state.toasts[0].text.contains("id.txt"));
        }

        #[test]
        fn a_files_outcome_toasts_the_summary_and_schedules_an_exists_refresh() {
            let mut state = AppState::default();

            apply_event(
                &mut state,
                JobEvent::Done {
                    job_id: 1,
                    outcome: JobOutcome::Files {
                        direction: Direction::Encrypt,
                        done: 7,
                        skipped: 1,
                        failed: 0,
                    },
                },
            );

            assert!(state.files.exists_refresh_pending);
            assert_eq!(state.toasts.len(), 1);
            assert!(matches!(state.toasts[0].kind, ToastKind::Warn));
            assert_eq!(state.toasts[0].text, "7 sealed, 1 skipped, 0 failed");
        }

        #[test]
        fn a_failed_text_job_poisons_the_output_box() {
            let mut state = AppState {
                busy: true,
                active_job: Some(JobKind::Text),
                ..AppState::default()
            };
            state.text.output = "partial plaintext".to_string();

            apply_event(
                &mut state,
                JobEvent::Failed {
                    job_id: 1,
                    error_text: "it broke".to_string(),
                },
            );

            assert!(!state.busy);
            assert!(state.text.output.is_empty());
            assert_eq!(state.toasts.len(), 1);
            assert!(matches!(state.toasts[0].kind, ToastKind::Err));
            assert_eq!(state.toasts[0].text, "it broke");
        }

        #[test]
        fn a_failed_files_job_unsticks_running_rows() {
            let mut state = AppState {
                active_job: Some(JobKind::Files),
                ..AppState::default()
            };
            state.files.rows.push(FileRow {
                status: RowStatus::Running,
                ..file_row("a.txt")
            });
            state.files.rows.push(FileRow {
                status: RowStatus::Done(5),
                ..file_row("b.txt")
            });

            apply_event(
                &mut state,
                JobEvent::Failed {
                    job_id: 1,
                    error_text: "identity file unreadable".to_string(),
                },
            );

            assert_eq!(state.files.rows[0].status, RowStatus::Pending);
            assert_eq!(state.files.rows[1].status, RowStatus::Done(5));
        }

        #[test]
        fn a_failed_job_outside_text_and_files_only_toasts() {
            let mut state = AppState {
                busy: true,
                active_job: Some(JobKind::Keygen),
                ..AppState::default()
            };
            state.text.output = "keep me".to_string();

            apply_event(
                &mut state,
                JobEvent::Failed {
                    job_id: 1,
                    error_text: "disk full".to_string(),
                },
            );

            assert_eq!(state.text.output, "keep me");
            assert_eq!(state.active_job, None);
            assert_eq!(state.toasts.len(), 1);
        }

        #[test]
        fn a_keygen_outcome_without_a_remembered_path_still_toasts() {
            let mut state = AppState::default();

            apply_event(
                &mut state,
                JobEvent::Done {
                    job_id: 1,
                    outcome: JobOutcome::Keygen {
                        public_key: "age1newkey".to_string(),
                    },
                },
            );

            assert!(state.toasts[0].text.contains("the chosen file"));
        }

        #[test]
        fn the_probe_outcome_toasts_the_bridge_proof() {
            let mut state = AppState::default();

            apply_event(
                &mut state,
                JobEvent::Done {
                    job_id: 1,
                    outcome: JobOutcome::Probe,
                },
            );

            assert_eq!(state.toasts.len(), 1);
            assert!(matches!(state.toasts[0].kind, ToastKind::Ok));
        }

        #[test]
        fn worker_gone_reports_once_and_stays_set() {
            let mut state = AppState {
                busy: true,
                ..AppState::default()
            };

            on_worker_gone(&mut state);
            on_worker_gone(&mut state);

            assert!(state.worker_gone);
            assert!(!state.busy);
            assert_eq!(state.toasts.len(), 1);
            assert!(matches!(state.toasts[0].kind, ToastKind::Err));
        }
    }

    mod picker {
        use super::*;

        #[test]
        fn effective_recipients_merges_checked_entries_with_the_free_key() {
            let book = book_of(&["Alice", "Bea", "Cho"]);
            let picker = RecipientPicker {
                selected: vec![true, false, true],
                free_text: format!("  {}  ", fresh_key()),
                free_error: None,
            };

            let recipients = effective_recipients(&book, &picker).unwrap();

            assert_eq!(recipients.len(), 3);
        }

        #[test]
        fn nothing_selected_and_no_free_text_blocks_with_the_guidance() {
            let book = book_of(&["Alice"]);
            let picker = RecipientPicker {
                selected: vec![false],
                ..RecipientPicker::default()
            };

            let error = effective_recipients(&book, &picker).unwrap_err();

            assert_eq!(error, MSG_PICK_RECIPIENTS);
        }

        #[test]
        fn an_invalid_free_key_blocks_with_the_key_message() {
            let picker = RecipientPicker {
                free_text: "age1-not-a-key".to_string(),
                ..RecipientPicker::default()
            };

            let error = effective_recipients(&[], &picker).unwrap_err();

            assert_eq!(error, MSG_BAD_KEY);
        }

        #[test]
        fn a_corrupted_book_entry_is_reported_by_label() {
            let book = vec![RecipientEntry {
                label: "Alice".to_string(),
                public_key: "hand-edited garbage".to_string(),
            }];
            let picker = RecipientPicker {
                selected: vec![true],
                ..RecipientPicker::default()
            };

            let error = effective_recipients(&book, &picker).unwrap_err();

            assert!(error.contains("Alice"));
        }

        #[test]
        fn free_text_validation_flags_bad_keys_and_clears_on_empty() {
            let mut picker = RecipientPicker {
                free_text: "garbage".to_string(),
                ..RecipientPicker::default()
            };
            picker.validate_free_text();
            assert_eq!(picker.free_error, Some(MSG_BAD_KEY.to_string()));

            picker.free_text = fresh_key();
            picker.validate_free_text();
            assert_eq!(picker.free_error, None);

            picker.free_text = "   ".to_string();
            picker.validate_free_text();
            assert_eq!(picker.free_error, None);
        }

        #[test]
        fn sync_with_book_grows_unchecked_and_shrinks_with_the_book() {
            let mut picker = RecipientPicker {
                selected: vec![true],
                ..RecipientPicker::default()
            };

            picker.sync_with_book(3);
            assert_eq!(picker.selected, vec![true, false, false]);

            picker.sync_with_book(1);
            assert_eq!(picker.selected, vec![true]);
        }
    }

    mod file_rows {
        use super::*;

        #[rstest]
        #[case::lowercase("report.pdf.age", true)]
        #[case::uppercase("report.pdf.AGE", true)]
        #[case::no_suffix("report.pdf", false)]
        #[case::nothing_but_the_suffix(".age", false)]
        fn unseal_mode_accepts_only_age_files(#[case] name: &str, #[case] valid: bool) {
            let mut files = FilesTabState {
                mode: Direction::Decrypt,
                ..FilesTabState::default()
            };

            files.add_path(PathBuf::from(name), PathKind::File { size: 1 });

            let status = &files.rows[0].status;
            assert_eq!(matches!(status, RowStatus::Pending), valid, "{status:?}");
            if !valid {
                assert_eq!(status, &RowStatus::Invalid(MSG_ROW_NEEDS_AGE.to_string()));
            }
        }

        #[test]
        fn seal_mode_rejects_a_nameless_path() {
            let mut files = FilesTabState::default();

            files.add_path(PathBuf::from(".."), PathKind::File { size: 0 });

            assert_eq!(
                files.rows[0].status,
                RowStatus::Invalid(MSG_ROW_UNNAMEABLE.to_string())
            );
        }

        #[test]
        fn directories_are_rejected_and_duplicates_ignored() {
            let mut files = FilesTabState::default();

            assert_eq!(
                files.add_path(PathBuf::from("a dir"), PathKind::Directory),
                AddOutcome::DirectoryRejected
            );
            assert_eq!(
                files.add_path(PathBuf::from("a.txt"), PathKind::File { size: 9 }),
                AddOutcome::Added
            );
            assert_eq!(
                files.add_path(PathBuf::from("a.txt"), PathKind::File { size: 9 }),
                AddOutcome::DuplicateIgnored
            );

            assert_eq!(files.rows.len(), 1);
        }

        #[test]
        fn switching_mode_remarks_every_row_and_drops_stale_results() {
            let mut files = FilesTabState::default();
            files.add_path(PathBuf::from("plain.txt"), PathKind::File { size: 1 });
            files.add_path(PathBuf::from("sealed.age"), PathKind::File { size: 1 });
            files.rows[0].status = RowStatus::Done(10);

            files.mode = Direction::Decrypt;
            files.revalidate();

            assert_eq!(
                files.rows[0].status,
                RowStatus::Invalid(MSG_ROW_NEEDS_AGE.to_string())
            );
            assert_eq!(files.rows[1].status, RowStatus::Pending);
        }

        #[test]
        fn begin_run_resets_results_to_pending() {
            let mut files = FilesTabState::default();
            files.add_path(PathBuf::from("a.txt"), PathKind::File { size: 1 });
            files.rows[0].status = RowStatus::Failed("old".to_string());

            files.begin_run();

            assert_eq!(files.rows[0].status, RowStatus::Pending);
        }

        #[test]
        fn mark_existing_outputs_probes_the_computed_path_for_each_mode() {
            let mut files = FilesTabState::default();
            files.add_path(PathBuf::from("a.txt"), PathKind::File { size: 1 });
            files.add_path(PathBuf::from("b.txt"), PathKind::File { size: 1 });

            mark_existing_outputs(&mut files, |path| path == Path::new("a.txt.age"));
            assert!(files.rows[0].output_exists);
            assert!(!files.rows[1].output_exists);

            files.out_choice = OutputChoice::Folder;
            files.out_dir = Some(PathBuf::from("safe"));
            mark_existing_outputs(&mut files, |path| {
                path == Path::new("safe").join("b.txt.age").as_path()
            });
            assert!(!files.rows[0].output_exists);
            assert!(files.rows[1].output_exists);
        }
    }

    mod jobs {
        use super::*;

        /// `unwrap_err` needs `JobRequest: Debug`, which it deliberately
        /// lacks (`Recipient` has no Debug impl), so errors are extracted by
        /// match instead.
        fn blocked_reason(result: Result<JobRequest, String>) -> String {
            match result {
                Ok(_) => panic!("expected the job to be blocked"),
                Err(reason) => reason,
            }
        }

        fn ready_text_state() -> AppState {
            let mut state = AppState {
                book: book_of(&["Alice"]),
                ..AppState::default()
            };
            state.text.input = "hello".to_string();
            state.text.picker.selected = vec![true];
            state
        }

        #[test]
        fn text_job_builds_encrypt_with_the_picked_recipients() {
            let state = ready_text_state();

            let job = text_job(&state).unwrap();

            assert!(matches!(
                job,
                JobRequest::EncryptText { text, recipients }
                    if text == "hello" && recipients.len() == 1
            ));
        }

        #[test]
        fn text_job_blocks_on_empty_input_or_missing_identity() {
            let mut state = ready_text_state();
            state.text.input = "  ".to_string();
            assert_eq!(blocked_reason(text_job(&state)), MSG_EMPTY_TEXT);

            let mut state = ready_text_state();
            state.text.direction = Direction::Decrypt;
            assert_eq!(blocked_reason(text_job(&state)), MSG_PICK_IDENTITY);

            state.identity_path = Some(PathBuf::from("id.txt"));
            assert!(matches!(
                text_job(&state).unwrap(),
                JobRequest::DecryptText { identity_path, .. }
                    if identity_path == Path::new("id.txt")
            ));
        }

        fn ready_files_state() -> AppState {
            let mut state = AppState {
                book: book_of(&["Alice"]),
                ..AppState::default()
            };
            state.files.picker.selected = vec![true];
            state
                .files
                .add_path(PathBuf::from("a.txt"), PathKind::File { size: 1 });
            state
        }

        #[test]
        fn files_job_blocks_on_empty_invalid_or_folderless_runs() {
            let state = AppState::default();
            assert_eq!(blocked_reason(files_job(&state)), MSG_NO_FILES);

            let mut state = ready_files_state();
            state.files.mode = Direction::Decrypt;
            state.files.revalidate();
            assert_eq!(blocked_reason(files_job(&state)), MSG_INVALID_ROWS);

            let mut state = ready_files_state();
            state.files.out_choice = OutputChoice::Folder;
            assert_eq!(blocked_reason(files_job(&state)), MSG_PICK_FOLDER);
        }

        #[test]
        fn files_job_builds_a_decrypt_batch_next_to_the_inputs() {
            let mut state = AppState {
                identity_path: Some(PathBuf::from("id.txt")),
                ..AppState::default()
            };
            state.files.mode = Direction::Decrypt;
            state
                .files
                .add_path(PathBuf::from("a.txt.age"), PathKind::File { size: 1 });

            assert!(matches!(
                files_job(&state),
                Ok(JobRequest::DecryptFiles { files, identity_path, out_dir: None, overwrite: false })
                    if files == vec![PathBuf::from("a.txt.age")]
                        && identity_path == Path::new("id.txt")
            ));
        }

        #[test]
        fn files_job_builds_the_batch_with_the_gate_and_folder() {
            let mut state = ready_files_state();
            state.files.overwrite = true;
            state.files.out_choice = OutputChoice::Folder;
            state.files.out_dir = Some(PathBuf::from("safe"));

            let job = files_job(&state).unwrap();

            assert!(matches!(
                job,
                JobRequest::EncryptFiles { files, recipients, out_dir, overwrite }
                    if files == vec![PathBuf::from("a.txt")]
                        && recipients.len() == 1
                        && out_dir == Some(PathBuf::from("safe"))
                        && overwrite
            ));
        }
    }

    mod keys_tab {
        use super::*;

        #[test]
        fn delete_arms_then_confirms_on_the_same_row() {
            let mut keys = KeysTabState::default();

            assert!(!keys.press_delete(2));
            assert_eq!(keys.confirm_delete, Some(2));
            assert!(keys.press_delete(2));
            assert_eq!(keys.confirm_delete, None);
        }

        #[test]
        fn pressing_another_row_rearms_instead_of_deleting() {
            let mut keys = KeysTabState::default();

            assert!(!keys.press_delete(0));
            assert!(!keys.press_delete(1));
            assert_eq!(keys.confirm_delete, Some(1));
        }

        #[test]
        fn any_other_interaction_disarms() {
            let mut keys = KeysTabState::default();
            keys.press_delete(0);

            keys.disarm();

            assert_eq!(keys.confirm_delete, None);
        }

        #[test]
        fn add_book_entry_validates_label_and_key() {
            let mut book = book_of(&["Alice"]);
            let key = fresh_key();

            assert!(add_book_entry(&mut book, "  ", &key).is_err());
            let duplicate = add_book_entry(&mut book, "Alice", &key).unwrap_err();
            assert!(duplicate.contains("unique"));
            assert_eq!(
                add_book_entry(&mut book, "Bea", "nonsense").unwrap_err(),
                MSG_BAD_KEY
            );

            add_book_entry(&mut book, "  Bea  ", &format!(" {key} ")).unwrap();
            assert_eq!(book.len(), 2);
            assert_eq!(book[1].label, "Bea");
            assert_eq!(book[1].public_key, key);
        }
    }

    mod summaries {
        use super::*;

        #[rstest]
        #[case::all_good(0, 0, ToastKind::Ok)]
        #[case::some_skipped(1, 0, ToastKind::Warn)]
        #[case::any_failure(1, 1, ToastKind::Err)]
        fn files_summary_grades_by_the_worst_result(
            #[case] skipped: usize,
            #[case] failed: usize,
            #[case] expected: ToastKind,
        ) {
            let (kind, text) = files_summary(Direction::Encrypt, 7, skipped, failed);

            assert_eq!(kind, expected);
            assert_eq!(
                text,
                format!("7 sealed, {skipped} skipped, {failed} failed")
            );
        }

        #[test]
        fn decrypt_summaries_say_opened() {
            let (_, text) = files_summary(Direction::Decrypt, 2, 0, 0);

            assert_eq!(text, "2 opened, 0 skipped, 0 failed");
        }

        #[test]
        fn truncated_key_shortens_long_keys_and_keeps_short_ones() {
            let long = "age1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq";

            let shown = truncated_key(long);

            assert_eq!(shown, "age1qqqqqqqq...qqqqqq");
            assert_eq!(truncated_key("age1short"), "age1short");
        }
    }
}

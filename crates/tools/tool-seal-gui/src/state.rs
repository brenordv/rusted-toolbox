//! What the seal window holds, and how each input changes it. `apply` is pure: it reads no
//! file, clock, or window, and hands what it needs done back as effects.

pub mod files;
mod files_flow;
pub mod notes;
pub mod recipients;
pub mod text;

use crate::config::SealConfig;
use crate::design::ChoiceKits;
use crate::services::{PostGate, Services};
use crate::worker::{FileResult, JobEvent, JobOutcome, JobRequest, WorkerMsg};
use common_cli::app_logger::FileLogging;
use common_gui_rune::Theme;
use common_gui_rune::parts::Tone;
use files::{FilesInput, FilesTab, PathKind, RowStatus};
use notes::{Copied, Missing, Note, Notes, RunningJob};
use recipients::{Book, EntryId, Picker, RecipientPicker};
use rune::FileDialogResult;
use shared_crypto::Direction;
use std::fmt;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use text::{TextInput, TextTab, text_job};

/// The window's three areas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    /// Seal a text to recipients as armor, or unseal armored text.
    Text,
    /// Seal or unseal files.
    Files,
    /// The key pair and the recipient book.
    Keys,
}

impl Tab {
    /// The tabs in the order the switch shows them.
    pub const ALL: [Self; 3] = [Self::Text, Self::Files, Self::Keys];
    /// The switch's labels, in [`Tab::ALL`]'s order.
    pub const LABELS: [&'static str; 3] = ["Text", "Files", "Keys"];

    /// The tab's place in [`Tab::ALL`].
    pub fn index(self) -> usize {
        match self {
            Self::Text => 0,
            Self::Files => 1,
            Self::Keys => 2,
        }
    }

    /// The tab's name as the window shows it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::Files => "Files",
            Self::Keys => "Keys",
        }
    }
}

/// The worker as the window sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Work {
    /// Nothing runs; a tab can start its job.
    Idle,
    /// A job runs; its final answer returns the worker to idle.
    Running(RunningJob),
    /// The worker thread ended; no job can run again this session.
    Stopped,
}

/// Why a tab cannot start its job now, for the guard and for the view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blocked {
    /// A job runs.
    Running(RunningJob),
    /// The worker stopped.
    Stopped,
}

/// A file dialog's purpose. One shows at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogPurpose {
    /// Choose the identity file, from the picker's tab.
    Identity(Picker),
    /// Add a file to the Files list.
    AddFile,
    /// Choose the Files tab's output folder.
    OutputFolder,
}

impl DialogPurpose {
    /// The tab whose button opened the dialog.
    pub fn tab(self) -> Tab {
        match self {
            Self::Identity(picker) => picker.tab(),
            Self::AddFile | Self::OutputFolder => Tab::Files,
        }
    }

    /// The purpose's name in a log line.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Identity(_) => "identity",
            Self::AddFile => "add-file",
            Self::OutputFolder => "output-folder",
        }
    }
}

/// Where a transition puts focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// The Text tab's input box.
    TextInput,
    /// A recipient's checkbox in the picker named.
    Recipient(Picker, EntryId),
    /// The Other key field of the picker named.
    OtherKey(Picker),
    /// "Choose identity file…" on the picker's tab.
    ChooseIdentity(Picker),
    /// The Files tab's "Add files…".
    AddFiles,
    /// Remove on the Files row at this place in the list.
    RemoveRow(usize),
    /// The Files tab's "Choose folder…".
    ChooseFolder,
}

/// Everything that can happen to the window.
pub enum Input {
    /// A tab was picked.
    TabPicked(Tab),
    /// The user did something on the Text tab.
    Text(TextInput),
    /// The user did something on the Files tab.
    Files(FilesInput),
    /// What the paths an intake asked about are.
    Inspected(Vec<(PathBuf, PathKind)>),
    /// Which rows' outputs exist, by row path.
    OutputsProbed(Vec<(PathBuf, bool)>),
    /// A button asked to open a file dialog.
    DialogOpened(DialogPurpose),
    /// The open dialog answered.
    DialogAnswered(DialogPurpose, FileDialogResult),
    /// An event of the running job.
    Job(JobEvent),
    /// The worker thread ended without being asked to.
    Stopped,
}

impl From<WorkerMsg> for Input {
    fn from(message: WorkerMsg) -> Self {
        match message {
            WorkerMsg::Job(event) => Self::Job(event),
            WorkerMsg::Stopped => Self::Stopped,
        }
    }
}

impl fmt::Debug for Input {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TabPicked(tab) => f.debug_tuple("TabPicked").field(tab).finish(),
            Self::Text(input) => f.debug_tuple("Text").field(input).finish(),
            Self::Files(input) => f.debug_tuple("Files").field(input).finish(),
            Self::Inspected(paths) => f.debug_tuple("Inspected").field(&paths.len()).finish(),
            Self::OutputsProbed(paths) => {
                f.debug_tuple("OutputsProbed").field(&paths.len()).finish()
            }
            Self::DialogOpened(purpose) => f.debug_tuple("DialogOpened").field(purpose).finish(),
            Self::DialogAnswered(purpose, answer) => f
                .debug_struct("DialogAnswered")
                .field("purpose", purpose)
                .field("answered", &answer.as_ref().map(Option::is_some))
                .finish(),
            Self::Job(event) => f.debug_tuple("Job").field(event).finish(),
            Self::Stopped => f.write_str("Stopped"),
        }
    }
}

/// Work for the services, run by `dispatch` after a transition.
pub enum Effect {
    /// Start a job on the worker.
    Run(JobRequest),
    /// Show the file dialog for a purpose.
    OpenDialog(DialogPurpose),
    /// Put text on the clipboard.
    Copy(String),
    /// Find out what these paths are; answered as [`Input::Inspected`].
    Inspect(Vec<PathBuf>),
    /// Find out which outputs exist, for (row path, output path) pairs; answered as
    /// [`Input::OutputsProbed`].
    ProbeOutputs(Vec<(PathBuf, PathBuf)>),
}

impl fmt::Debug for Effect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Run(request) => f.debug_tuple("Run").field(request).finish(),
            Self::OpenDialog(purpose) => f.debug_tuple("OpenDialog").field(purpose).finish(),
            Self::Copy(text) => f.debug_struct("Copy").field("len", &text.len()).finish(),
            Self::Inspect(paths) => f.debug_tuple("Inspect").field(&paths.len()).finish(),
            Self::ProbeOutputs(pairs) => f.debug_tuple("ProbeOutputs").field(&pairs.len()).finish(),
        }
    }
}

/// What a transition did.
#[derive(Debug, Default)]
pub struct Outcome {
    /// Whether the view must re-run.
    pub changed: bool,
    pub effects: Vec<Effect>,
    /// Where focus goes, when the transition names a place.
    pub focus: Option<Focus>,
}

/// The seal window's state.
pub struct AppState {
    pub theme: Theme,
    pub choice_kits: ChoiceKits,
    pub tab: Tab,
    pub work: Work,
    /// The file dialog showing, if any; cleared only by its own answer.
    pub dialog: Option<DialogPurpose>,
    pub book: Book,
    /// The identity file Unseal reads, shared by the Text and Files tabs.
    pub identity_path: Option<PathBuf>,
    pub text: TextTab,
    pub files: FilesTab,
    notes: Notes,
    /// What the log file is doing, for the Keys tab's About group.
    pub file_logging: FileLogging,
    pub(crate) services: Option<Rc<dyn Services>>,
    /// Shut when the window closes, so the worker's late posts go nowhere.
    pub(crate) gate: Option<PostGate>,
}

impl fmt::Debug for AppState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppState")
            .field("tab", &self.tab)
            .field("work", &self.work)
            .field("dialog", &self.dialog)
            .field("book_entries", &self.book.entries().len())
            .field("identity_path", &self.identity_path)
            .field("text", &self.text)
            .field("files", &self.files)
            .field("notes", &self.notes)
            .finish_non_exhaustive()
    }
}

impl AppState {
    /// A window over `config`, starting jobs through `services` and closing `gate` when the
    /// window closes.
    pub fn new(
        config: SealConfig,
        file_logging: FileLogging,
        services: Option<Rc<dyn Services>>,
        gate: Option<PostGate>,
    ) -> Self {
        Self {
            theme: Theme::new(),
            choice_kits: ChoiceKits::new(),
            tab: Tab::Text,
            work: Work::Idle,
            dialog: None,
            book: Book::from_entries(config.recipients),
            identity_path: config.last_identity_file,
            text: TextTab::default(),
            files: FilesTab {
                out_dir: config.last_output_folder,
                overwrite: config.overwrite_existing,
                ..FilesTab::default()
            },
            notes: Notes::default(),
            file_logging,
            services,
            gate,
        }
    }

    /// An empty window with no services, for `AppSpec`'s plain constructor.
    pub fn detached() -> Self {
        Self::new(SealConfig::default(), FileLogging::Off, None, None)
    }

    /// The note `tab` holds, if any.
    pub fn note(&self, tab: Tab) -> Option<&Note> {
        self.notes.get(tab)
    }

    /// What a refused start on `tab` found missing, while its note says so; the view marks
    /// that field.
    pub fn missing(&self, tab: Tab) -> Option<&Missing> {
        match self.notes.get(tab)? {
            Note::Missing(missing) => Some(missing),
            Note::Failure(_)
            | Note::NotText
            | Note::Copied(_)
            | Note::DialogError(_)
            | Note::OtherTabFailed(_)
            | Note::Summary { .. }
            | Note::Skipped(_) => None,
        }
    }

    /// The notes, for a test to set one directly.
    #[cfg(test)]
    pub(crate) fn notes_mut_for_tests(&mut self) -> &mut Notes {
        &mut self.notes
    }

    /// Why no tab can start a job now, if one cannot.
    pub fn blocked(&self) -> Option<Blocked> {
        match self.work {
            Work::Idle => None,
            Work::Running(job) => Some(Blocked::Running(job)),
            Work::Stopped => Some(Blocked::Stopped),
        }
    }

    /// Whether a job of `tab` runs now.
    pub fn running_on(&self, tab: Tab) -> bool {
        match self.work {
            Work::Running(job) => job.tab() == tab,
            Work::Idle | Work::Stopped => false,
        }
    }

    /// What `tab`'s status line shows, first match wins: the job running on this tab, a job
    /// running on another, this tab's note, nothing.
    pub fn status(&self, tab: Tab) -> Option<(Tone, String)> {
        match self.work {
            Work::Running(job) if job.tab() == tab => return Some((Tone::Muted, job.phrase())),
            Work::Running(job) => return Some((Tone::Muted, job.elsewhere())),
            Work::Idle | Work::Stopped => {}
        }
        self.notes.get(tab).map(Note::words)
    }

    /// What assistive technology hears for `tab`'s status while a job runs, when it differs
    /// from what shows: a running batch's count is left out.
    pub fn status_spoken(&self, tab: Tab) -> Option<String> {
        match self.work {
            Work::Running(job) if job.tab() == tab => Some(job.spoken()),
            Work::Running(job) => Some(job.spoken_elsewhere()),
            Work::Idle | Work::Stopped => None,
        }
    }

    /// Applies `input`.
    pub fn apply(&mut self, input: Input) -> Outcome {
        let mut out = Outcome::default();
        let failures_cleared: Vec<Tab> = Tab::ALL
            .into_iter()
            .filter(|tab| match self.notes.get(*tab) {
                Some(note @ Note::Failure(_)) => clears_note(&input, *tab, note),
                Some(_) | None => false,
            })
            .collect();
        if self
            .notes
            .clear_where(|tab, note| clears_note(&input, tab, note))
        {
            out.changed = true;
        }
        // The other tabs' pointers at a failure go with it.
        for tab in failures_cleared {
            self.clear_other_tab_failure(tab);
        }
        match input {
            Input::TabPicked(tab) => self.tab_picked(tab, &mut out),
            Input::Text(input) => self.text_input(input, &mut out),
            Input::Files(input) => self.files_input(input, &mut out),
            Input::Inspected(inspected) => self.inspected(inspected, &mut out),
            Input::OutputsProbed(probed) => {
                self.files.set_outputs(probed);
                out.changed = true;
            }
            Input::DialogOpened(purpose) => self.dialog_opened(purpose, &mut out),
            Input::DialogAnswered(purpose, answer) => {
                self.dialog_answered(purpose, answer, &mut out);
            }
            Input::Job(event) => self.job_event(event, &mut out),
            Input::Stopped => self.stopped(&mut out),
        }
        out
    }

    fn tab_picked(&mut self, tab: Tab, out: &mut Outcome) {
        if self.tab == tab {
            return;
        }
        self.tab = tab;
        self.files.drop_hover = false;
        out.changed = true;
        out.focus = match tab {
            Tab::Text => Some(Focus::TextInput),
            Tab::Files => Some(Focus::AddFiles),
            Tab::Keys => None,
        };
    }

    fn text_input(&mut self, input: TextInput, out: &mut Outcome) {
        match input {
            TextInput::DirectionPicked(direction) => {
                if self.running_on(Tab::Text) || self.text.direction == direction {
                    return;
                }
                self.text.direction = direction;
                self.text.output.clear();
                out.changed = true;
            }
            TextInput::InputEdited => {}
            TextInput::RecipientToggled(id) => {
                if self.book.get(id).is_some() {
                    self.text.picker.toggle(id);
                    out.changed = true;
                }
            }
            TextInput::OtherKeyEdited => {
                self.text.picker.check_other_key();
                out.changed = true;
            }
            TextInput::StartPressed => self.start_text(out),
            TextInput::CopyPressed => {
                // While a Text job runs, the output it will replace is not worth copying, and
                // a "Copied" note would outlive it.
                if self.text.output.is_empty() || self.running_on(Tab::Text) {
                    return;
                }
                out.effects.push(Effect::Copy(self.text.output.clone()));
                self.notes.set(Tab::Text, Note::Copied(Copied::Output));
                out.changed = true;
            }
        }
    }

    fn start_text(&mut self, out: &mut Outcome) {
        if self.blocked().is_some() {
            return;
        }
        match text_job(&self.text, &self.book, self.identity_path.as_ref()) {
            Ok(request) => {
                self.accept_start(RunningJob::Text(self.text.direction), request, out);
            }
            Err(missing) => self.refuse_start(Picker::Text, missing, out),
        }
    }

    /// The recipient choice of `picker`'s tab.
    pub fn picker(&self, picker: Picker) -> &RecipientPicker {
        match picker {
            Picker::Text => &self.text.picker,
            Picker::Files => &self.files.picker,
        }
    }

    /// The field a refused start on `picker`'s tab names, for focus.
    fn focus_for(&self, picker: Picker, missing: &Missing) -> Focus {
        match missing {
            Missing::Input => Focus::TextInput,
            Missing::Recipients => self
                .book
                .entries()
                .first()
                .map_or(Focus::OtherKey(picker), |entry| {
                    Focus::Recipient(picker, entry.id)
                }),
            Missing::SavedKey { id, .. } => Focus::Recipient(picker, *id),
            Missing::OtherKey => Focus::OtherKey(picker),
            Missing::Identity => Focus::ChooseIdentity(picker),
            Missing::Files => Focus::AddFiles,
            Missing::ValidRows(index) => Focus::RemoveRow(*index),
            Missing::Folder => Focus::ChooseFolder,
        }
    }

    fn clear_other_tab_failure(&mut self, started: Tab) {
        self.notes
            .clear_where(|_, note| *note == Note::OtherTabFailed(started));
    }

    /// A start that found something missing: the note says it and focus goes to the field.
    fn refuse_start(&mut self, picker: Picker, missing: Missing, out: &mut Outcome) {
        out.focus = Some(self.focus_for(picker, &missing));
        self.notes.set(picker.tab(), Note::Missing(missing));
        out.changed = true;
    }

    /// A start that can run: the worker is busy with `job`, and the other tabs stop pointing at
    /// this tab's last failure.
    fn accept_start(&mut self, job: RunningJob, request: JobRequest, out: &mut Outcome) {
        self.work = Work::Running(job);
        self.clear_other_tab_failure(job.tab());
        out.effects.push(Effect::Run(request));
        out.changed = true;
    }

    fn dialog_opened(&mut self, purpose: DialogPurpose, out: &mut Outcome) {
        if self.dialog.is_some() {
            return;
        }
        self.dialog = Some(purpose);
        out.effects.push(Effect::OpenDialog(purpose));
        out.changed = true;
    }

    fn dialog_answered(
        &mut self,
        purpose: DialogPurpose,
        answer: FileDialogResult,
        out: &mut Outcome,
    ) {
        if self.dialog != Some(purpose) {
            return;
        }
        self.dialog = None;
        out.changed = true;
        match answer {
            Ok(Some(path)) => match purpose {
                DialogPurpose::Identity(_) => {
                    self.identity_path = Some(path);
                    // Both tabs unseal with this one file, so neither still lacks it.
                    self.notes
                        .clear_where(|_, note| *note == Note::Missing(Missing::Identity));
                }
                DialogPurpose::AddFile => out.effects.push(Effect::Inspect(vec![path])),
                DialogPurpose::OutputFolder => {
                    self.files.out_dir = Some(path);
                    self.files.out_choice = files::OutputChoice::Folder;
                    self.probe_outputs(out);
                }
            },
            Ok(None) => {}
            Err(error) => self.notes.set(purpose.tab(), Note::DialogError(error.0)),
        }
    }

    /// Asks which of the rows' outputs exist.
    fn probe_outputs(&self, out: &mut Outcome) {
        out.effects
            .push(Effect::ProbeOutputs(self.files.output_pairs()));
    }

    fn job_event(&mut self, event: JobEvent, out: &mut Outcome) {
        let job = match self.work {
            Work::Running(job) => job,
            Work::Idle | Work::Stopped => return,
        };
        match event {
            JobEvent::FileProgress { path } => match job {
                RunningJob::Text(_) => {}
                RunningJob::Files { .. } => {
                    if let Some(row) = self.files.row_mut(&path)
                        && row.status == RowStatus::Pending
                    {
                        row.status = RowStatus::Running;
                        out.changed = true;
                    }
                }
            },
            JobEvent::FileFinished { path, result } => match job {
                RunningJob::Text(_) => {}
                RunningJob::Files {
                    direction,
                    finished,
                    total,
                } => self.file_finished((direction, finished, total), &path, result, out),
            },
            JobEvent::Finished(outcome) => self.job_finished(job, outcome, out),
            JobEvent::Failed(text) => {
                self.work = Work::Idle;
                let tab = job.tab();
                match job {
                    RunningJob::Text(_) => self.text.output.clear(),
                    RunningJob::Files { .. } => self.files.abandon_running(),
                }
                for other in Tab::ALL {
                    if other == tab {
                        self.notes.set(tab, Note::Failure(text.clone()));
                    } else {
                        self.notes.set(other, Note::OtherTabFailed(tab));
                    }
                }
                out.changed = true;
            }
        }
    }

    /// One file of the running batch finished: its row takes the result, from Pending or
    /// Running, and the count moves on. A path that is not a row changes nothing.
    fn file_finished(
        &mut self,
        (direction, finished, total): (Direction, usize, usize),
        path: &Path,
        result: FileResult,
        out: &mut Outcome,
    ) {
        let Some(row) = self.files.row_mut(path) else {
            return;
        };
        match row.status {
            RowStatus::Pending | RowStatus::Running => {}
            RowStatus::Invalid(_)
            | RowStatus::Done(_)
            | RowStatus::Failed(_)
            | RowStatus::Skipped => {
                return;
            }
        }
        row.status = match result {
            FileResult::Done(size) => RowStatus::Done(size),
            FileResult::Skipped => RowStatus::Skipped,
            FileResult::Failed(text) => RowStatus::Failed(text),
        };
        self.work = Work::Running(RunningJob::Files {
            direction,
            finished: finished + 1,
            total,
        });
        out.changed = true;
    }

    fn job_finished(&mut self, job: RunningJob, outcome: JobOutcome, out: &mut Outcome) {
        let answered = match (job, outcome) {
            (RunningJob::Text(_), JobOutcome::Text(text)) => {
                self.text.output = text;
                true
            }
            (RunningJob::Text(_), JobOutcome::TextRedirect) => {
                self.text.output.clear();
                self.notes.set(Tab::Text, Note::NotText);
                true
            }
            (
                RunningJob::Files { .. },
                JobOutcome::Files {
                    direction,
                    done,
                    skipped,
                    failed,
                },
            ) => {
                self.files.abandon_running();
                self.notes.set(
                    Tab::Files,
                    Note::Summary {
                        direction,
                        done,
                        skipped,
                        failed,
                    },
                );
                self.probe_outputs(out);
                true
            }
            (
                RunningJob::Text(_),
                JobOutcome::Keygen { .. }
                | JobOutcome::KeygenExists { .. }
                | JobOutcome::Files { .. },
            )
            | (
                RunningJob::Files { .. },
                JobOutcome::Text(_)
                | JobOutcome::TextRedirect
                | JobOutcome::Keygen { .. }
                | JobOutcome::KeygenExists { .. },
            ) => false,
        };
        if answered {
            self.work = Work::Idle;
            out.changed = true;
        }
    }

    fn stopped(&mut self, out: &mut Outcome) {
        match self.work {
            Work::Stopped => {}
            Work::Idle | Work::Running(_) => {
                self.work = Work::Stopped;
                self.files.abandon_running();
                self.notes.clear_where(|_, _| true);
                out.changed = true;
            }
        }
    }
}

/// Whether `input` clears `tab`'s `note`. The user's input on a tab clears that tab's note; a
/// switch to a tab is an input on neither, and only clears every note that points at the tab
/// switched to; a background answer and a drag passing over the list clear nothing (the stop
/// clears every note itself).
pub fn clears_note(input: &Input, tab: Tab, note: &Note) -> bool {
    let user_on = |on: Tab| on == tab;
    match input {
        Input::TabPicked(to) => match note {
            Note::OtherTabFailed(failed) => failed == to,
            Note::Failure(_)
            | Note::NotText
            | Note::Copied(_)
            | Note::DialogError(_)
            | Note::Missing(_)
            | Note::Summary { .. }
            | Note::Skipped(_) => false,
        },
        Input::Text(_) => user_on(Tab::Text),
        Input::Files(FilesInput::DropHover(_)) => false,
        Input::Files(_) => user_on(Tab::Files),
        Input::DialogOpened(purpose) | Input::DialogAnswered(purpose, _) => user_on(purpose.tab()),
        Input::Inspected(_) | Input::OutputsProbed(_) | Input::Job(_) | Input::Stopped => false,
    }
}

#[cfg(test)]
mod files_tests;
#[cfg(test)]
mod tests;

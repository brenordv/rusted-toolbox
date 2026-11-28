//! The Files tab: a list of files to seal to recipients, or sealed files to unseal, written
//! next to each input or into a chosen folder.

use super::notes::Missing;
use super::recipients::{Book, EntryId, RecipientIssue, RecipientPicker, effective_recipients};
use crate::worker::{JobRequest, resolve_output_path};
use seal_crypto::{Direction, output_name_for};
use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};

/// What a row says when Unseal is picked for a file that is not `.age`.
pub const MSG_ROW_NEEDS_AGE: &str = "Unseal needs a .age file.";
/// What a row says when no output name can be made for its path.
pub const MSG_ROW_UNNAMEABLE: &str = "No output name can be made for this path.";

/// What the user does on the Files tab.
#[derive(Clone, PartialEq, Eq)]
pub enum FilesInput {
    /// Seal or Unseal was picked.
    ModePicked(Direction),
    /// "Overwrite existing files" was pressed.
    OverwriteToggled,
    /// An output choice was picked.
    OutputPicked(OutputChoice),
    /// Paths were dropped on the list.
    AddPaths(Vec<PathBuf>),
    /// Remove was pressed on the row with this path.
    RemoveRow(PathBuf),
    /// Clear list was pressed.
    ClearList,
    /// A recipient's checkbox was pressed.
    RecipientToggled(EntryId),
    /// The Other key field was edited.
    OtherKeyEdited,
    /// Seal files or Unseal files was pressed, or `mod+enter`.
    StartPressed,
    /// A file drag started or stopped hovering the list.
    DropHover(bool),
}

impl fmt::Debug for FilesInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ModePicked(mode) => f.debug_tuple("ModePicked").field(mode).finish(),
            Self::OverwriteToggled => f.write_str("OverwriteToggled"),
            Self::OutputPicked(choice) => f.debug_tuple("OutputPicked").field(choice).finish(),
            Self::AddPaths(paths) => f.debug_tuple("AddPaths").field(&paths.len()).finish(),
            Self::RemoveRow(path) => f.debug_tuple("RemoveRow").field(path).finish(),
            Self::ClearList => f.write_str("ClearList"),
            Self::RecipientToggled(id) => f.debug_tuple("RecipientToggled").field(id).finish(),
            Self::OtherKeyEdited => f.write_str("OtherKeyEdited"),
            Self::StartPressed => f.write_str("StartPressed"),
            Self::DropHover(on) => f.debug_tuple("DropHover").field(on).finish(),
        }
    }
}

/// What a dropped or picked path is, read from the file system (links followed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathKind {
    /// A regular file of this many bytes.
    RegularFile { size: u64 },
    /// A folder.
    Folder,
    /// Something else: a pipe, a socket, a device.
    Other,
    /// It could not be read; the worker reports the real error when it runs.
    Unreadable,
}

/// Where a row stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowStatus {
    /// Waiting for a run.
    Pending,
    /// The row cannot run in this mode; why.
    Invalid(&'static str),
    /// The running batch is on it.
    Running,
    /// Written; the output's size in bytes.
    Done(u64),
    /// The failure's text.
    Failed(String),
    /// The output existed and overwriting was off.
    Skipped,
}

/// One listed file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRow {
    pub path: PathBuf,
    pub size: u64,
    pub status: RowStatus,
    /// The output this row would write already exists.
    pub output_exists: bool,
}

/// Where the Files tab writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputChoice {
    /// Next to each input, under the shared naming rule.
    NextToInput,
    /// Into the chosen folder, keeping the derived file name.
    Folder,
}

/// The Files tab's fields and rows.
#[derive(Debug)]
pub struct FilesTab {
    pub mode: Direction,
    pub rows: Vec<FileRow>,
    pub out_choice: OutputChoice,
    pub out_dir: Option<PathBuf>,
    pub overwrite: bool,
    pub picker: RecipientPicker,
    /// A file drag hovers the list.
    pub drop_hover: bool,
}

impl Default for FilesTab {
    fn default() -> Self {
        Self {
            mode: Direction::Encrypt,
            rows: Vec::new(),
            out_choice: OutputChoice::NextToInput,
            out_dir: None,
            overwrite: false,
            picker: RecipientPicker::default(),
            drop_hover: false,
        }
    }
}

/// Why the first path an intake turned away was not added.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skipped {
    Folder,
    NotRegular,
}

impl FilesTab {
    /// Adds each regular or unreadable path that is not a row yet, marked for the current mode;
    /// why the first other path was turned away, if one was.
    pub fn add_inspected(&mut self, inspected: Vec<(PathBuf, PathKind)>) -> Option<Skipped> {
        let mut listed: HashSet<PathBuf> = self.rows.iter().map(|row| row.path.clone()).collect();
        let mut skipped = None;
        for (path, kind) in inspected {
            let size = match kind {
                PathKind::RegularFile { size } => size,
                PathKind::Unreadable => 0,
                PathKind::Folder => {
                    skipped.get_or_insert(Skipped::Folder);
                    continue;
                }
                PathKind::Other => {
                    skipped.get_or_insert(Skipped::NotRegular);
                    continue;
                }
            };
            if !listed.insert(path.clone()) {
                continue;
            }
            let status = row_status_for(&path, self.mode);
            self.rows.push(FileRow {
                path,
                size,
                status,
                output_exists: false,
            });
        }
        skipped
    }

    /// Re-marks every row for the current mode, dropping earlier results.
    pub fn revalidate(&mut self) {
        for row in &mut self.rows {
            row.status = row_status_for(&row.path, self.mode);
        }
    }

    /// Readies every row for a new run.
    pub fn begin_run(&mut self) {
        for row in &mut self.rows {
            row.status = RowStatus::Pending;
        }
    }

    /// Returns rows the batch was on to Pending, after a run that ended without finishing them.
    pub fn abandon_running(&mut self) {
        for row in &mut self.rows {
            if row.status == RowStatus::Running {
                row.status = RowStatus::Pending;
            }
        }
    }

    /// The row with `path`, if listed.
    pub fn row_mut(&mut self, path: &Path) -> Option<&mut FileRow> {
        self.rows.iter_mut().find(|row| row.path == path)
    }

    /// Each row's path with the output it would write, for the existence probe; rows whose
    /// output cannot be named are left out, and with Folder picked and no folder chosen no
    /// output is known yet.
    pub fn output_pairs(&self) -> Vec<(PathBuf, PathBuf)> {
        let out_dir = match (self.out_choice, self.out_dir.as_deref()) {
            (OutputChoice::NextToInput, _) => None,
            (OutputChoice::Folder, Some(dir)) => Some(dir),
            (OutputChoice::Folder, None) => return Vec::new(),
        };
        self.rows
            .iter()
            .filter_map(|row| {
                resolve_output_path(&row.path, self.mode, out_dir)
                    .ok()
                    .map(|output| (row.path.clone(), output))
            })
            .collect()
    }

    /// Records which outputs exist; rows the answer does not name keep no mark.
    pub fn set_outputs(&mut self, probed: Vec<(PathBuf, bool)>) {
        for row in &mut self.rows {
            row.output_exists = false;
        }
        for (path, exists) in probed {
            if let Some(row) = self.row_mut(&path) {
                row.output_exists = exists;
            }
        }
    }

    fn first_invalid(&self) -> Option<usize> {
        self.rows.iter().position(|row| match row.status {
            RowStatus::Invalid(_) => true,
            RowStatus::Pending
            | RowStatus::Running
            | RowStatus::Done(_)
            | RowStatus::Failed(_)
            | RowStatus::Skipped => false,
        })
    }
}

/// A row is valid when the shared naming rule can name its output: any named file to seal, a
/// `.age` file (any case) to unseal.
pub fn row_status_for(path: &Path, mode: Direction) -> RowStatus {
    if output_name_for(path, mode).is_ok() {
        return RowStatus::Pending;
    }
    RowStatus::Invalid(match mode {
        Direction::Decrypt => MSG_ROW_NEEDS_AGE,
        Direction::Encrypt => MSG_ROW_UNNAMEABLE,
    })
}

/// The Files tab's batch, or the first thing missing for it.
///
/// # Errors
/// What is missing: files, valid rows, a folder, then recipients or the identity file.
pub fn files_job(
    files: &FilesTab,
    book: &Book,
    identity_path: Option<&PathBuf>,
) -> Result<JobRequest, Missing> {
    if files.rows.is_empty() {
        return Err(Missing::Files);
    }
    if let Some(index) = files.first_invalid() {
        return Err(Missing::ValidRows(index));
    }
    let out_dir = match files.out_choice {
        OutputChoice::NextToInput => None,
        OutputChoice::Folder => Some(files.out_dir.clone().ok_or(Missing::Folder)?),
    };
    let paths: Vec<PathBuf> = files.rows.iter().map(|row| row.path.clone()).collect();
    match files.mode {
        Direction::Encrypt => {
            let recipients =
                effective_recipients(book, &files.picker).map_err(|issue| match issue {
                    RecipientIssue::None => Missing::Recipients,
                    RecipientIssue::BadOtherKey => Missing::OtherKey,
                    RecipientIssue::BadSavedKey(id) => Missing::SavedKey {
                        id,
                        label: book
                            .get(id)
                            .map(|entry| entry.label.clone())
                            .unwrap_or_default(),
                    },
                })?;
            Ok(JobRequest::EncryptFiles {
                files: paths,
                recipients,
                out_dir,
                overwrite: files.overwrite,
            })
        }
        Direction::Decrypt => Ok(JobRequest::DecryptFiles {
            files: paths,
            identity_path: identity_path.cloned().ok_or(Missing::Identity)?,
            out_dir,
            overwrite: files.overwrite,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn tab_with(paths: &[&str]) -> FilesTab {
        let mut files = FilesTab::default();
        files.add_inspected(
            paths
                .iter()
                .map(|path| (PathBuf::from(path), PathKind::RegularFile { size: 1 }))
                .collect(),
        );
        files
    }

    #[rstest]
    #[case::lowercase("report.pdf.age", true)]
    #[case::uppercase("report.pdf.AGE", true)]
    #[case::no_suffix("report.pdf", false)]
    #[case::nothing_but_the_suffix(".age", false)]
    fn unseal_mode_accepts_only_age_files(#[case] name: &str, #[case] valid: bool) {
        let mut files = FilesTab {
            mode: Direction::Decrypt,
            ..FilesTab::default()
        };

        files.add_inspected(vec![(
            PathBuf::from(name),
            PathKind::RegularFile { size: 1 },
        )]);

        let status = &files.rows[0].status;
        assert_eq!(*status == RowStatus::Pending, valid, "{status:?}");
        if !valid {
            assert_eq!(status, &RowStatus::Invalid(MSG_ROW_NEEDS_AGE));
        }
    }

    #[test]
    fn seal_mode_marks_a_nameless_path() {
        let files = tab_with(&[".."]);

        assert_eq!(files.rows[0].status, RowStatus::Invalid(MSG_ROW_UNNAMEABLE));
    }

    #[test]
    fn switching_mode_remarks_every_row_and_drops_results() {
        let mut files = tab_with(&["plain.txt", "sealed.age"]);
        files.rows[0].status = RowStatus::Done(10);

        files.mode = Direction::Decrypt;
        files.revalidate();

        assert_eq!(files.rows[0].status, RowStatus::Invalid(MSG_ROW_NEEDS_AGE));
        assert_eq!(files.rows[1].status, RowStatus::Pending);
    }

    #[test]
    fn output_pairs_follow_the_output_choice() {
        let mut files = tab_with(&["a.txt", ".."]);

        let next_to = files.output_pairs();
        files.out_choice = OutputChoice::Folder;
        let no_folder_yet = files.output_pairs();
        files.out_dir = Some(PathBuf::from("safe"));
        let into_folder = files.output_pairs();

        assert_eq!(
            next_to,
            vec![(PathBuf::from("a.txt"), PathBuf::from("a.txt.age"))]
        );
        assert!(no_folder_yet.is_empty());
        assert_eq!(
            into_folder,
            vec![(PathBuf::from("a.txt"), Path::new("safe").join("a.txt.age"))]
        );
    }

    #[test]
    fn set_outputs_marks_by_path_and_ignores_paths_that_are_not_rows() {
        let mut files = tab_with(&["a.txt", "b.txt"]);
        files.rows[1].output_exists = true;

        files.set_outputs(vec![
            (PathBuf::from("a.txt"), true),
            (PathBuf::from("gone.txt"), true),
        ]);

        assert!(files.rows[0].output_exists);
        assert!(!files.rows[1].output_exists);
    }
}

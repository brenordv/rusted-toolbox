//! What each tab's status line says: the running job, another tab's job, or the tab's one
//! stored note.

use super::Tab;
use super::files::Skipped;
use common_gui_rune::parts::Tone;
use common_utils::string_utils::{display_line, format_thousands};
use shared_crypto::Direction;

/// A job the worker is running, with what its status line needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunningJob {
    /// A Text tab job.
    Text(Direction),
    /// A Files batch: how many files it finished, of how many.
    Files {
        direction: Direction,
        finished: usize,
        total: usize,
    },
    /// Generating a key pair.
    Keygen,
}

impl RunningJob {
    /// The tab the job belongs to.
    pub fn tab(self) -> Tab {
        match self {
            Self::Text(_) => Tab::Text,
            Self::Files { .. } => Tab::Files,
            Self::Keygen => Tab::Keys,
        }
    }

    /// What the job's own tab shows while it runs. A batch counts the file it is on.
    pub fn phrase(self) -> String {
        match self {
            Self::Text(Direction::Encrypt) => "Sealing…".to_owned(),
            Self::Text(Direction::Decrypt) => "Unsealing…".to_owned(),
            Self::Keygen => "Generating a key pair…".to_owned(),
            Self::Files {
                direction,
                finished,
                total,
            } => format!(
                "{} {} of {}",
                verb(direction),
                format_thousands((finished + 1).min(total) as u64),
                format_thousands(total as u64)
            ),
        }
    }

    /// What assistive technology hears for the job's own tab: one phrase per job, with no
    /// count, so the status is announced once rather than on every file.
    pub fn spoken(self) -> String {
        match self {
            Self::Text(_) | Self::Keygen => self.phrase(),
            Self::Files { direction, .. } => format!("{} files", verb(direction)),
        }
    }

    /// What another tab shows while the job runs: the job's tab, then the phrase.
    pub fn elsewhere(self) -> String {
        format!("{}: {}", self.tab().name(), lower_first(&self.phrase()))
    }

    /// What assistive technology hears for another tab while the job runs.
    pub fn spoken_elsewhere(self) -> String {
        format!("{}: {}", self.tab().name(), lower_first(&self.spoken()))
    }
}

fn verb(direction: Direction) -> &'static str {
    match direction {
        Direction::Encrypt => "Sealing",
        Direction::Decrypt => "Unsealing",
    }
}

fn lower_first(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_lowercase().chain(chars).collect::<String>()
    })
}

/// What a refused start found missing; the note says it and the field it names is marked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Missing {
    /// The Text tab's input is blank.
    Input,
    /// No recipient is checked and no key is pasted.
    Recipients,
    /// A checked entry's saved key does not parse: the entry, and its label as stored.
    SavedKey {
        id: super::recipients::EntryId,
        label: String,
    },
    /// The pasted key does not parse.
    OtherKey,
    /// No identity file is chosen.
    Identity,
    /// The Files list is empty.
    Files,
    /// A row cannot run in this mode: the first such row's place in the list.
    ValidRows(usize),
    /// Folder is picked and no folder is chosen.
    Folder,
}

impl Missing {
    /// The words the status line shows.
    pub fn words(&self) -> String {
        match self {
            Self::Input => "Enter some text first.".to_owned(),
            Self::Files => "Add at least one file.".to_owned(),
            Self::ValidRows(_) => "Remove the invalid rows first.".to_owned(),
            Self::Folder => "Choose an output folder.".to_owned(),
            Self::Recipients => "Choose at least one recipient or paste a key.".to_owned(),
            Self::SavedKey { label, .. } => format!(
                "The saved key for {} is not a valid age public key.",
                display_line(label.chars())
            ),
            Self::OtherKey => super::recipients::MSG_BAD_KEY.to_owned(),
            Self::Identity => "Choose an identity file first.".to_owned(),
        }
    }
}

/// What was copied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Copied {
    /// The Text tab's output box.
    Output,
    /// A public key, from the book or the generated pair.
    PublicKey,
}

/// A tab's stored outcome, kept until the user's next input on that tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Note {
    /// The tab's job failed: the failure text.
    Failure(String),
    /// Unsealed content that is not text.
    NotText,
    /// Something was copied.
    Copied(Copied),
    /// A file dialog could not show: its error text.
    DialogError(String),
    /// A start found something missing.
    Missing(Missing),
    /// The job of another tab failed.
    OtherTabFailed(Tab),
    /// A Files batch ended: its totals.
    Summary {
        direction: Direction,
        done: usize,
        skipped: usize,
        failed: usize,
    },
    /// An intake turned a path away.
    Skipped(Skipped),
    /// A generated key pair was saved.
    KeySaved,
    /// An entry joined the book: its label.
    Added(String),
}

impl Note {
    /// The tone and words the status line shows.
    pub fn words(&self) -> (Tone, String) {
        match self {
            Self::Failure(text) => (Tone::Danger, text.clone()),
            Self::KeySaved => (Tone::Muted, "Saved the key file.".to_owned()),
            Self::Added(label) => (
                Tone::Muted,
                format!(
                    "Added {} to the recipient book.",
                    display_line(label.chars())
                ),
            ),
            Self::Copied(Copied::PublicKey) => (Tone::Muted, "Copied the public key.".to_owned()),
            Self::Summary {
                direction,
                done,
                skipped,
                failed,
            } => {
                let tone = if *failed > 0 {
                    Tone::Danger
                } else if *skipped > 0 {
                    Tone::Warning
                } else {
                    Tone::Muted
                };
                let verb = match direction {
                    Direction::Encrypt => "sealed",
                    Direction::Decrypt => "unsealed",
                };
                (
                    tone,
                    format!(
                        "{} {verb}, {} skipped, {} failed.",
                        format_thousands(*done as u64),
                        format_thousands(*skipped as u64),
                        format_thousands(*failed as u64)
                    ),
                )
            }
            Self::Skipped(Skipped::Folder) => (
                Tone::Warning,
                "Folders cannot be added; add the files inside instead.".to_owned(),
            ),
            Self::Skipped(Skipped::NotRegular) => (
                Tone::Warning,
                "Only regular files can be added; a pipe or device was left out.".to_owned(),
            ),
            Self::NotText => (
                Tone::Warning,
                "The unsealed content is not text. Unseal it as a file in the Files tab."
                    .to_owned(),
            ),
            Self::Copied(Copied::Output) => (Tone::Muted, "Copied the output.".to_owned()),
            Self::DialogError(text) => (
                Tone::Danger,
                format!("The file dialog could not open: {text}"),
            ),
            Self::Missing(missing) => (Tone::Danger, missing.words()),
            Self::OtherTabFailed(Tab::Text) => {
                (Tone::Danger, "The Text job failed. See Text.".to_owned())
            }
            Self::OtherTabFailed(Tab::Files) => {
                (Tone::Danger, "The Files run failed. See Files.".to_owned())
            }
            Self::OtherTabFailed(Tab::Keys) => {
                (Tone::Danger, "Key generation failed. See Keys.".to_owned())
            }
        }
    }
}

/// The one stored note of each tab.
#[derive(Debug, Clone, Default)]
pub struct Notes {
    text: Option<Note>,
    files: Option<Note>,
    keys: Option<Note>,
}

impl Notes {
    pub fn get(&self, tab: Tab) -> Option<&Note> {
        self.slot(tab).as_ref()
    }

    pub fn set(&mut self, tab: Tab, note: Note) {
        *self.slot_mut(tab) = Some(note);
    }

    /// Clears `tab`'s note; true when there was one.
    pub fn clear(&mut self, tab: Tab) -> bool {
        self.slot_mut(tab).take().is_some()
    }

    /// Clears every tab's note for which `clears` says so; true when any went.
    pub fn clear_where(&mut self, clears: impl Fn(Tab, &Note) -> bool) -> bool {
        let mut cleared = false;
        for tab in Tab::ALL {
            let slot = self.slot_mut(tab);
            if slot.as_ref().is_some_and(|note| clears(tab, note)) {
                *slot = None;
                cleared = true;
            }
        }
        cleared
    }

    fn slot(&self, tab: Tab) -> &Option<Note> {
        match tab {
            Tab::Text => &self.text,
            Tab::Files => &self.files,
            Tab::Keys => &self.keys,
        }
    }

    fn slot_mut(&mut self, tab: Tab) -> &mut Option<Note> {
        match tab {
            Tab::Text => &mut self.text,
            Tab::Files => &mut self.files,
            Tab::Keys => &mut self.keys,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::seal(Direction::Encrypt, "Sealing…", "Text: sealing…")]
    #[case::unseal(Direction::Decrypt, "Unsealing…", "Text: unsealing…")]
    fn a_text_job_names_itself_and_its_tab(
        #[case] direction: Direction,
        #[case] own: &str,
        #[case] elsewhere: &str,
    ) {
        let job = RunningJob::Text(direction);

        assert_eq!(job.phrase(), own);
        assert_eq!(job.elsewhere(), elsewhere);
    }

    #[rstest]
    #[case::text(Tab::Text, "The Text job failed. See Text.")]
    #[case::files(Tab::Files, "The Files run failed. See Files.")]
    #[case::keys(Tab::Keys, "Key generation failed. See Keys.")]
    fn another_tabs_failure_is_worded_per_tab(#[case] tab: Tab, #[case] words: &str) {
        assert_eq!(
            Note::OtherTabFailed(tab).words(),
            (Tone::Danger, words.to_owned())
        );
    }

    #[test]
    fn a_saved_keys_label_shows_cleaned() {
        let mut book = super::super::recipients::Book::default();
        let id = book.push("Alice".to_owned(), "age1".to_owned());
        let words = Missing::SavedKey {
            id,
            label: "Ali\u{202E}ce\n".to_owned(),
        }
        .words();

        assert!(!words.contains('\u{202E}'), "{words}");
        assert!(!words.contains('\n'), "{words}");
    }

    #[test]
    fn clear_where_drops_only_the_notes_it_names() {
        let mut notes = Notes::default();
        notes.set(Tab::Text, Note::OtherTabFailed(Tab::Files));
        notes.set(Tab::Keys, Note::Copied(Copied::Output));

        let cleared = notes.clear_where(|_, note| matches!(note, Note::OtherTabFailed(_)));

        assert!(cleared);
        assert_eq!(notes.get(Tab::Text), None);
        assert_eq!(notes.get(Tab::Keys), Some(&Note::Copied(Copied::Output)));
    }
}

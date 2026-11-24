//! The Keys tab: the key pair the user generates (the secret goes straight to a file; only the
//! public key is kept), and the recipient book with its add form.

use super::recipients::{Book, EntryId, MSG_BAD_KEY};
use common_utils::string_utils::display_line;
use rune::TextEdit;
use shared_crypto::Recipient;
use std::fmt;
use std::path::PathBuf;

/// What the user does on the Keys tab.
#[derive(Clone, PartialEq, Eq)]
pub enum KeysInput {
    /// "Generate key pair" was pressed.
    GeneratePressed,
    /// "Replace key file" was pressed on the confirm.
    ReplacePressed,
    /// "Cancel" was pressed on the confirm.
    CancelPressed,
    /// "Copy" was pressed on the generated key.
    CopyGenerated,
    /// "Add to recipient book" was pressed on the generated key.
    AddGeneratedPressed,
    /// "Copy" was pressed on a book row.
    CopyEntry(EntryId),
    /// "Delete" or "Confirm delete" was pressed on a book row.
    DeletePressed(EntryId),
    /// The add form's Label field was edited.
    LabelEdited,
    /// The add form's Public key field was edited.
    KeyEdited,
    /// "Add recipient" was pressed, or Return or `mod+enter` in the add form.
    AddPressed,
}

impl fmt::Debug for KeysInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GeneratePressed => f.write_str("GeneratePressed"),
            Self::ReplacePressed => f.write_str("ReplacePressed"),
            Self::CancelPressed => f.write_str("CancelPressed"),
            Self::CopyGenerated => f.write_str("CopyGenerated"),
            Self::AddGeneratedPressed => f.write_str("AddGeneratedPressed"),
            Self::CopyEntry(id) => f.debug_tuple("CopyEntry").field(id).finish(),
            Self::DeletePressed(id) => f.debug_tuple("DeletePressed").field(id).finish(),
            Self::LabelEdited => f.write_str("LabelEdited"),
            Self::KeyEdited => f.write_str("KeyEdited"),
            Self::AddPressed => f.write_str("AddPressed"),
        }
    }
}

/// Where key generation stands between a job and the user's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeygenStep {
    /// Nothing waits on the user.
    Idle,
    /// The file the user picked exists; replacing it waits on the confirm.
    ConfirmReplace { path: PathBuf },
}

/// The key pair generated this session: its public key and the identity file it was saved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Generated {
    pub public_key: String,
    pub path: PathBuf,
}

impl Generated {
    /// The label an add from this key uses: the saved file's name without its extension.
    pub fn stem(&self) -> String {
        self.path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

/// What the add form found wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddIssue {
    /// The label is empty once cleaned.
    EmptyLabel,
    /// The cleaned label is already in the book.
    DuplicateLabel(String),
    /// The key does not parse.
    BadKey,
}

impl AddIssue {
    /// The words under the field the issue names.
    pub fn words(&self) -> String {
        match self {
            Self::EmptyLabel => "Enter a label.".to_owned(),
            Self::DuplicateLabel(label) => format!("{label} is already in the book."),
            Self::BadKey => MSG_BAD_KEY.to_owned(),
        }
    }

    /// Whether the issue names the Label field (else the Public key field).
    pub fn on_label(&self) -> bool {
        match self {
            Self::EmptyLabel | Self::DuplicateLabel(_) => true,
            Self::BadKey => false,
        }
    }
}

/// The Keys tab's fields and the two confirms.
pub struct KeysTab {
    pub step: KeygenStep,
    pub generated: Option<Generated>,
    /// The book row whose Delete was pressed once; its button now confirms.
    pub armed: Option<EntryId>,
    pub label: TextEdit,
    pub key: TextEdit,
    pub issue: Option<AddIssue>,
}

impl Default for KeysTab {
    fn default() -> Self {
        Self {
            step: KeygenStep::Idle,
            generated: None,
            armed: None,
            label: TextEdit::new(""),
            key: TextEdit::new(""),
            issue: None,
        }
    }
}

impl fmt::Debug for KeysTab {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeysTab")
            .field("step", &self.step)
            .field("generated", &self.generated.is_some())
            .field("armed", &self.armed)
            .field("label_len", &self.label.text.len())
            .field("key_len", &self.key.text.len())
            .field("issue", &self.issue.as_ref().map(AddIssue::on_label))
            .finish()
    }
}

impl KeysTab {
    /// Drops both confirms; true when one was armed.
    pub fn disarm(&mut self) -> bool {
        let confirming = match self.step {
            KeygenStep::Idle => false,
            KeygenStep::ConfirmReplace { .. } => true,
        };
        let was_armed = self.armed.is_some() || confirming;
        self.armed = None;
        self.step = KeygenStep::Idle;
        was_armed
    }
}

/// A label as the book compares and stores it: control and bidirectional characters as
/// spaces, then trimmed.
pub fn clean_label(label: &str) -> String {
    display_line(label.chars()).trim().to_owned()
}

/// The entry `label` and `key` would add, or what is wrong with them: the label first.
///
/// # Errors
/// An empty or taken label, then a key that does not parse.
pub fn new_entry(book: &Book, label: &str, key: &str) -> Result<(String, String), AddIssue> {
    let label = clean_label(label);
    if label.is_empty() {
        return Err(AddIssue::EmptyLabel);
    }
    if book
        .entries()
        .iter()
        .any(|entry| clean_label(&entry.label) == label)
    {
        return Err(AddIssue::DuplicateLabel(label));
    }
    let key = key.trim();
    if key.parse::<Recipient>().is_err() {
        return Err(AddIssue::BadKey);
    }
    Ok((label, key.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RecipientEntry;
    use shared_crypto::keys::{generate_identity, public_key_of};

    fn book_of(labels: &[&str]) -> Book {
        Book::from_entries(
            labels
                .iter()
                .map(|label| RecipientEntry {
                    label: (*label).to_owned(),
                    public_key: public_key_of(&generate_identity()),
                })
                .collect(),
        )
    }

    #[test]
    fn c9_an_empty_label_comes_before_a_bad_key() {
        let book = book_of(&[]);

        assert_eq!(
            new_entry(&book, "  \u{7} ", "nope"),
            Err(AddIssue::EmptyLabel)
        );
    }

    #[test]
    fn c9_a_label_taken_once_cleaned_is_a_duplicate() {
        let book = book_of(&["Alice"]);
        let key = public_key_of(&generate_identity());

        assert_eq!(
            new_entry(&book, " Alice\u{202E}", &key),
            Err(AddIssue::DuplicateLabel("Alice".to_owned()))
        );
    }

    #[test]
    fn c9_a_bad_key_is_named_after_a_free_label() {
        let book = book_of(&["Alice"]);

        assert_eq!(new_entry(&book, "Bob", "age1nope"), Err(AddIssue::BadKey));
    }

    #[test]
    fn c9_a_good_entry_is_cleaned_and_trimmed() {
        let book = book_of(&[]);
        let key = public_key_of(&generate_identity());

        let added = new_entry(&book, " Bo\u{7}b ", &format!(" {key} "));

        assert_eq!(added, Ok(("Bo b".to_owned(), key)));
    }

    #[test]
    fn the_stem_of_the_saved_file_is_its_name_without_the_extension() {
        let generated = Generated {
            public_key: "age1x".to_owned(),
            path: PathBuf::from("keys").join("work-laptop.txt"),
        };

        assert_eq!(generated.stem(), "work-laptop");
    }
}

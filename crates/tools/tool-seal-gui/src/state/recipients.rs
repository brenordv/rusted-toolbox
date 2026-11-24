//! The recipient book as the window holds it, and the per-tab pickers over it. Each entry gets
//! a session id when the book is loaded or an entry is added, so selection follows the entry
//! whatever happens to the others. The config file never sees the ids.

use super::Tab;
use crate::config::RecipientEntry;
use rune::TextEdit;
use shared_crypto::Recipient;
use std::collections::BTreeSet;
use std::fmt;

/// What the window says about a key that does not parse; fixed words that never repeat what
/// was typed.
pub const MSG_BAD_KEY: &str = "Not a valid age public key (expected an age1... value).";

/// A book entry's id for this session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntryId(u64);

impl EntryId {
    /// The id's number, for an element key.
    pub fn number(self) -> u64 {
        self.0
    }
}

/// One saved recipient.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookEntry {
    pub id: EntryId,
    pub label: String,
    pub public_key: String,
}

/// The saved recipients, in the config file's order, with the next id to hand out.
#[derive(Debug, Clone, Default)]
pub struct Book {
    entries: Vec<BookEntry>,
    next_id: u64,
}

impl Book {
    /// The book of `entries`, each given a fresh id.
    pub fn from_entries(entries: Vec<RecipientEntry>) -> Self {
        let mut book = Self::default();
        for entry in entries {
            book.push(entry.label, entry.public_key);
        }
        book
    }

    /// Appends an entry under a fresh id and returns the id.
    pub fn push(&mut self, label: String, public_key: String) -> EntryId {
        let id = EntryId(self.next_id);
        self.next_id += 1;
        self.entries.push(BookEntry {
            id,
            label,
            public_key,
        });
        id
    }

    pub fn entries(&self) -> &[BookEntry] {
        &self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The entry with `id`, if the book still holds it.
    pub fn get(&self, id: EntryId) -> Option<&BookEntry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    /// The entries as the config file stores them, without ids.
    pub fn to_config(&self) -> Vec<RecipientEntry> {
        self.entries
            .iter()
            .map(|entry| RecipientEntry {
                label: entry.label.clone(),
                public_key: entry.public_key.clone(),
            })
            .collect()
    }
}

/// A tab that seals to recipients and unseals with the identity file, each with its own
/// [`RecipientPicker`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Picker {
    Text,
    Files,
}

impl Picker {
    /// The tab the picker is on.
    pub fn tab(self) -> Tab {
        match self {
            Self::Text => Tab::Text,
            Self::Files => Tab::Files,
        }
    }
}

/// One tab's choice of recipients: checked book entries plus a key pasted for one run.
#[derive(Default)]
pub struct RecipientPicker {
    pub selected: BTreeSet<EntryId>,
    pub other_key: TextEdit,
    /// The pasted key does not parse; shown under the field.
    pub other_key_bad: bool,
}

impl fmt::Debug for RecipientPicker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RecipientPicker")
            .field("selected", &self.selected)
            .field("other_key_len", &self.other_key.text.len())
            .field("other_key_bad", &self.other_key_bad)
            .finish()
    }
}

impl RecipientPicker {
    /// Checks or unchecks `id`.
    pub fn toggle(&mut self, id: EntryId) {
        if !self.selected.remove(&id) {
            self.selected.insert(id);
        }
    }

    /// Re-checks the pasted key, for the mark under the field: an empty field is fine.
    pub fn check_other_key(&mut self) {
        let typed = self.other_key.text.trim();
        self.other_key_bad = !typed.is_empty() && typed.parse::<Recipient>().is_err();
    }
}

/// Why a run has no recipients to seal to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecipientIssue {
    /// Nothing checked and nothing pasted.
    None,
    /// A checked entry's saved key does not parse (a hand-edited config file); its id.
    BadSavedKey(EntryId),
    /// The pasted key does not parse.
    BadOtherKey,
}

/// The checked entries of `book`, in the book's order, then the pasted key.
///
/// # Errors
/// A [`RecipientIssue`] when nothing is chosen or a chosen key does not parse.
pub fn effective_recipients(
    book: &Book,
    picker: &RecipientPicker,
) -> Result<Vec<Recipient>, RecipientIssue> {
    let mut recipients = Vec::new();
    for entry in book.entries() {
        if !picker.selected.contains(&entry.id) {
            continue;
        }
        let recipient = entry
            .public_key
            .parse::<Recipient>()
            .map_err(|_| RecipientIssue::BadSavedKey(entry.id))?;
        recipients.push(recipient);
    }

    let pasted = picker.other_key.text.trim();
    if !pasted.is_empty() {
        recipients.push(
            pasted
                .parse::<Recipient>()
                .map_err(|_| RecipientIssue::BadOtherKey)?,
        );
    }

    if recipients.is_empty() {
        return Err(RecipientIssue::None);
    }
    Ok(recipients)
}

/// Shortens an `age1...` key for a list row; the whole key stays the row's description and one
/// Copy away.
pub fn truncated_key(key: &str) -> String {
    const HEAD: usize = 12;
    const TAIL: usize = 6;
    if key.len() <= HEAD + TAIL + 3 || !key.is_ascii() {
        return key.to_owned();
    }
    format!("{}…{}", &key[..HEAD], &key[key.len() - TAIL..])
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared_crypto::keys::{generate_identity, public_key_of};

    fn fresh_key() -> String {
        public_key_of(&generate_identity())
    }

    fn book_of(labels: &[&str]) -> Book {
        Book::from_entries(
            labels
                .iter()
                .map(|label| RecipientEntry {
                    label: (*label).to_owned(),
                    public_key: fresh_key(),
                })
                .collect(),
        )
    }

    fn id_of(book: &Book, label: &str) -> EntryId {
        book.entries()
            .iter()
            .find(|entry| entry.label == label)
            .unwrap()
            .id
    }

    #[test]
    fn checked_entries_and_the_pasted_key_make_the_recipients() {
        let book = book_of(&["Alice", "Bea", "Cho"]);
        let mut picker = RecipientPicker::default();
        picker.toggle(id_of(&book, "Alice"));
        picker.toggle(id_of(&book, "Cho"));
        picker.other_key = TextEdit::new(format!("  {}  ", fresh_key()));

        let recipients = effective_recipients(&book, &picker).unwrap();

        assert_eq!(recipients.len(), 3);
    }

    #[test]
    fn nothing_chosen_is_the_none_issue() {
        let book = book_of(&["Alice"]);

        let issue = effective_recipients(&book, &RecipientPicker::default()).unwrap_err();

        assert_eq!(issue, RecipientIssue::None);
    }

    #[test]
    fn a_bad_pasted_key_is_its_own_issue() {
        let picker = RecipientPicker {
            other_key: TextEdit::new("age1-not-a-key"),
            ..RecipientPicker::default()
        };

        let issue = effective_recipients(&Book::default(), &picker).unwrap_err();

        assert_eq!(issue, RecipientIssue::BadOtherKey);
    }

    #[test]
    fn a_saved_key_that_no_longer_parses_names_its_entry() {
        let book = Book::from_entries(vec![RecipientEntry {
            label: "Alice".to_owned(),
            public_key: "hand-edited garbage".to_owned(),
        }]);
        let mut picker = RecipientPicker::default();
        let alice = id_of(&book, "Alice");
        picker.toggle(alice);

        let issue = effective_recipients(&book, &picker).unwrap_err();

        assert_eq!(issue, RecipientIssue::BadSavedKey(alice));
    }

    #[test]
    fn two_entries_sharing_a_label_are_chosen_separately() {
        let book = Book::from_entries(vec![
            RecipientEntry {
                label: "Alice".to_owned(),
                public_key: fresh_key(),
            },
            RecipientEntry {
                label: "Alice".to_owned(),
                public_key: "hand-edited garbage".to_owned(),
            },
        ]);
        let mut picker = RecipientPicker::default();
        picker.toggle(book.entries()[0].id);

        let recipients = effective_recipients(&book, &picker).unwrap();

        assert_eq!(recipients.len(), 1);
    }

    #[test]
    fn toggling_twice_unchecks() {
        let book = book_of(&["Alice"]);
        let mut picker = RecipientPicker::default();
        let alice = id_of(&book, "Alice");

        picker.toggle(alice);
        picker.toggle(alice);

        assert!(picker.selected.is_empty());
    }

    #[test]
    fn the_pasted_key_check_marks_garbage_and_accepts_empty_or_valid() {
        let mut picker = RecipientPicker {
            other_key: TextEdit::new("garbage"),
            ..RecipientPicker::default()
        };
        picker.check_other_key();
        assert!(picker.other_key_bad);

        picker.other_key = TextEdit::new(fresh_key());
        picker.check_other_key();
        assert!(!picker.other_key_bad);

        picker.other_key = TextEdit::new("   ");
        picker.check_other_key();
        assert!(!picker.other_key_bad);
    }

    #[test]
    fn ids_are_fresh_and_the_config_form_drops_them() {
        let mut book = book_of(&["Alice"]);
        let first = book.entries()[0].id;

        let second = book.push("Bea".to_owned(), fresh_key());

        assert_ne!(first, second);
        let config = book.to_config();
        assert_eq!(
            config
                .iter()
                .map(|entry| entry.label.as_str())
                .collect::<Vec<_>>(),
            ["Alice", "Bea"]
        );
    }

    #[test]
    fn truncated_key_shortens_long_keys_and_keeps_short_ones() {
        let long = "age1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq";

        assert_eq!(truncated_key(long), "age1qqqqqqqq…qqqqqq");
        assert_eq!(truncated_key("age1short"), "age1short");
    }

    #[test]
    fn the_pickers_debug_never_shows_the_pasted_key() {
        let picker = RecipientPicker {
            other_key: TextEdit::new("AGE-SECRET-KEY-1MARKER"),
            ..RecipientPicker::default()
        };

        let shown = format!("{picker:?}");

        assert!(!shown.contains("MARKER"), "{shown}");
    }
}

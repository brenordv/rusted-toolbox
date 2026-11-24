//! The Text tab: a text to seal to recipients as armor, or armored text to unseal.

use super::notes::Missing;
use super::recipients::{Book, RecipientIssue, RecipientPicker, effective_recipients};
use crate::worker::JobRequest;
use rune::TextEdit;
use shared_crypto::Direction;
use std::fmt;
use std::path::PathBuf;

/// What the user does on the Text tab.
#[derive(Clone, PartialEq, Eq)]
pub enum TextInput {
    /// Seal or Unseal was picked.
    DirectionPicked(Direction),
    /// The input box was edited.
    InputEdited,
    /// A recipient's checkbox was pressed.
    RecipientToggled(super::recipients::EntryId),
    /// The Other key field was edited.
    OtherKeyEdited,
    /// Seal or Unseal was pressed, or `mod+enter`.
    StartPressed,
    /// Copy output was pressed.
    CopyPressed,
}

impl fmt::Debug for TextInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DirectionPicked(direction) => {
                f.debug_tuple("DirectionPicked").field(direction).finish()
            }
            Self::InputEdited => f.write_str("InputEdited"),
            Self::RecipientToggled(id) => f.debug_tuple("RecipientToggled").field(id).finish(),
            Self::OtherKeyEdited => f.write_str("OtherKeyEdited"),
            Self::StartPressed => f.write_str("StartPressed"),
            Self::CopyPressed => f.write_str("CopyPressed"),
        }
    }
}

/// The Text tab's fields and result.
pub struct TextTab {
    pub direction: Direction,
    pub input: TextEdit,
    /// The last result: armored text or unsealed text. Empty when there is none.
    pub output: String,
    pub picker: RecipientPicker,
}

impl Default for TextTab {
    fn default() -> Self {
        Self {
            direction: Direction::Encrypt,
            input: TextEdit::new(""),
            output: String::new(),
            picker: RecipientPicker::default(),
        }
    }
}

impl fmt::Debug for TextTab {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TextTab")
            .field("direction", &self.direction)
            .field("input_len", &self.input.text.len())
            .field("output_len", &self.output.len())
            .field("picker", &self.picker)
            .finish()
    }
}

/// The Text tab's job, or what is missing for it.
///
/// # Errors
/// The first thing missing: the input, then the recipients or the identity file.
pub fn text_job(
    text: &TextTab,
    book: &Book,
    identity_path: Option<&PathBuf>,
) -> Result<JobRequest, Missing> {
    let input = text.input.text.to_string();
    if input.trim().is_empty() {
        return Err(Missing::Input);
    }
    match text.direction {
        Direction::Encrypt => {
            let recipients =
                effective_recipients(book, &text.picker).map_err(|issue| match issue {
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
            Ok(JobRequest::EncryptText {
                text: input,
                recipients,
            })
        }
        Direction::Decrypt => {
            let identity_path = identity_path.cloned().ok_or(Missing::Identity)?;
            Ok(JobRequest::DecryptText {
                text: input,
                identity_path,
            })
        }
    }
}

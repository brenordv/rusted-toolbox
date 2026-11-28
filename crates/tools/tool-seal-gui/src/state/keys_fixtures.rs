//! What the Keys and config-save tests share: a window on the Keys tab, the inputs they send,
//! and the effects they read back.

use super::*;
use crate::test_support::state_with_book;
use rune::TextEdit;
use seal_crypto::keys::{generate_identity, public_key_of};

pub fn fresh_key() -> String {
    public_key_of(&generate_identity())
}

/// A window on the Keys tab whose book holds `labels`.
pub fn on_keys(labels: &[&str]) -> AppState {
    let mut s = state_with_book(labels);
    s.tab = Tab::Keys;
    s
}

pub fn keys(s: &mut AppState, input: KeysInput) -> Outcome {
    s.apply(Input::Keys(input))
}

pub fn id_of(s: &AppState, index: usize) -> EntryId {
    s.book.entries()[index].id
}

/// The config snapshots `out` asks to save.
pub fn saves(out: &Outcome) -> Vec<&SealConfig> {
    out.effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::SaveConfig(config) => Some(config),
            Effect::Run(_)
            | Effect::OpenDialog(_)
            | Effect::Copy(_)
            | Effect::Inspect(_)
            | Effect::ProbeOutputs(_) => None,
        })
        .collect()
}

/// The jobs `out` starts.
pub fn runs(out: &Outcome) -> Vec<&JobRequest> {
    out.effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Run(request) => Some(request),
            Effect::SaveConfig(_)
            | Effect::OpenDialog(_)
            | Effect::Copy(_)
            | Effect::Inspect(_)
            | Effect::ProbeOutputs(_) => None,
        })
        .collect()
}

/// Opens the dialog for `purpose` and answers it with `path`.
pub fn answer(s: &mut AppState, purpose: DialogPurpose, path: &str) -> Outcome {
    s.apply(Input::DialogOpened(purpose));
    s.apply(Input::DialogAnswered(
        purpose,
        Ok(Some(PathBuf::from(path))),
    ))
}

/// Fills the add form and presses "Add recipient".
pub fn add_via_form(s: &mut AppState, label: &str, key: &str) -> Outcome {
    s.keys.label = TextEdit::new(label);
    s.keys.key = TextEdit::new(key);
    keys(s, KeysInput::AddPressed)
}

/// Presses Delete on the entry, then Confirm delete.
pub fn delete(s: &mut AppState, id: EntryId) -> Outcome {
    keys(s, KeysInput::DeletePressed(id));
    keys(s, KeysInput::DeletePressed(id))
}

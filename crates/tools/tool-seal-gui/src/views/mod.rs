//! The seal window: the tab switch in a fixed header row over the tab that shows. Views are pure
//! functions of the state; handlers dispatch inputs.

mod files_tab;
mod parts;
mod picker;
mod text_tab;

pub use files_tab::{
    ADD_FILES_KEY, CHOOSE_FOLDER_KEY, CLEAR_LIST_KEY, FILE_LIST_KEY, FILES_ACTIONS_KEY,
    FILES_CHOOSE_IDENTITY_KEY, FILES_FORM_KEY, FILES_IDENTITY_KEY, FILES_OTHER_KEY_KEY,
    FILES_RECIPIENTS_KEY, FILES_SCROLL_KEY, FILES_STATUS_KEY, FILES_SUBMIT_KEY, MODE_KEY,
    MSG_OUTPUT_EXISTS, OUTPUT_CHOICE_KEY, OUTPUT_FOLDER_KEY, OVERWRITE_KEY, file_row_key,
    remove_key,
};
pub use parts::BadgeTone;
pub use picker::{picker_keys, recipient_key};
pub use text_tab::{
    CHOOSE_IDENTITY_KEY, COPY_OUTPUT_KEY, DIRECTION_KEY, IDENTITY_KEY, INPUT_KEY, OTHER_KEY_KEY,
    OUTPUT_KEY, RECIPIENTS_KEY, TEXT_ACTIONS_KEY, TEXT_FORM_KEY, TEXT_SCROLL_KEY, TEXT_STATUS_KEY,
    TEXT_SUBMIT_KEY,
};

use crate::design::FORM_MAX_WIDTH;
use crate::services::dispatch;
use crate::state::files::FilesInput;
use crate::state::text::TextInput;
use crate::state::{AppState, Focus, Input, Tab, Work};
use common_gui_rune::keys::{area_chord, is_chord};
use common_gui_rune::motion::CONTROL_PAIR;
use common_gui_rune::parts::{StatusLines, Tone, caption, choice_switch, hairline, status_line};
use common_gui_rune::tokens::{SPACE_LG, SPACE_SM};
use common_gui_rune::{Surface, surface};
use rune::{Cx, El, KeyEvent, Transition, Tx, col, row};
use rune_kit::Kit;

/// The window's title in the OS.
pub const OS_TITLE: &str = "seal";
/// The tab switch in the header row.
pub const TABS_KEY: &str = "seal-tabs";
/// The Text tab's root.
pub const TEXT_TAB_KEY: &str = "seal-text";
/// The Files tab's root.
pub const FILES_TAB_KEY: &str = "seal-files";
/// The Keys tab's root.
pub const KEYS_TAB_KEY: &str = "seal-keys";
/// The standing caption shown once the worker stopped.
pub const STOPPED_KEY: &str = "seal-worker-stopped";

/// What the standing caption says once the worker stopped.
pub const STOPPED_CAPTION: &str =
    "The background worker stopped. Restart seal-gui to seal or unseal again.";
/// What a tab that is not built yet shows.
pub const PLACEHOLDER: &str = "This tab arrives with the next change.";

/// The key of the control `focus` names.
pub fn focus_key(focus: Focus) -> String {
    match focus {
        Focus::TextInput => INPUT_KEY.to_owned(),
        Focus::Recipient(picker, id) => recipient_key(picker, id),
        Focus::OtherKey(picker) => picker_keys(picker).other_key.to_owned(),
        Focus::ChooseIdentity(picker) => picker_keys(picker).choose_identity.to_owned(),
        Focus::AddFiles => ADD_FILES_KEY.to_owned(),
        Focus::RemoveRow(index) => remove_key(index),
        Focus::ChooseFolder => CHOOSE_FOLDER_KEY.to_owned(),
    }
}

/// Runs when the window is first on screen: focus goes to the Text tab's input. Always true.
pub fn shown(_: &mut AppState, tx: &mut Tx) -> bool {
    tx.focus(INPUT_KEY, false);
    true
}

/// Sets the appearance the window draws in (true: dark); true when it changed.
pub fn set_appearance(s: &mut AppState, dark: bool) -> bool {
    s.theme.set(dark)
}

/// Runs as the window closes: the worker's later posts go nowhere.
pub fn on_close(s: &mut AppState, _: &mut Tx) {
    if let Some(gate) = &s.gate {
        gate.close();
    }
}

/// The window's shortcuts: `mod+1..3` pick a tab as a fresh press, and `mod+enter` starts the
/// job of the Text or Files tab while it shows. True when `key` was one of them. A focused field
/// takes these chords for itself, so each field runs this before its own keys.
pub fn window_key(s: &mut AppState, key: &KeyEvent, tx: &mut Tx) -> bool {
    if let Some(tab) = area_chord(Tab::ALL.len(), key, tx).and_then(|index| Tab::ALL.get(index)) {
        dispatch(s, tx, Input::TabPicked(*tab));
        return true;
    }
    if is_chord("mod+enter", key, tx) {
        let start = match s.tab {
            Tab::Text => Input::Text(TextInput::StartPressed),
            Tab::Files => Input::Files(FilesInput::StartPressed),
            Tab::Keys => return false,
        };
        dispatch(s, tx, start);
        return true;
    }
    false
}

/// The window's root.
pub fn view(s: &AppState, cx: &mut Cx<AppState>) -> El {
    let k = s.theme.kit(cx.system_dark());
    let tabs = choice_switch(
        k,
        TABS_KEY,
        "Tab",
        &Tab::LABELS,
        s.tab.index(),
        true,
        |s: &mut AppState, index: usize, tx: &mut Tx| {
            if let Some(tab) = Tab::ALL.get(index) {
                dispatch(s, tx, Input::TabPicked(*tab));
            }
        },
    );
    let header = col()
        .w_full()
        .padding(SPACE_LG)
        .items_center()
        .child(row().w_full().max_w(FORM_MAX_WIDTH).child(tabs));
    let tab = match s.tab {
        Tab::Text => text_tab::view(s, k, cx).key(TEXT_TAB_KEY),
        Tab::Files => files_tab::view(s, k, cx).key(FILES_TAB_KEY),
        Tab::Keys => placeholder(s, k).key(KEYS_TAB_KEY),
    };
    let window = col()
        .w_full()
        .h_full()
        .fill(surface(k, Surface::Window))
        .child(header)
        .child(hairline(k))
        .child(
            col()
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .fill(surface(k, Surface::Content))
                .child(tab.transition(Transition::opacity().springs(CONTROL_PAIR))),
        );
    rune_kit::root(window, Vec::new())
}

/// `tab`'s status line keyed `key`, when it has something to say. While a job runs, assistive
/// technology hears the job's phrase without a running count, so a batch is announced once.
fn tab_status(s: &AppState, k: &Kit, tab: Tab, key: &str) -> Option<El> {
    let (tone, words) = s.status(tab)?;
    let line = status_line(k, key, tone, StatusLines::Wrap, &words);
    Some(match s.status_spoken(tab) {
        Some(spoken) => line.label(spoken),
        None => line,
    })
}

/// The captions every tab shows between its body and its action row while a window-wide
/// condition stands; `None` when none does.
fn standing_captions(s: &AppState, k: &Kit) -> Option<El> {
    match s.work {
        Work::Stopped => Some(
            col()
                .w_full()
                .px(SPACE_LG)
                .pb(SPACE_SM)
                .items_center()
                .child(col().w_full().max_w(FORM_MAX_WIDTH).child(status_line(
                    k,
                    STOPPED_KEY,
                    Tone::Danger,
                    StatusLines::Wrap,
                    STOPPED_CAPTION,
                ))),
        ),
        Work::Idle | Work::Running(_) => None,
    }
}

/// A tab that is not built yet: one muted line, centred, over the standing captions.
fn placeholder(s: &AppState, k: &Kit) -> El {
    let mut tab = col().w_full().grow(1.0).min_h(0.0).child(
        col()
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .center()
            .child(caption(k, PLACEHOLDER)),
    );
    if let Some(captions) = standing_captions(s, k) {
        tab = tab.child(captions);
    }
    tab
}

#[cfg(test)]
mod files_tests;
#[cfg(test)]
mod tests;

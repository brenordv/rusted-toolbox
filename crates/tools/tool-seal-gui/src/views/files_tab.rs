//! The Files tab: Mode and the overwrite choice, the file list (the drop target), the
//! recipients or the identity file, where the outputs go, and the action row.

use super::parts::{BadgeTone, ListHeight, ListRows, badge, drop_hovered, list_box};
use super::picker::{identity, recipients};
use super::text_tab::{DIRECTION_LABELS, DIRECTIONS};
use super::{standing_captions, tab_status};
use crate::design::{
    FILE_BADGE_WIDTH, FILE_SIZE_WIDTH, FILES_MIN_ROWS, FILES_RECIPIENT_ROWS, FORM_MAX_WIDTH,
};
use crate::services::dispatch;
use crate::state::files::{FileRow, FilesInput, OutputChoice, RowStatus};
use crate::state::recipients::Picker;
use crate::state::{AppState, Blocked, DialogPurpose, Input, Tab};
use common_gui_rune::parts::{
    action_row, choice_switch, detail_row, field_label, issue, reveal, section_header, warning,
};
use common_gui_rune::tokens::{
    CONTROL_HEIGHT, SPACE_LG, SPACE_MD, SPACE_SM, SPACE_XL, SPACE_XS, TYPE_CAPTION, TYPE_LABEL,
    TYPE_MONO,
};
use common_utils::string_utils::{display_line, format_bytes_to_string};
use rune::{Align, Cx, El, GrowthSpec, ScrollSpec, Tx, col, row, spacer, text};
use rune_kit::{Button, ButtonVariant, Checkbox, Kit, RadioGroup, RadioItem, ScrollBar};
use seal_crypto::Direction;
use std::borrow::Cow;
use std::path::PathBuf;
use unicode_segmentation::UnicodeSegmentation;

/// The Files tab's scroll viewport.
pub const FILES_SCROLL_KEY: &str = "seal-files-scroll";
/// What the scroll viewport holds: the form with its insets.
pub const FILES_FORM_KEY: &str = "seal-files-form";
/// The Seal/Unseal switch.
pub const MODE_KEY: &str = "seal-files-mode";
/// "Overwrite existing files".
pub const OVERWRITE_KEY: &str = "seal-files-overwrite";
/// "Add files…".
pub const ADD_FILES_KEY: &str = "seal-files-add";
/// "Clear list".
pub const CLEAR_LIST_KEY: &str = "seal-files-clear";
/// The file list, which is the drop target.
pub const FILE_LIST_KEY: &str = "seal-files-list";
/// The column inside the file list that builds only the rows in view.
pub const FILE_ROWS_KEY: &str = "seal-files-rows";
/// The recipient list.
pub const FILES_RECIPIENTS_KEY: &str = "seal-files-recipients";
/// The Other key field.
pub const FILES_OTHER_KEY_KEY: &str = "seal-files-other-key";
/// The identity file row.
pub const FILES_IDENTITY_KEY: &str = "seal-files-identity";
/// "Choose identity file…".
pub const FILES_CHOOSE_IDENTITY_KEY: &str = "seal-files-choose-identity";
/// The output choice.
pub const OUTPUT_CHOICE_KEY: &str = "seal-files-output";
/// The output folder row.
pub const OUTPUT_FOLDER_KEY: &str = "seal-files-output-folder";
/// "Choose folder…".
pub const CHOOSE_FOLDER_KEY: &str = "seal-files-choose-folder";
/// The action row.
pub const FILES_ACTIONS_KEY: &str = "seal-files-actions";
/// The action row's status line.
pub const FILES_STATUS_KEY: &str = "seal-files-status";
/// Seal files or Unseal files.
pub const FILES_SUBMIT_KEY: &str = "seal-files-submit";

const OVERWRITE_LABEL: &str = "Overwrite existing files";
/// What a row says when its output exists and overwriting is off.
pub const MSG_OUTPUT_EXISTS: &str = "The output exists; this row will be skipped.";

/// The key of the row at `index`.
pub fn file_row_key(index: usize) -> String {
    format!("seal-files-row-{index}")
}

/// The key of the Remove button of the row at `index`.
pub fn remove_key(index: usize) -> String {
    format!("seal-files-remove-{index}")
}

/// The key of the path text of the row at `index`.
pub fn file_path_key(index: usize) -> String {
    format!("seal-files-path-{index}")
}

/// `text` with a zero-width space between each two graphemes, so a layout that breaks lines only
/// between words can break it anywhere. A grapheme (a letter with its combining marks, an emoji
/// sequence) is never split.
pub(super) fn breakable(text: &str) -> String {
    let mut graphemes = text.graphemes(true);
    let mut out = String::with_capacity(text.len() * 2);
    if let Some(first) = graphemes.next() {
        out.push_str(first);
    }
    for grapheme in graphemes {
        out.push('\u{200B}');
        out.push_str(grapheme);
    }
    out
}

/// The Files tab.
pub fn view(s: &AppState, k: &Kit, cx: &mut Cx<AppState>) -> El {
    let viewport = cx.layout_with(FILES_SCROLL_KEY, |rect| rect[3]);
    let system_dark = cx.system_dark();
    let choice_kit = s.choice_kits.kit(s.theme.dark(system_dark));
    let missing = s.missing(Tab::Files);
    let form = col()
        .w_full()
        .max_w(FORM_MAX_WIDTH)
        .grow(1.0)
        .child(mode_row(s, (k, choice_kit)))
        .child(file_list(s, k, system_dark))
        .child(match s.files.mode {
            Direction::Encrypt => reveal(recipients(
                s,
                (k, choice_kit),
                Picker::Files,
                missing,
                FILES_RECIPIENT_ROWS,
            ))
            .key("seal-files-recipients-group"),
            Direction::Decrypt => {
                reveal(identity(s, k, Picker::Files)).key("seal-files-identity-group")
            }
        })
        .child(output(s, (k, choice_kit)));
    let body = col()
        .key(FILES_SCROLL_KEY)
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(
            col()
                .key(FILES_FORM_KEY)
                .w_full()
                .min_h(viewport.unwrap_or(0.0))
                .padding(SPACE_LG)
                .items_center()
                .child(form),
        )
        .scroll_bar(k);
    let mut tab = col().w_full().grow(1.0).min_h(0.0).child(body);
    if let Some(captions) = standing_captions(s, k) {
        tab = tab.child(captions);
    }
    tab.child(actions(s, k))
}

/// Mode over its switch, and beside the switch "Overwrite existing files".
fn mode_row(s: &AppState, (k, choice_kit): (&Kit, &Kit)) -> El {
    let selected = DIRECTIONS
        .iter()
        .position(|direction| *direction == s.files.mode)
        .unwrap_or(0);
    let toggle = |s: &mut AppState, tx: &mut Tx| {
        dispatch(s, tx, Input::Files(FilesInput::OverwriteToggled));
    };
    let overwrite = row()
        .h(CONTROL_HEIGHT)
        .gap(SPACE_SM)
        .center_y()
        .on_press_tx(toggle)
        .child(
            Checkbox::new(s.files.overwrite)
                .label(OVERWRITE_LABEL)
                .hide_label()
                .on_toggle_tx(toggle)
                .build(choice_kit)
                .key(OVERWRITE_KEY),
        )
        .child(
            text(OVERWRITE_LABEL)
                .font(TYPE_LABEL)
                .color(k.c.text_base)
                .a11y_hidden(),
        );
    row()
        .w_full()
        .gap(SPACE_XL)
        .items_end()
        .child(
            col()
                .gap(SPACE_XS)
                .child(field_label(k, "Mode").a11y_hidden())
                .child(choice_switch(
                    k,
                    MODE_KEY,
                    "Mode",
                    &DIRECTION_LABELS,
                    selected,
                    !s.running_on(Tab::Files),
                    |s: &mut AppState, index: usize, tx: &mut Tx| {
                        if let Some(mode) = DIRECTIONS.get(index) {
                            dispatch(s, tx, Input::Files(FilesInput::ModePicked(*mode)));
                        }
                    },
                )),
        )
        .child(overwrite)
}

/// The Files header with its two actions over the list, which fills the space the form leaves,
/// takes dropped files, and builds only the rows in view, drawn with the kit for `system_dark`.
fn file_list(s: &AppState, k: &Kit, system_dark: bool) -> El {
    let running = s.running_on(Tab::Files);
    let add = Button::new("Add files…")
        .variant(ButtonVariant::Ghost)
        .disabled(s.dialog.is_some())
        .on_press_tx(|s: &mut AppState, tx: &mut Tx| {
            dispatch(s, tx, Input::DialogOpened(DialogPurpose::AddFile));
        })
        .build(k)
        .key(ADD_FILES_KEY);
    let clear = Button::new("Clear list")
        .variant(ButtonVariant::Ghost)
        .disabled(running || s.files.rows.is_empty())
        .on_press_tx(|s: &mut AppState, tx: &mut Tx| {
            dispatch(s, tx, Input::Files(FilesInput::ClearList));
        })
        .build(k)
        .key(CLEAR_LIST_KEY);
    let count = s.files.rows.len();
    let column = col()
        .key(FILE_ROWS_KEY)
        .w_full()
        .gap(SPACE_XS)
        .growth(GrowthSpec::default())
        .virtual_rows(
            count,
            CONTROL_HEIGHT,
            |_: &AppState, index| Cow::Owned(file_row_key(index)),
            move |s: &AppState, index| file_row_at(s, system_dark, index),
        );
    let mut list = list_box(
        k,
        (FILE_LIST_KEY, "Files"),
        ListRows::Virtual { count, column },
        "Drop files here, or use Add files…",
        ListHeight::Fill {
            min_rows: FILES_MIN_ROWS,
        },
    )
    .on_drop_files(|s: &mut AppState, paths: &[PathBuf], tx: &mut Tx| {
        dispatch(s, tx, Input::Files(FilesInput::AddPaths(paths.to_vec())));
    })
    .on_drop_hover(|s: &mut AppState, over: bool, tx: &mut Tx| {
        dispatch(s, tx, Input::Files(FilesInput::DropHover(over)));
    });
    if s.files.drop_hover {
        list = drop_hovered(k, list);
    }
    col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .child(
            row()
                .w_full()
                .mt(SPACE_LG)
                .mb(SPACE_SM)
                .gap(SPACE_SM)
                .center_y()
                .child(section_header(k, "Files").mt(0.0))
                .child(spacer())
                .child(add)
                .child(clear),
        )
        .child(list)
}

/// The badge and its words for a row's status.
fn badge_for(status: &RowStatus) -> (&'static str, BadgeTone) {
    match status {
        RowStatus::Pending => ("Pending", BadgeTone::Waiting),
        RowStatus::Invalid(_) => ("Invalid", BadgeTone::Attention),
        RowStatus::Running => ("Running", BadgeTone::Running),
        RowStatus::Done(_) => ("Done", BadgeTone::Finished),
        RowStatus::Failed(_) => ("Failed", BadgeTone::Failed),
        RowStatus::Skipped => ("Skipped", BadgeTone::Attention),
    }
}

/// The row at `index` of the file list, drawn with the kit for `system_dark`. An index past the
/// rows gets an empty row of a row's height: the list asks for its rows by index after counting
/// them, and a row removed in between leaves the last index with nothing to show.
pub(super) fn file_row_at(s: &AppState, system_dark: bool, index: usize) -> El {
    let Some(file) = s.files.rows.get(index) else {
        return row().h(CONTROL_HEIGHT);
    };
    let running = s.running_on(Tab::Files);
    file_row(
        s.theme.kit(system_dark),
        index,
        file,
        (!running && !s.files.overwrite, !running),
    )
}

/// One file: its badge, the path on one line (cut in the middle when it does not fit, with the
/// whole path as a tooltip) with its message under it, the size, and Remove. The row is one node
/// named by its status and file name, described by its message; Remove is named with the file
/// name. `warn_skip` says whether a pending row whose output exists is marked as one the next run
/// skips; `removable` enables Remove.
fn file_row(k: &Kit, index: usize, file: &FileRow, (warn_skip, removable): (bool, bool)) -> El {
    let (status, tone) = badge_for(&file.status);
    let path = display_line(file.path.display().to_string().chars());
    let name = file.path.file_name().map_or_else(
        || path.clone(),
        |name| display_line(name.to_string_lossy().chars()),
    );
    let message = match &file.status {
        RowStatus::Invalid(message) => Some((true, (*message).to_owned())),
        RowStatus::Failed(message) => Some((true, display_line(message.chars()))),
        RowStatus::Pending if file.output_exists && warn_skip => {
            Some((false, MSG_OUTPUT_EXISTS.to_owned()))
        }
        RowStatus::Pending | RowStatus::Running | RowStatus::Done(_) | RowStatus::Skipped => None,
    };
    let removed = file.path.clone();
    let remove = Button::new("Remove")
        .variant(ButtonVariant::Ghost)
        .disabled(!removable)
        .on_press_tx(move |s: &mut AppState, tx: &mut Tx| {
            dispatch(s, tx, Input::Files(FilesInput::RemoveRow(removed.clone())));
        })
        .build(k)
        .key(remove_key(index))
        .label(format!("Remove {name}"));
    // Each text sits centred on the path line's control height and stays at the top when the
    // message under the path wraps.
    let mono_inset = (CONTROL_HEIGHT - TYPE_MONO.line_px()) / 2.0;
    let badge_inset = (CONTROL_HEIGHT - TYPE_CAPTION.line_px()) / 2.0;
    let tooltip = breakable(&path);
    let mut main = col()
        .grow(1.0)
        .min_w(0.0)
        .gap(SPACE_XS)
        .pt(mono_inset)
        .child(
            text(path)
                .key(file_path_key(index))
                .font(TYPE_MONO)
                .color(k.c.text_base)
                .truncate_middle()
                .help_when_truncated(tooltip)
                .a11y_hidden(),
        );
    let mut entry = row()
        .key(file_row_key(index))
        .role("group")
        .label(format!("{status}: {name}"))
        .w_full()
        .min_h(CONTROL_HEIGHT)
        .gap(SPACE_SM);
    if let Some((is_issue, line)) = message {
        let caption = if is_issue {
            issue(k, &line)
        } else {
            warning(k, &line)
        };
        main = main.child(caption.a11y_hidden());
        entry = entry.description(line);
    }
    entry
        .child(badge(k, status, tone, FILE_BADGE_WIDTH).mt(badge_inset))
        .child(main)
        .child(
            text(format_bytes_to_string(file.size))
                .font(TYPE_MONO)
                .color(k.c.text_muted)
                .align(Align::End)
                .w(FILE_SIZE_WIDTH)
                .mt(mono_inset)
                .a11y_hidden(),
        )
        .child(remove)
}

/// Where the outputs go: next to each input, or into the folder shown under the choice.
fn output(s: &AppState, (k, choice_kit): (&Kit, &Kit)) -> El {
    let choice = s.files.out_choice;
    let item = |label: &str, value: OutputChoice| {
        RadioItem::new(label, choice == value).on_select_tx(move |s: &mut AppState, tx: &mut Tx| {
            dispatch(s, tx, Input::Files(FilesInput::OutputPicked(value)));
        })
    };
    let radios = RadioGroup::new()
        .label("Output")
        .item(item("Next to each input", OutputChoice::NextToInput))
        .item(item("Folder", OutputChoice::Folder))
        .build(choice_kit)
        .key(OUTPUT_CHOICE_KEY);
    let mut group = col()
        .w_full()
        .child(section_header(k, "Output").mb(SPACE_SM))
        .child(radios);
    match choice {
        OutputChoice::NextToInput => {}
        OutputChoice::Folder => {
            let shown = s.files.out_dir.as_ref().map_or_else(
                || "None chosen".to_owned(),
                |path| display_line(path.display().to_string().chars()),
            );
            let choose = Button::new("Choose folder…")
                .variant(ButtonVariant::Ghost)
                .disabled(s.dialog.is_some())
                .on_press_tx(|s: &mut AppState, tx: &mut Tx| {
                    dispatch(s, tx, Input::DialogOpened(DialogPurpose::OutputFolder));
                })
                .build(k)
                .key(CHOOSE_FOLDER_KEY);
            group = group.child(
                reveal(
                    row()
                        .w_full()
                        .mt(SPACE_MD)
                        .gap(SPACE_MD)
                        .child(
                            col()
                                .grow(1.0)
                                .min_w(0.0)
                                .key(OUTPUT_FOLDER_KEY)
                                .child(detail_row(k, "Output folder", &shown)),
                        )
                        .child(choose),
                )
                .key("seal-files-folder-group"),
            );
        }
    }
    group
}

fn actions(s: &AppState, k: &Kit) -> El {
    let label = match s.files.mode {
        Direction::Encrypt => "Seal files",
        Direction::Decrypt => "Unseal files",
    };
    let (variant, disabled) = match s.blocked() {
        None => (ButtonVariant::Submit, false),
        Some(Blocked::Running(job)) if job.tab() == Tab::Files => (ButtonVariant::Loading, false),
        Some(Blocked::Running(_) | Blocked::Stopped) => (ButtonVariant::Submit, true),
    };
    let submit = Button::new(label)
        .variant(variant)
        .disabled(disabled)
        .on_press_tx(|s: &mut AppState, tx: &mut Tx| {
            dispatch(s, tx, Input::Files(FilesInput::StartPressed));
        })
        .build(k)
        .key(FILES_SUBMIT_KEY)
        .default_button();
    action_row(
        k,
        FILES_ACTIONS_KEY,
        tab_status(s, k, Tab::Files, FILES_STATUS_KEY),
        vec![submit],
    )
}

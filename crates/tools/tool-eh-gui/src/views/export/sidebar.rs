//! The Export sidebar: the archive search form.

use crate::app::dispatch;
use crate::archive::ArchiveInput;
use crate::design::parts;
use crate::design::tokens::{SPACE_LG, SPACE_MD, SPACE_SM};
use crate::state::{AppState, Input, ListSection};
use crate::views::rules_form;
use rune::{El, col, row};
use rune_kit::{Button, ButtonVariant, Kit};

pub fn sidebar(s: &AppState, k: &Kit, layer: &mut Vec<El>) -> El {
    let archive = &s.archive;
    let issues = archive.form.parsed().err().unwrap_or_default();
    let form = col()
        .key("archive-search")
        .gap(SPACE_MD)
        .px(SPACE_LG)
        .child(parts::section_header(k, "Search archive"))
        .child(parts::caption(
            k,
            "Find archived messages by enqueue time, text, or both.",
        ));
    let search = Button::new("Search")
        .variant(if archive.fetch_in_flight() {
            ButtonVariant::Loading
        } else {
            ButtonVariant::Submit
        })
        .disabled(!archive.searchable())
        .on_press(|s: &mut AppState| dispatch(s, Input::Archive(ArchiveInput::SearchPressed)))
        .build(k)
        .key("archive-search-button");
    let clear = Button::new("Clear")
        .variant(ButtonVariant::Ghost)
        .on_press(|s: &mut AppState| dispatch(s, Input::Archive(ArchiveInput::ClearForm)))
        .build(k)
        .key("archive-clear");
    rules_form::fields(s, k, layer, ListSection::Export, &issues, form)
        .child(row().gap(SPACE_SM).child(search).child(clear))
        .child(parts::caption(k, "Works without a connection."))
}

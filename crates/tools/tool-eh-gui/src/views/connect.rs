//! The connection sheet: the profile, auth method, the fields it needs, storage, and Save /
//! Connect.

mod profile;

pub(crate) use profile::move_profile_focus;
pub use profile::{
    GROUP_KEY as PROFILE_GROUP_KEY, NAME_ISSUE_KEY, NEW_PROFILE_CANCEL_KEY, NEW_PROFILE_KEY,
    NEW_PROFILE_NAME_KEY, NEW_PROFILE_ROW_KEY, NEW_PROFILE_SAVE_KEY, NOTE_KEY as PROFILE_NOTE_KEY,
    PICKER_KEY, ROW_ISSUE_KEY,
};

use super::details;
use crate::app::dispatch;
use crate::design::parts::{self, SheetFrame};
use crate::design::tokens::{SHEET_WIDTH, SPACE_MD, SPACE_SM, SPACE_XL, SPACE_XS};
use crate::form::{
    AuthChoice, CredentialChoice, DISK_WRITE_LABELS, DISK_WRITE_MODES, FieldIssues, SecretSource,
    disk_write_index,
};
use crate::read::rows::display_path;
use crate::state::{
    AppState, Input, RECALLED_FIELD, SaveStatus, SaveTo, SheetPurpose, ToastKind, error_preview,
};
use rune::{El, FileDialogOptions, ScrollSpec, TextEdit, Tx, col, row, spacer};
use rune_kit::{
    Button, ButtonVariant, IconButton, IconButtonVariant, Kit, ScrollBar, SegmentedControl,
    TextField, TextInput,
};
use shared_eventhub_persistence::export::DiskWriteMode;
use tracing::warn;

type EditAccess = fn(&mut AppState) -> &mut TextEdit;

/// The connection sheet while it is open. A closed sheet builds nothing, so its secret fields
/// are never mounted, focusable, or exposed to assistive technology.
pub fn sheet(s: &AppState, k: &Kit, layer: &mut Vec<El>) {
    let Some(purpose) = s.sheet else {
        return;
    };
    let issues = if s.show_issues {
        s.form.issues()
    } else {
        FieldIssues::default()
    };
    // The kit paints overlays in the order they are pushed, and the sheet pushes its scrim
    // after the body is built, so the picker's list goes on after the sheet.
    let mut overlays = Vec::new();
    let mut body = col()
        .key("connect-form")
        .w_full()
        .gap(SPACE_MD)
        .px(SPACE_XL)
        .pt(SPACE_SM)
        .pb(SPACE_XL)
        .child(profile::group(s, k, &mut overlays))
        .child(
            SegmentedControl::new(&AuthChoice::LABELS, Some(s.form.auth.index()))
                .label("Sign-in method")
                .fit()
                .on_pick(|s: &mut AppState, index: usize, _| {
                    if let Some(auth) = AuthChoice::ALL.get(index) {
                        s.form.set_auth(*auth);
                    }
                })
                .build(k),
        );
    if let Some(caption) = &s.config_caption {
        let preview = error_preview(caption);
        body = body.child(parts::caption(k, &preview.shown));
        if preview.cut {
            body = body.child(details::button(k, "config-details", open_config_caption));
        }
    }
    body = method_fields(s, k, &issues, body)
        .child(field(
            k,
            "Consumer group",
            &s.form.consumer_group,
            &issues.consumer_group,
            |s| &mut s.form.consumer_group,
        ))
        .child(storage(s, k, &issues));

    let scroller = col()
        .key("connect-scroll")
        .w_full()
        .shrink()
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(body)
        .scroll_bar(k);
    let title = match purpose {
        SheetPurpose::Connect => "Connect to an event hub",
        SheetPurpose::SendTarget => "Choose where to send",
    };
    parts::sheet(
        k,
        SheetFrame {
            key: "connect",
            title,
            width: SHEET_WIDTH,
        },
        |s: &mut AppState, _: &mut Tx| dispatch(s, Input::CloseSheet),
        scroller,
        footer(s, k, purpose),
        layer,
    );
    layer.extend(overlays);
}

/// Opens the whole config.json caption; nothing once the caption is gone.
fn open_config_caption(s: &mut AppState, _: &mut rune::Tx) {
    let Some(caption) = s.config_caption.clone() else {
        return;
    };
    details::show(s, "config.json", &caption);
}

fn method_fields(s: &AppState, k: &Kit, issues: &FieldIssues, mut body: El) -> El {
    let form = &s.form;
    match form.auth {
        AuthChoice::ConnectionString => {
            body = body.child(secret(
                k,
                "connection-string",
                "Connection string",
                &form.connection_string,
                &issues.connection_string,
                |s| s.form.connection_string.edit_mut(),
            ));
        }
        AuthChoice::Credential => {}
        AuthChoice::KeyVault => {
            body = body
                .child(field(
                    k,
                    "Vault URL",
                    &form.vault_url,
                    &issues.vault_url,
                    |s| &mut s.form.vault_url,
                ))
                .child(field(
                    k,
                    "Secret name",
                    &form.secret_name,
                    &issues.secret_name,
                    |s| &mut s.form.secret_name,
                ));
        }
    }
    let names_note = match form.auth {
        AuthChoice::Credential => "Namespace",
        AuthChoice::ConnectionString | AuthChoice::KeyVault => "Namespace (optional)",
    };
    let marks = s.recall_marks();
    let namespace = field(k, names_note, &form.namespace, &issues.namespace, |s| {
        &mut s.form.namespace
    });
    let event_hub = field(k, "Event hub", &form.event_hub, &issues.event_hub, |s| {
        &mut s.form.event_hub
    });
    body = body
        .child(recall_marked(
            k,
            namespace,
            marks.namespace && issues.namespace.is_none(),
        ))
        .child(recall_marked(
            k,
            event_hub,
            marks.event_hub && issues.event_hub.is_none(),
        ));
    if form.auth != AuthChoice::ConnectionString {
        body = body.child(credential(s, k, issues));
    }
    body
}

/// A name field with [`RECALLED_FIELD`] as a muted caption under it while `marked`. The field
/// always sits in the same column, so the mark coming or going never remounts it.
fn recall_marked(k: &Kit, field: El, marked: bool) -> El {
    let mut group = col().gap(SPACE_XS).w_full().child(field);
    if marked {
        group = group.child(parts::caption(k, RECALLED_FIELD));
    }
    group
}

fn credential(s: &AppState, k: &Kit, issues: &FieldIssues) -> El {
    let form = &s.form;
    let mut group = col().gap(SPACE_MD).w_full().child(
        SegmentedControl::new(&CredentialChoice::LABELS, Some(form.credential.index()))
            .label("Azure credential")
            .fit()
            .on_pick(|s: &mut AppState, index: usize, _| {
                if let Some(credential) = CredentialChoice::ALL.get(index) {
                    s.form.credential = *credential;
                }
            })
            .build(k),
    );
    if form.uses_client_secret() {
        group = group
            .child(field(
                k,
                "Tenant id",
                &form.tenant_id,
                &issues.tenant_id,
                |s| &mut s.form.tenant_id,
            ))
            .child(field(
                k,
                "Client id",
                &form.client_id,
                &issues.client_id,
                |s| &mut s.form.client_id,
            ))
            .child(secret(
                k,
                "client-secret",
                "Client secret",
                &form.client_secret,
                &issues.client_secret,
                |s| s.form.client_secret.edit_mut(),
            ));
    }
    group
}

fn storage(s: &AppState, k: &Kit, issues: &FieldIssues) -> El {
    let form = &s.form;
    let lock = s.data_folder_lock();
    let folder = display_path(s.data_paths().base());
    let mut group = col()
        .gap(SPACE_MD)
        .w_full()
        .child(parts::section_header(k, "Storage"))
        .child(
            row()
                .gap(SPACE_SM)
                .w_full()
                .items_end()
                .child(parts::detail_row(k, "Data folder", &folder).grow(1.0))
                .child(
                    Button::new("Change")
                        .variant(ButtonVariant::Outline)
                        .disabled(lock.is_some())
                        .on_press_tx(choose_data_folder)
                        .build(k)
                        .key("change-folder"),
                ),
        );
    if let Some(reason) = lock {
        group = group.child(parts::caption(k, reason));
    }
    group = group.child(parts::caption(
        k,
        "One data folder holds the archive of one event hub. Keep one config.json per data \
         folder.",
    ));
    if let Some(issue) = &issues.data_folder {
        group = group.child(parts::caption(k, issue));
    }
    group = group.child(
        SegmentedControl::new(&DISK_WRITE_LABELS, Some(disk_write_index(form.disk_write)))
            .label("Message files")
            .fit()
            .on_pick(|s: &mut AppState, index: usize, _| {
                if let Some(mode) = DISK_WRITE_MODES.get(index) {
                    s.form.disk_write = *mode;
                }
            })
            .build(k),
    );
    if form.disk_write != DiskWriteMode::Off {
        group = group.child(field(
            k,
            "Message files folder",
            &form.received_msg_path,
            &issues.received_msg_path,
            |s| &mut s.form.received_msg_path,
        ));
    }
    group
}

fn choose_data_folder(_: &mut AppState, tx: &mut rune::Tx) {
    let options = FileDialogOptions {
        title: "Choose the data folder".to_owned(),
        ..FileDialogOptions::default()
    };
    tx.open_folder(options, |s: &mut AppState, picked, _| match picked {
        Ok(Some(folder)) => dispatch(s, Input::DataFolderPicked(folder)),
        Ok(None) => {}
        Err(error) => {
            warn!(error = %error, "the folder picker failed");
            s.toast(
                ToastKind::Danger,
                format!("Could not open the folder picker: {error}"),
            );
        }
    });
}

fn footer(s: &AppState, k: &Kit, purpose: SheetPurpose) -> El {
    let caption = match &s.save_status {
        SaveStatus::Idle => None,
        SaveStatus::Saving(SaveTo::Home(_)) => Some("Saving config.json".to_owned()),
        SaveStatus::Saving(SaveTo::NewProfile { name, .. }) => {
            Some(format!("Saving profile {}", name.as_str()))
        }
        SaveStatus::Saved => Some("Saved config.json".to_owned()),
        SaveStatus::Failed(error) => Some(error_preview(&format!("Not saved: {error}")).shown),
    };
    let mut bar = row()
        .key("connect-footer")
        .gap(SPACE_SM)
        .w_full()
        .center_y()
        .child(
            Button::new("Cancel")
                .variant(ButtonVariant::Ghost)
                .on_press(|s: &mut AppState| dispatch(s, Input::CloseSheet))
                .build(k)
                .key("cancel"),
        )
        .child(spacer());
    if let Some(caption) = caption {
        bar = bar.child(parts::caption(k, &caption));
    }
    bar.child(
        Button::new("Save config")
            .variant(ButtonVariant::Outline)
            .disabled(s.save_status.is_saving())
            .on_press(|s: &mut AppState| dispatch(s, Input::SavePressed))
            .build(k)
            .key("save-config"),
    )
    .child(match purpose {
        SheetPurpose::Connect => Button::new("Connect")
            .variant(ButtonVariant::Submit)
            .on_press(|s: &mut AppState| dispatch(s, Input::ConnectPressed))
            .build(k)
            .key("sheet-connect")
            .default_button(),
        SheetPurpose::SendTarget => Button::new("Done")
            .variant(ButtonVariant::Submit)
            .on_press(|s: &mut AppState| dispatch(s, Input::CloseSheet))
            .build(k)
            .key("sheet-done")
            .default_button(),
    })
}

fn field(k: &Kit, label: &str, edit: &TextEdit, issue: &Option<String>, access: EditAccess) -> El {
    let mut field = TextField::new(label, edit).on_edit(access);
    if let Some(issue) = issue {
        field = field.error(issue.clone());
    }
    field.build(k)
}

/// A secret field: typed (hidden, with a reveal toggle) or, when stored, a note with Replace.
fn secret(
    k: &Kit,
    key: &str,
    label: &str,
    source: &SecretSource,
    issue: &Option<String>,
    access: EditAccess,
) -> El {
    if source.is_stored() {
        return row()
            .key(key)
            .gap(SPACE_SM)
            .w_full()
            .items_end()
            .child(parts::detail_row(k, label, "Saved in local store").grow(1.0))
            .child(
                Button::new("Replace")
                    .variant(ButtonVariant::Outline)
                    .on_press(move |s: &mut AppState| {
                        access(s);
                    })
                    .build(k),
            );
    }

    let revealed = source.edit().is_revealed();
    let toggle = IconButton::new(if revealed {
        k.glyph.eye_off
    } else {
        k.glyph.eye
    })
    .variant(IconButtonVariant::Ghost)
    .label(if revealed { "Hide" } else { "Show" })
    .on_press(move |s: &mut AppState| {
        let edit = access(s);
        let shown = edit.is_revealed();
        edit.set_revealed(!shown);
    })
    .build(k);
    let mut group = col().key(key).gap(SPACE_SM).w_full().child(
        TextInput::new(source.edit())
            .secure()
            .revealed(revealed)
            .label(label)
            .width(None)
            .placeholder(label)
            .invalid(issue.is_some())
            .trailing(toggle)
            .on_edit(access)
            .build(k),
    );
    if let Some(issue) = issue {
        group = group.child(parts::caption(k, issue));
    }
    group
}

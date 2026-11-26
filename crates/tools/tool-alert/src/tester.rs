//! The tester window (`--gui`): a title, a body, a sound file, and Test, which shows the alert
//! by starting this same program with the form's values as its arguments.

use crate::design::BODY_LINES;
use crate::models::TesterSeed;
use anyhow::{Context, Result};
use common_gui_rune::keys::{is_chord, type_newline};
use common_gui_rune::parts::{
    EditorHeight, EditorSpec, LineFieldSpec, StatusLines, Tone, field_label, line_field,
    status_line, text_editor,
};
use common_gui_rune::tokens::{SPACE_LG, SPACE_MD, SPACE_XS, TYPE_BODY};
use common_gui_rune::{Surface, Theme, surface};
use rune::{Cx, El, KeyEvent, TextEdit, Tx, col, row, spacer};
use rune_kit::{Button, ButtonVariant, Kit};
use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use tracing::{info, warn};

/// The Title field.
pub const TITLE_KEY: &str = "tester-title";
/// The Body editor.
pub const BODY_KEY: &str = "tester-body";
/// The Sound file field.
pub const SOUND_KEY: &str = "tester-sound";
/// The line under the fields saying why nothing was shown.
pub const ISSUE_KEY: &str = "tester-issue";
/// The Test button.
pub const TEST_KEY: &str = "tester-test";

/// The tester window's title in the OS.
pub const OS_TITLE: &str = "alert tester";
/// What Test says when the body is blank.
pub const BLANK_BODY: &str = "Type a body for the alert.";

/// Starts an alert with these arguments.
pub type StartAlert = Box<dyn Fn(&[OsString]) -> Result<()>>;

/// The command-line arguments that show an alert with these values: `--title=<title>` and
/// `--sound=<sound>` when they are not blank, then `--` and the body, so a value that starts
/// with a dash is never read as a flag.
pub fn alert_arguments(title: &str, body: &str, sound: &str) -> Vec<OsString> {
    let mut arguments = Vec::new();
    if !title.trim().is_empty() {
        arguments.push(OsString::from(format!("--title={title}")));
    }
    if !sound.trim().is_empty() {
        arguments.push(OsString::from(format!("--sound={sound}")));
    }
    arguments.push(OsString::from("--"));
    arguments.push(OsString::from(body));
    arguments
}

/// Starts `executable` with `arguments` as an argument vector, never through a shell, with no
/// standard streams, and leaves a thread waiting on it so the child is reaped when it ends.
///
/// # Errors
/// When the program cannot be started.
pub fn start_alert(executable: &Path, arguments: &[OsString]) -> Result<()> {
    let mut child = Command::new(executable)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("Failed to start the alert program")?;
    let pid = child.id();
    info!(pid, "alert tester: child started");
    thread::spawn(move || match child.wait() {
        Ok(status) => info!(pid, status = %status, "alert tester: child ended"),
        // Best-effort: the wait only reaps the child and logs how it ended.
        Err(error) => warn!(pid, error = %error, "alert tester: could not wait for the child"),
    });
    Ok(())
}

/// The tester's form and what the last Test found.
pub struct TesterState {
    theme: Theme,
    title: TextEdit,
    body: TextEdit,
    sound: TextEdit,
    issue: Option<String>,
    start: StartAlert,
}

impl TesterState {
    /// The form filled from `seed`; Test starts alerts with `start`.
    pub fn new(seed: &TesterSeed, start: StartAlert) -> Self {
        Self {
            theme: Theme::new(),
            title: TextEdit::new(seed.title.as_str()),
            body: TextEdit::new(seed.body.as_str()),
            sound: TextEdit::new(seed.sound.as_str()),
            issue: None,
            start,
        }
    }

    /// An empty form whose Test reports that it can start nothing.
    pub fn detached() -> Self {
        Self::new(
            &TesterSeed::default(),
            Box::new(|_| anyhow::bail!("The tester is not set up")),
        )
    }

    /// Why the last Test showed nothing, when it did not.
    pub fn issue(&self) -> Option<&str> {
        self.issue.as_deref()
    }

    /// The Body field's text.
    pub fn body_text(&self) -> String {
        self.body.text.to_string()
    }

    /// Test: starts an alert with the form's values, or says why it did not.
    pub fn test(&mut self) {
        let body = self.body.text.to_string();
        if body.trim().is_empty() {
            self.issue = Some(BLANK_BODY.to_owned());
            return;
        }
        let title = self.title.text.to_string();
        let sound = self.sound.text.to_string();
        let arguments = alert_arguments(&title, &body, &sound);
        self.issue = match (self.start)(&arguments) {
            Ok(()) => None,
            Err(error) => {
                let error = format!("{error:#}");
                warn!(error = %error, "alert tester: the alert could not start");
                Some(format!("Couldn't start the alert: {error}"))
            }
        };
    }
}

/// The tester's window first seen on screen changes nothing in the form; always false.
pub fn shown(_: &mut TesterState, _: &mut Tx) -> bool {
    false
}

/// Sets the appearance the window draws in (true: dark); true when it changed.
pub fn set_appearance(s: &mut TesterState, dark: bool) -> bool {
    s.theme.set(dark)
}

/// `mod+enter` presses Test; true when `key` was that chord. A focused text field takes
/// `mod+enter` for itself and passes nothing on, so each field runs this before its own keys.
pub fn window_key(s: &mut TesterState, key: &KeyEvent, tx: &mut Tx) -> bool {
    if !is_chord("mod+enter", key, tx) {
        return false;
    }
    s.test();
    true
}

/// Keys in the Body editor, before the editor sees them: `mod+enter` presses Test, and Return,
/// with or without Shift, types a line break.
fn body_key(s: &mut TesterState, key: &KeyEvent, tx: &mut Tx) -> bool {
    window_key(s, key, tx) || type_newline(&mut s.body, key)
}

/// The window's root.
pub fn view(s: &TesterState, cx: &mut Cx<TesterState>) -> El {
    let k = s.theme.kit(cx.system_dark());
    let mut form = col()
        .w_full()
        .h_full()
        .gap(SPACE_MD)
        .padding(SPACE_LG)
        .fill(surface(k, Surface::Window))
        .child(tester_field(
            k,
            TITLE_KEY,
            "Title (optional)",
            &s.title,
            |s| &mut s.title,
        ))
        .child(body_field(s, k))
        .child(tester_field(
            k,
            SOUND_KEY,
            "Sound file (optional)",
            &s.sound,
            |s| &mut s.sound,
        ));
    if let Some(issue) = &s.issue {
        form = form.child(status_line(
            k,
            ISSUE_KEY,
            Tone::Danger,
            StatusLines::Wrap,
            issue,
        ));
    }
    let test = Button::new("Test")
        .variant(ButtonVariant::Submit)
        .on_press(|s: &mut TesterState| s.test())
        .build(k)
        .key(TEST_KEY);
    rune_kit::root(
        form.child(row().w_full().child(spacer()).child(test)),
        Vec::new(),
    )
}

/// A single-line field keyed `key` under its label, taking the tester's shortcut while it has
/// focus.
fn tester_field(
    k: &Kit,
    key: &str,
    label: &str,
    edit: &TextEdit,
    access: fn(&mut TesterState) -> &mut TextEdit,
) -> El {
    let spec = LineFieldSpec {
        key,
        label,
        placeholder: "",
        issue: None,
    };
    line_field(
        k,
        spec,
        edit,
        access,
        window_key,
        |_: &mut TesterState, _: &mut Tx| {},
    )
}

/// The Body label over its editor. The editor carries the name, so the visible label stays
/// out of the accessibility tree.
fn body_field(s: &TesterState, k: &Kit) -> El {
    let spec = EditorSpec {
        key: BODY_KEY,
        label: "Body",
        placeholder: "The message the alert shows",
        description: None,
        invalid: false,
        height: EditorHeight::Fixed { lines: BODY_LINES },
        font: TYPE_BODY,
    };
    col()
        .w_full()
        .gap(SPACE_XS)
        .child(field_label(k, "Body").a11y_hidden())
        .child(text_editor(
            k,
            spec,
            &s.body,
            |s: &mut TesterState| &mut s.body,
            body_key,
            |s: &mut TesterState, _: &mut Tx| s.issue = None,
        ))
}

#[cfg(test)]
mod tests;

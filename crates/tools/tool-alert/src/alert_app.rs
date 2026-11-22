//! What a run of alert does: picks the mode, opens the window, plays the sound, and says how
//! the process exits.

use crate::design::{ALERT_SIZE, TESTER_MIN, TESTER_SIZE};
use crate::models::{AlertArgs, AlertSpec, Mode, TesterSeed};
use crate::tester::{self, TesterState, start_alert};
use crate::window::{self, AlertState};
use anyhow::{Context, Result, bail};
use common_audio::{Clip, Speaker};
use common_gui_rune::window::{GPU_BACKENDS, WatchHooks, spawn_window_watch};
use common_utils::string_utils::display_line;
use rune::{App, AppSpec, WindowOptions};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::thread;
use tracing::warn;

/// What a blank or missing body is refused with.
pub const BLANK_BODY: &str = "The alert's body is blank; pass the message as BODY";
/// What a sound on a network share is refused with on Windows.
pub const NETWORK_SHARE_REFUSED: &str = "a sound on a network share is not played";

/// What `args` asks for: the tester with `--gui`, the alert otherwise.
///
/// # Errors
/// When the alert's body is blank or missing.
pub fn mode(args: AlertArgs) -> Result<Mode> {
    if args.gui {
        return Ok(Mode::Tester(TesterSeed {
            title: args.title.unwrap_or_default(),
            body: args.body.unwrap_or_default(),
            sound: args.sound.unwrap_or_default(),
        }));
    }
    let body = args.body.unwrap_or_default();
    if body.trim().is_empty() {
        bail!(BLANK_BODY);
    }
    Ok(Mode::Show(AlertSpec {
        title: args.title.filter(|title| !title.trim().is_empty()),
        body,
        sound: args
            .sound
            .filter(|sound| !sound.trim().is_empty())
            .map(PathBuf::from),
    }))
}

/// Whether `path` starts with two slashes or backslashes in any mix, the start of a Windows
/// network-share path.
fn starts_like_a_network_share(path: &str) -> bool {
    let mut chars = path.chars();
    let is_separator = |c: Option<char>| matches!(c, Some('/' | '\\'));
    is_separator(chars.next()) && is_separator(chars.next())
}

/// Why the sound at `path` did not play, when it did not: `play`'s error, or on Windows the
/// refusal of a network-share path, which never reaches `play`. Opening such a path makes
/// Windows offer the user's sign-in to the host it names.
pub fn sound_issue(path: &Path, play: impl FnOnce(&Path) -> Result<()>) -> Option<String> {
    if cfg!(windows) && starts_like_a_network_share(&path.to_string_lossy()) {
        return Some(NETWORK_SHARE_REFUSED.to_owned());
    }
    play(path).err().map(|error| format!("{error:#}"))
}

/// Plays the sound file at `path` on the default output device and returns when it ends.
///
/// # Errors
/// When the file cannot be read, is not a playable sound, or no output device opens.
pub fn play_file(path: &Path) -> Result<()> {
    let clip = Clip::from_file(path)?;
    let mut speaker = Speaker::open()?;
    speaker.play(&clip)?;
    speaker.wait();
    Ok(())
}

/// Plays the sound at `path` with `play` on a thread of its own, then hands `post` what
/// [`sound_issue`] found: `None` once the sound played to its end.
pub fn start_sound(
    path: PathBuf,
    play: impl FnOnce(&Path) -> Result<()> + Send + 'static,
    post: impl Fn(Option<String>) + Send + 'static,
) {
    thread::spawn(move || {
        let issue = sound_issue(&path, play);
        if let Some(error) = &issue {
            warn!(
                path = ?path,
                error = %display_line(error.chars()),
                "alert: sound could not play"
            );
        }
        post(issue);
    });
}

/// The process exit code: 1 when the window could not be created, 0 otherwise. An alert the
/// user saw exits 0 whatever happened to its sound.
pub fn exit_code(window_failure: Option<&str>) -> i32 {
    match window_failure {
        Some(_) => 1,
        None => 0,
    }
}

/// What a window that could not be created needs, for the log.
pub fn gpu_hint() -> String {
    format!("alert needs a GPU that supports {GPU_BACKENDS}")
}

/// Runs alert: the tester with `--gui`, the alert otherwise. Returns when the window closes,
/// with the reason when the window could not be created.
///
/// # Errors
/// When the body is blank, or the window's event loop cannot start.
pub fn run(args: AlertArgs) -> Result<Option<String>> {
    match mode(args)? {
        Mode::Show(spec) => show(spec),
        Mode::Tester(seed) => open_tester(seed),
    }
}

/// Shows the alert above other windows at its fixed size. Rune reads no process arguments:
/// clap owns the command line.
fn show(spec: AlertSpec) -> Result<Option<String>> {
    let mut app_spec = AppSpec::new(
        window::OS_TITLE,
        ALERT_SIZE,
        AlertState::detached,
        window::view,
    );
    app_spec.min_size = Some(ALERT_SIZE);
    app_spec.on_key = Some(window::window_key);
    let app = App::new(app_spec);
    let watch = spawn_window_watch(
        app.handle(),
        WatchHooks {
            shown: window::shown,
            appearance: window::set_appearance,
        },
    );
    let poster = app.handle();
    let options = WindowOptions {
        always_on_top: true,
        resizable: false,
        min_size: Some(ALERT_SIZE),
        max_size: Some(ALERT_SIZE),
        ..WindowOptions::default()
    };
    let ran = app
        .args(Vec::new())
        .window_options(options)
        .on_close(window::on_close)
        .init(move || {
            let start_sound = spec.sound.clone().map(|path| {
                Box::new(move || {
                    start_sound(path, play_file, move |issue| {
                        poster.update_if(move |state, _| state.set_sound_issue(issue));
                    });
                }) as Box<dyn FnOnce()>
            });
            AlertState::new(&spec, start_sound)
        })
        .try_run();
    ran.map_err(|reason| anyhow::anyhow!(reason))
        .context("Could not start the alert window")?;
    Ok(watch.failure())
}

/// Opens the tester. Its Test button starts this same executable.
fn open_tester(seed: TesterSeed) -> Result<Option<String>> {
    let mut app_spec = AppSpec::new(
        tester::OS_TITLE,
        TESTER_SIZE,
        TesterState::detached,
        tester::view,
    );
    app_spec.min_size = Some(TESTER_MIN);
    app_spec.on_key = Some(tester::window_key);
    let app = App::new(app_spec);
    let watch = spawn_window_watch(
        app.handle(),
        WatchHooks {
            shown: tester::shown,
            appearance: tester::set_appearance,
        },
    );
    let ran = app
        .args(Vec::new())
        .init(move || TesterState::new(&seed, Box::new(start_this_program)))
        .try_run();
    ran.map_err(|reason| anyhow::anyhow!(reason))
        .context("Could not start the tester window")?;
    Ok(watch.failure())
}

/// Starts the running executable again with `arguments`.
fn start_this_program(arguments: &[OsString]) -> Result<()> {
    let executable = std::env::current_exe().context("Failed to find the running alert program")?;
    start_alert(&executable, arguments)
}

#[cfg(test)]
mod tests;

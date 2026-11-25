use anyhow::{Context, Result};
use common_audio::{Clip, Speaker};
use common_cli::tool_exit_helpers::{exit_error, exit_success};
use common_gui_rune::window::report_failure;
use speak::cli_utils::initialize;
use speak::config::locate_default_config;
use speak::gui;
use speak::models::SpeakArgs;
use speak::speak_app::{Deps, run};
use std::io::IsTerminal;
use std::path::PathBuf;
use tracing::error;

fn main() {
    let args = initialize();

    // `Ok(Some(reason))` is a window that could not be created; only `--gui` opens one.
    let result = config_path().and_then(|config_path| {
        if args.gui {
            gui::run(config_path)
        } else {
            speak(args, config_path).map(|()| None)
        }
    });
    match result {
        Ok(None) => exit_success(),
        Ok(Some(reason)) => {
            report_failure(env!("CARGO_PKG_NAME"), &reason);
            exit_error();
        }
        Err(error) => {
            error!(error = %format!("{error:#}"), "speak failed");
            exit_error();
        }
    }
}

/// The settings file to use: the one in speak's folder, after moving it there from the folder
/// speak used before 2.0.0 when only that one holds it, or the old one when it could not be
/// moved. The move, or why it did not happen, is logged.
fn config_path() -> Result<PathBuf> {
    let location = locate_default_config()?;
    location.report();
    Ok(location.path())
}

/// Runs the request on a current-thread runtime, so playback blocks the main thread until the
/// audio ends.
fn speak(args: SpeakArgs, config_path: PathBuf) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("Failed to start the async runtime")?;
    let stdin = std::io::stdin();
    let stderr = std::io::stderr();
    let deps = Deps {
        config_path,
        stdin_terminal: stdin.is_terminal(),
        stderr_terminal: stderr.is_terminal(),
        stdin: stdin.lock(),
        prompt: stderr,
        play: play_to_the_end,
    };
    runtime.block_on(run(args, deps))
}

/// Plays `clip` on the default output device and returns when it ends.
fn play_to_the_end(clip: Clip) -> Result<()> {
    let mut speaker = Speaker::open()?;
    speaker.play(&clip)?;
    speaker.wait();
    Ok(())
}

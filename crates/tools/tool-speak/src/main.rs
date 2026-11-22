use anyhow::{Context, Result};
use common_audio::{Clip, Speaker};
use common_cli::tool_exit_helpers::{exit_error, exit_success};
use common_gui_rune::window::gpu_hint;
use speak::cli_utils::initialize;
use speak::config::default_config_path;
use speak::gui;
use speak::models::SpeakArgs;
use speak::speak_app::{Deps, run};
use std::io::IsTerminal;
use tracing::error;

fn main() {
    let args = initialize();

    // `Ok(Some(reason))` is a window that could not be created; only `--gui` opens one.
    let result = if args.gui {
        gui::run(default_config_path())
    } else {
        speak(args).map(|()| None)
    };
    match result {
        Ok(None) => exit_success(),
        Ok(Some(reason)) => {
            error!(
                reason = %reason,
                hint = %gpu_hint(env!("CARGO_PKG_NAME")),
                "speak could not open a window"
            );
            exit_error();
        }
        Err(error) => {
            error!(error = %format!("{error:#}"), "speak failed");
            exit_error();
        }
    }
}

/// Runs the request on a current-thread runtime, so playback blocks the main thread until the
/// audio ends.
fn speak(args: SpeakArgs) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("Failed to start the async runtime")?;
    let stdin = std::io::stdin();
    let stderr = std::io::stderr();
    let deps = Deps {
        config_path: default_config_path(),
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

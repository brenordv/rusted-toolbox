//! One run of speak: find the text, find the server, ask it for audio, play it.

use crate::config::{self, SpeakConfig};
use crate::first_run::{self, FIRST_RUN_REFUSAL, Route};
use crate::models::SpeakArgs;
use crate::speech_client::{BYTES_PER_MB, SpeechClient};
use anyhow::{Context, Result, bail};
use common_audio::Clip;
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tracing::info;

/// The most text read from stdin.
pub const MAX_STDIN_TEXT_BYTES: usize = BYTES_PER_MB;

/// What a run reaches outside itself through.
pub struct Deps<R, W, P> {
    /// Where the settings file is.
    pub config_path: PathBuf,
    /// Whether stdin is a terminal.
    pub stdin_terminal: bool,
    /// Whether stderr is a terminal.
    pub stderr_terminal: bool,
    /// Stdin: the text when it is piped, the first run's answers when it is a terminal.
    pub stdin: R,
    /// Where the first run's questions are written.
    pub prompt: W,
    /// Plays the audio and returns when it ends.
    pub play: P,
}

/// Speaks `args.text`, or the text piped on stdin: loads the settings (asking for the server
/// on a first run in a terminal), sends the text to the speech server, and plays its answer.
///
/// # Errors
/// When there is no text or it is blank; the settings cannot be loaded, do not validate, or
/// do not exist and nobody can be asked; the server fails; its answer is not a playable
/// sound; or playback fails.
pub async fn run<R, W, P>(args: SpeakArgs, mut deps: Deps<R, W, P>) -> Result<()>
where
    R: BufRead,
    W: Write,
    P: FnOnce(Clip) -> Result<()>,
{
    let text = resolve_text(
        args.text,
        deps.stdin_terminal,
        &mut deps.stdin,
        MAX_STDIN_TEXT_BYTES,
    )?;
    if text.is_empty() {
        bail!("The text to speak is blank");
    }

    let mut config = match config::load(&deps.config_path).await? {
        Some(config) => {
            info!(path = %deps.config_path.display(), "config loaded");
            config
        }
        None => first_run_config(
            &deps.config_path,
            first_run::first_run_route(deps.stdin_terminal, deps.stderr_terminal),
            &mut deps.stdin,
            &mut deps.prompt,
        )?,
    };
    if let Some(model) = &args.model {
        config.set_model(model);
    }
    if let Some(voice) = &args.voice {
        config.set_voice(voice);
    }
    config.validate()?;

    let clip = speech_clip(&SpeechClient::new()?, &config, &text).await?;

    let started = Instant::now();
    (deps.play)(clip)?;
    info!(
        elapsed_ms = started.elapsed().as_millis() as u64,
        "playback ended"
    );
    Ok(())
}

/// Asks `config`'s server to say `text` and returns its answer as a sound.
///
/// # Errors
/// When the request fails (see [`SpeechClient::synthesize`]), or the answer is not a playable
/// sound; that error names the answer's content type and size.
pub async fn speech_clip(client: &SpeechClient, config: &SpeakConfig, text: &str) -> Result<Clip> {
    let speech = client.synthesize(config, text).await?;
    let size = speech.bytes.len();
    Clip::from_bytes(speech.bytes).with_context(|| {
        format!(
            "The speech server's answer could not be played (content type {}, {size} bytes)",
            speech.content_type.as_deref().unwrap_or("not given")
        )
    })
}

/// The text to speak, without surrounding whitespace: `argument` when given; otherwise all
/// of `reader` when stdin is piped, up to `cap` bytes.
///
/// # Errors
/// When there is no argument and stdin is a terminal, or the piped text is longer than `cap`
/// bytes, cannot be read, or is not UTF-8.
pub fn resolve_text(
    argument: Option<String>,
    stdin_terminal: bool,
    reader: &mut impl BufRead,
    cap: usize,
) -> Result<String> {
    if let Some(text) = argument {
        return Ok(text.trim().to_owned());
    }
    if stdin_terminal {
        bail!("There is no text to speak: pass it as an argument or pipe it on stdin");
    }
    let mut bytes = Vec::new();
    reader
        .take(cap as u64 + 1)
        .read_to_end(&mut bytes)
        .context("Failed to read the text from stdin")?;
    if bytes.len() > cap {
        bail!(
            "The text on stdin is longer than the {} limit",
            byte_limit(cap)
        );
    }
    let text = String::from_utf8(bytes).context("The text on stdin is not valid UTF-8")?;
    Ok(text.trim().to_owned())
}

fn byte_limit(cap: usize) -> String {
    if cap >= BYTES_PER_MB {
        format!("{} MB", cap / BYTES_PER_MB)
    } else {
        format!("{cap} byte")
    }
}

/// The settings of a run that found no settings file: asked for and saved on `Route::Ask`.
fn first_run_config(
    path: &Path,
    route: Route,
    stdin: &mut impl BufRead,
    prompt: &mut impl Write,
) -> Result<SpeakConfig> {
    match route {
        Route::Refuse => bail!(FIRST_RUN_REFUSAL),
        Route::Ask => {
            let config = first_run::ask(stdin, &mut *prompt)?;
            config.save(path)?;
            info!(path = %path.display(), "config saved");
            writeln!(prompt, "Saved the speech server to {}", path.display())
                .context("Failed to write where the settings were saved")?;
            Ok(config)
        }
    }
}

#[cfg(test)]
mod tests;

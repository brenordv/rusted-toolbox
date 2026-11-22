//! What the speak window asks of the outside world: the speech server, the settings file, and
//! the sound device. Each request is answered by exactly one [`Input`].

use super::state::{Effect, Input, SpeakState};
use crate::config::SpeakConfig;
use crate::speak_app::speech_clip;
use crate::speech_client::SpeechClient;
use anyhow::Result;
use common_audio::{Clip, Speaker};
use rune::AppHandle;
use std::future::Future;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use tokio::runtime::Handle;
use tokio::task::JoinError;
use tracing::{error, info, warn};

/// The steps a failure is logged under.
const STEP_GENERATE: &str = "generate";
const STEP_PLAY: &str = "play";
const STEP_SAVE: &str = "save";

/// What an effect is answered with when the window has no services.
const NOT_SET_UP: &str = "speak's window is not set up";
/// What Play is answered with when the audio thread is gone.
const AUDIO_STOPPED: &str = "The sound player stopped";

/// The work the window's effects ask for. Each call returns at once; its answer arrives later
/// as an [`Input`].
pub trait Services {
    /// Asks `config`'s server to say `text`; answers [`Input::Generated`].
    fn generate(&self, config: SpeakConfig, text: String);
    /// Plays `clip`, stopping one that still sounds; answers [`Input::PlayFailed`] only when
    /// it cannot.
    fn play(&self, clip: Clip);
    /// Writes `config` to `path`; answers [`Input::ConfigSaved`].
    fn save(&self, config: SpeakConfig, path: PathBuf);
}

/// Applies `input` to `s` and hands the effects it asks for to the window's services; true
/// when anything the view reads changed.
pub fn dispatch(s: &mut SpeakState, input: Input) -> bool {
    let outcome = s.apply(input);
    let mut changed = outcome.changed;
    for effect in outcome.effects {
        changed |= run_effect(s, effect);
    }
    changed
}

/// Hands `effect` to the services. A window with none answers the effect with a failure at
/// once, so the request it belongs to still gets its one answer; true when that changed the
/// view.
fn run_effect(s: &mut SpeakState, effect: Effect) -> bool {
    let Some(services) = s.services.clone() else {
        error!("the speak window's services are not set up");
        let answer = match effect {
            Effect::Generate { .. } => Input::Generated(Err(NOT_SET_UP.to_owned())),
            Effect::Play(_) => Input::PlayFailed(NOT_SET_UP.to_owned()),
            Effect::Save { .. } => Input::ConfigSaved(Err(NOT_SET_UP.to_owned())),
        };
        return s.apply(answer).changed;
    };
    match effect {
        Effect::Generate { config, text } => services.generate(config, text),
        Effect::Play(clip) => services.play(clip),
        Effect::Save { config, path } => services.save(config, path),
    }
    false
}

/// The whole text of `error`, logged once as a failure of `step`.
fn failure_text(error: &anyhow::Error, step: &'static str) -> String {
    let text = format!("{error:#}");
    warn!(error = %text, step, "speak: a request from the window failed");
    text
}

/// The answer to a generate request: the audio `config`'s server made of `text`, or why there
/// is none.
pub async fn generated(client: &SpeechClient, config: &SpeakConfig, text: &str) -> Input {
    match speech_clip(client, config, text).await {
        Ok(clip) => Input::Generated(Ok(clip)),
        Err(error) => Input::Generated(Err(failure_text(&error, STEP_GENERATE))),
    }
}

/// The answer to a save request: `config` once it is written to `path`, or why it was not.
pub async fn saved(config: SpeakConfig, path: PathBuf) -> Input {
    let written = tokio::task::spawn_blocking(move || match config.save(&path) {
        Ok(()) => {
            info!(path = %path.display(), "config saved");
            Input::ConfigSaved(Ok(config))
        }
        Err(error) => Input::ConfigSaved(Err(failure_text(&error, STEP_SAVE))),
    })
    .await;
    answer_of(written, STEP_SAVE, save_failed)
}

fn generate_failed(why: String) -> Input {
    Input::Generated(Err(why))
}

fn save_failed(why: String) -> Input {
    Input::ConfigSaved(Err(why))
}

/// The answer a task gave, or, when the task died before giving one, the failure `failed`
/// makes of that. Either way the request gets exactly one answer.
pub fn answer_of(
    joined: Result<Input, JoinError>,
    step: &'static str,
    failed: fn(String) -> Input,
) -> Input {
    match joined {
        Ok(answer) => answer,
        Err(error) => {
            let text = format!("The {step} task stopped unexpectedly: {error}");
            warn!(error = %text, step, "speak: a request from the window failed");
            failed(text)
        }
    }
}

/// Something that plays clips, one at a time.
pub trait Player {
    /// Starts `clip`, stopping the one that still sounds.
    ///
    /// # Errors
    /// When the clip cannot be played.
    fn play(&mut self, clip: &Clip) -> Result<()>;
}

impl Player for Speaker {
    fn play(&mut self, clip: &Clip) -> Result<()> {
        Speaker::play(self, clip)
    }
}

/// Starts the one thread that owns the sound device. It plays each clip sent to it, opening
/// the device with `open` for the first clip and keeping it; while `open` fails it is tried
/// again for the next clip. A clip that cannot be played is answered through `post` with
/// [`Input::PlayFailed`]. The thread ends when the sender is dropped.
pub fn spawn_audio<P: Player + 'static>(
    mut open: impl FnMut() -> Result<P> + Send + 'static,
    post: impl Fn(Input) + Send + 'static,
) -> (mpsc::Sender<Clip>, JoinHandle<()>) {
    let (sender, clips) = mpsc::channel::<Clip>();
    let thread = thread::spawn(move || {
        let mut player: Option<P> = None;
        for clip in clips {
            if let Err(error) = play_on(&mut player, &mut open, &clip) {
                post(Input::PlayFailed(failure_text(&error, STEP_PLAY)));
            }
        }
    });
    (sender, thread)
}

fn play_on<P: Player>(
    player: &mut Option<P>,
    open: &mut impl FnMut() -> Result<P>,
    clip: &Clip,
) -> Result<()> {
    let player = match player {
        Some(player) => player,
        None => player.insert(open()?),
    };
    player.play(clip)
}

/// Hands `clip` to the audio thread; the [`Input::PlayFailed`] to answer with when that thread
/// is gone.
fn send_clip(audio: &mpsc::Sender<Clip>, clip: Clip) -> Option<Input> {
    match audio.send(clip) {
        Ok(()) => None,
        Err(_) => {
            warn!(
                error = AUDIO_STOPPED,
                step = STEP_PLAY,
                "speak: a request from the window failed"
            );
            Some(Input::PlayFailed(AUDIO_STOPPED.to_owned()))
        }
    }
}

/// The services of a running window: requests run on the tokio runtime, sound on its own
/// thread, and every answer is applied on the UI thread through the app handle.
pub struct ProdServices {
    handle: AppHandle<SpeakState>,
    rt: Handle,
    client: SpeechClient,
    audio: mpsc::Sender<Clip>,
}

impl ProdServices {
    /// Services posting to `handle`, with the audio thread started on the default sound
    /// device.
    pub fn new(handle: AppHandle<SpeakState>, rt: Handle, client: SpeechClient) -> Self {
        let poster = handle.clone();
        let (audio, _thread) = spawn_audio(Speaker::open, move |input| deliver(&poster, input));
        Self {
            handle,
            rt,
            client,
            audio,
        }
    }

    /// Runs `work` on the runtime and applies its answer; a `work` that dies is answered with
    /// `failed`.
    fn post(
        &self,
        work: impl Future<Output = Input> + Send + 'static,
        step: &'static str,
        failed: fn(String) -> Input,
    ) {
        let handle = self.handle.clone();
        self.rt.spawn(async move {
            let answer = answer_of(tokio::spawn(work).await, step, failed);
            deliver(&handle, answer);
        });
    }
}

/// Applies `input` on the UI thread.
fn deliver(handle: &AppHandle<SpeakState>, input: Input) {
    handle.update_if(move |s, _| dispatch(s, input));
}

impl Services for ProdServices {
    fn generate(&self, config: SpeakConfig, text: String) {
        let client = self.client.clone();
        self.post(
            async move { generated(&client, &config, &text).await },
            STEP_GENERATE,
            generate_failed,
        );
    }

    fn play(&self, clip: Clip) {
        if let Some(failure) = send_clip(&self.audio, clip) {
            // Posted from the runtime: this call runs inside a handler on the UI thread.
            let handle = self.handle.clone();
            self.rt.spawn(async move { deliver(&handle, failure) });
        }
    }

    fn save(&self, config: SpeakConfig, path: PathBuf) {
        self.post(saved(config, path), STEP_SAVE, save_failed);
    }
}

#[cfg(test)]
mod tests;

//! Test support for the window: services that record what they were asked and do nothing.

use super::services::Services;
use super::state::SpeakState;
use crate::config::SpeakConfig;
use common_audio::Clip;
use common_audio::test_support::tone_wav;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

pub const SERVER_URL: &str = "http://my-host:10157/v1/audio/speech";

/// One call a window made to its services.
#[derive(Debug, Clone, PartialEq)]
pub enum Call {
    Generate {
        config: SpeakConfig,
        text: String,
    },
    /// The size of the clip asked to play.
    Play(usize),
    Save {
        config: SpeakConfig,
        path: PathBuf,
    },
}

/// Services that record each call.
#[derive(Default)]
pub struct Recording {
    calls: RefCell<Vec<Call>>,
}

impl Recording {
    /// Every call so far, oldest first.
    pub fn calls(&self) -> Vec<Call> {
        self.calls.borrow().clone()
    }
}

impl Services for Recording {
    fn generate(&self, config: SpeakConfig, text: String) {
        self.calls
            .borrow_mut()
            .push(Call::Generate { config, text });
    }

    fn play(&self, clip: Clip) {
        self.calls.borrow_mut().push(Call::Play(clip.size()));
    }

    fn save(&self, config: SpeakConfig, path: PathBuf) {
        self.calls.borrow_mut().push(Call::Save { config, path });
    }
}

/// The default settings pointed at [`SERVER_URL`].
pub fn config() -> SpeakConfig {
    SpeakConfig {
        url: SERVER_URL.to_owned(),
        ..SpeakConfig::default()
    }
}

/// Where the recording windows save.
pub fn config_path() -> PathBuf {
    PathBuf::from("settings").join("config.json")
}

/// A short valid clip.
pub fn clip() -> Clip {
    Clip::from_bytes(tone_wav()).expect("the test tone is a valid clip")
}

/// A window over `config` with recording services.
pub fn recording(config: Option<SpeakConfig>) -> (SpeakState, Rc<Recording>) {
    let services = Rc::new(Recording::default());
    let state = SpeakState::new(config, config_path(), Some(services.clone()));
    (state, services)
}

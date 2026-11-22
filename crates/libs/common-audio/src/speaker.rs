//! The default sound output device.

use crate::clip::Clip;
use anyhow::{Result, anyhow};
use rodio::{DeviceSinkBuilder, MixerDeviceSink, Player};

/// The failure text when the default output device cannot be opened.
pub const NO_OUTPUT_DEVICE: &str = "No sound output device could be opened";

/// The default sound output device, playing one clip at a time. Dropping it stops playback.
pub struct Speaker {
    sink: MixerDeviceSink,
    player: Option<Player>,
}

impl Speaker {
    /// Opens the default output device.
    ///
    /// # Errors
    /// [`NO_OUTPUT_DEVICE`], with the device's own reason, when there is no device or it
    /// cannot be opened.
    pub fn open() -> Result<Self> {
        let mut sink = DeviceSinkBuilder::open_default_sink()
            .map_err(|error| anyhow!(error).context(NO_OUTPUT_DEVICE))?;
        // rodio prints to stderr when a sink is dropped; dropping a `Speaker` is how its
        // playback is meant to end.
        sink.log_on_drop(false);
        Ok(Self { sink, player: None })
    }

    /// Starts playing `clip` from its beginning, stopping any sound still playing.
    ///
    /// # Errors
    /// When the clip can no longer be decoded.
    pub fn play(&mut self, clip: &Clip) -> Result<()> {
        self.player = None;
        let decoder = clip.decoder()?;
        let player = Player::connect_new(self.sink.mixer());
        player.append(decoder);
        self.player = Some(player);
        Ok(())
    }

    /// Blocks until the sound that is playing ends. Returns at once when nothing plays.
    pub fn wait(&self) {
        if let Some(player) = &self.player {
            player.sleep_until_end();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::tone_wav;

    #[test]
    #[ignore = "plays a tone on the real output device; CI runners have none"]
    fn the_default_device_plays_a_clip_to_its_end() {
        let clip = Clip::from_bytes(tone_wav()).unwrap();
        let mut speaker = Speaker::open().unwrap();

        speaker.play(&clip).unwrap();
        speaker.wait();
    }
}

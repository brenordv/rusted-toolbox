//! Decodes a sound and plays it on the default output device.
//!
//! [`Clip`] is sound data checked once against the decoders (WAV, MP3, FLAC, Ogg Vorbis);
//! [`Speaker`] is the default output device and plays one clip at a time. The crate logs
//! nothing: every failure is returned, worded for the person at the keyboard.

pub mod clip;
pub mod speaker;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

pub use clip::{Clip, MAX_SOUND_FILE_BYTES, NO_SOUND_DATA, NOT_A_PLAYABLE_SOUND};
pub use speaker::{NO_OUTPUT_DEVICE, Speaker};

//! Sound data that a decoder recognized.

use anyhow::{Context, Result, anyhow, bail};
use rodio::Decoder;
use std::fmt;
use std::fs::File;
use std::io::{Cursor, Read};
use std::path::Path;
use std::sync::Arc;

/// The largest sound file [`Clip::from_file`] reads.
pub const MAX_SOUND_FILE_BYTES: u64 = 64 * 1024 * 1024;
/// The failure text for empty sound data.
pub const NO_SOUND_DATA: &str = "There is no sound data";
/// The failure text for data no decoder recognizes.
pub const NOT_A_PLAYABLE_SOUND: &str =
    "This is not a sound this tool can play (WAV, MP3, FLAC, Ogg Vorbis)";

/// The decoder a [`Clip`] plays through.
pub(crate) type ClipDecoder = Decoder<Cursor<Arc<[u8]>>>;

/// Sound bytes a decoder recognized. Cloning shares the bytes.
#[derive(Clone)]
pub struct Clip {
    bytes: Arc<[u8]>,
}

impl fmt::Debug for Clip {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Clip")
            .field("bytes", &self.bytes.len())
            .finish()
    }
}

impl Clip {
    /// A clip over `bytes`.
    ///
    /// # Errors
    /// When `bytes` is empty ([`NO_SOUND_DATA`]) or no decoder recognizes it
    /// ([`NOT_A_PLAYABLE_SOUND`]).
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        if bytes.is_empty() {
            bail!(NO_SOUND_DATA);
        }
        let clip = Self {
            bytes: Arc::from(bytes),
        };
        clip.decoder()?;
        Ok(clip)
    }

    /// A clip read from the file at `path`.
    ///
    /// # Errors
    /// When the file cannot be read (the error names the path), is larger than
    /// [`MAX_SOUND_FILE_BYTES`], or is not a sound a decoder recognizes.
    pub fn from_file(path: &Path) -> Result<Self> {
        Self::from_file_capped(path, MAX_SOUND_FILE_BYTES)
    }

    /// [`Clip::from_file`] refusing a file larger than `cap` bytes.
    fn from_file_capped(path: &Path, cap: u64) -> Result<Self> {
        let file = File::open(path)
            .with_context(|| format!("Failed to open the sound file {}", path.display()))?;
        let mut bytes = Vec::new();
        file.take(cap.saturating_add(1))
            .read_to_end(&mut bytes)
            .with_context(|| format!("Failed to read the sound file {}", path.display()))?;
        if bytes.len() as u64 > cap {
            bail!(
                "The sound file {} is larger than the {} MB limit",
                path.display(),
                cap / (1024 * 1024)
            );
        }
        Self::from_bytes(bytes)
            .with_context(|| format!("Failed to load the sound file {}", path.display()))
    }

    /// The size of the sound data in bytes.
    pub fn size(&self) -> usize {
        self.bytes.len()
    }

    /// The sound data.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// A decoder at the start of the clip.
    pub(crate) fn decoder(&self) -> Result<ClipDecoder> {
        Decoder::new(Cursor::new(Arc::clone(&self.bytes)))
            .map_err(|error| anyhow!(error).context(NOT_A_PLAYABLE_SOUND))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::tone_wav;

    #[test]
    fn from_bytes_accepts_a_wav() {
        let wav = tone_wav();

        let clip = Clip::from_bytes(wav.clone()).unwrap();

        assert_eq!(clip.bytes(), wav.as_slice());
        assert_eq!(clip.size(), wav.len());
    }

    #[test]
    fn from_bytes_refuses_empty_data() {
        let error = Clip::from_bytes(Vec::new()).unwrap_err();

        assert_eq!(error.to_string(), NO_SOUND_DATA);
    }

    #[test]
    fn from_bytes_refuses_text_and_names_the_formats() {
        let error = Clip::from_bytes(b"this is plain text, not audio".to_vec()).unwrap_err();

        assert_eq!(error.to_string(), NOT_A_PLAYABLE_SOUND);
        assert!(error.to_string().contains("WAV, MP3, FLAC, Ogg Vorbis"));
    }

    #[test]
    fn from_file_reads_a_wav() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tone.wav");
        std::fs::write(&path, tone_wav()).unwrap();

        let clip = Clip::from_file(&path).unwrap();

        assert_eq!(clip.size(), tone_wav().len());
    }

    #[test]
    fn from_file_names_a_missing_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.wav");

        let error = Clip::from_file(&path).unwrap_err();

        assert!(format!("{error:#}").contains(&path.display().to_string()));
    }

    #[test]
    fn from_file_names_a_folder_given_in_place_of_a_file() {
        let dir = tempfile::tempdir().unwrap();

        let error = Clip::from_file(dir.path()).unwrap_err();

        assert!(format!("{error:#}").contains(&dir.path().display().to_string()));
    }

    #[test]
    fn from_file_refuses_a_file_over_the_cap() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tone.wav");
        let wav = tone_wav();
        std::fs::write(&path, &wav).unwrap();

        let error = Clip::from_file_capped(&path, wav.len() as u64 - 1).unwrap_err();

        assert!(error.to_string().contains("larger than"), "{error:#}");
    }

    #[test]
    fn from_file_accepts_a_file_exactly_at_the_cap() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tone.wav");
        let wav = tone_wav();
        std::fs::write(&path, &wav).unwrap();

        let clip = Clip::from_file_capped(&path, wav.len() as u64).unwrap();

        assert_eq!(clip.size(), wav.len());
    }

    #[test]
    fn from_file_names_the_path_of_a_file_that_is_not_a_sound() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.wav");
        std::fs::write(&path, "plain text").unwrap();

        let error = Clip::from_file(&path).unwrap_err();

        let text = format!("{error:#}");
        assert!(text.contains(&path.display().to_string()), "{text}");
        assert!(text.contains(NOT_A_PLAYABLE_SOUND), "{text}");
    }

    #[test]
    fn debug_shows_the_size_and_not_the_bytes() {
        let clip = Clip::from_bytes(tone_wav()).unwrap();

        assert_eq!(
            format!("{clip:?}"),
            format!("Clip {{ bytes: {} }}", clip.size())
        );
    }
}

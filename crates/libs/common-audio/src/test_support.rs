//! Test support (`cfg(test)` or the dev-only `test-support` feature): sound data built in
//! memory, so no binary fixture is committed.

const SAMPLE_RATE: u32 = 8_000;
const SAMPLES: u32 = 400;
/// Samples per half period: a 400 Hz square wave at [`SAMPLE_RATE`].
const HALF_PERIOD: u32 = 10;
const AMPLITUDE: i16 = 6_000;

/// A valid WAV file: 50 ms of a quiet 400 Hz square wave, 16-bit mono at 8 kHz.
pub fn tone_wav() -> Vec<u8> {
    let data_bytes = SAMPLES * 2;
    let mut wav = Vec::with_capacity(44 + data_bytes as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_bytes.to_le_bytes());
    for sample in 0..SAMPLES {
        let value = if (sample / HALF_PERIOD).is_multiple_of(2) {
            AMPLITUDE
        } else {
            -AMPLITUDE
        };
        wav.extend_from_slice(&value.to_le_bytes());
    }
    wav
}

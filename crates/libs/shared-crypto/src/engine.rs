//! Streaming encrypt/decrypt over the age format. Both entry points move
//! bytes `Read` to `Write` in constant memory; the trait-object adaptation
//! age's constructors need stays inside this module, so consumers only ever
//! handle the concrete x25519 types.

use crate::errors::{
    CTX_IO_ENCRYPT, MSG_PASSPHRASE_REFUSED, map_copy_error, map_decrypt_error, map_encrypt_error,
};
use age::armor::{ArmoredReader, ArmoredWriter, Format};
use age::x25519::{Identity, Recipient};
use anyhow::{Context, bail};
use std::io;

/// Encrypts `input` for `recipients`, streaming into `output`. `armor`
/// selects ASCII armor; otherwise the output is the binary age format.
/// Returns the number of plaintext bytes read from `input` (for the
/// ciphertext size, stat the output). An empty recipient set is an error.
///
/// The age header reaches `output` before the first payload byte, so a
/// failed call can leave a partial file behind; callers writing to a path
/// should write to a temp file and rename.
pub fn encrypt(
    recipients: &[Recipient],
    input: &mut impl io::Read,
    output: &mut impl io::Write,
    armor: bool,
) -> anyhow::Result<u64> {
    let format = if armor {
        Format::AsciiArmor
    } else {
        Format::Binary
    };

    let encryptor = age::Encryptor::with_recipients(
        recipients
            .iter()
            .map(|recipient| recipient as &dyn age::Recipient),
    )
    .map_err(map_encrypt_error)?;

    // The armor writer wraps the output in both modes: Format::Binary is a
    // pass-through, so one code path serves --armor and binary.
    let armored = ArmoredWriter::wrap_output(&mut *output, format).context(CTX_IO_ENCRYPT)?;

    let mut writer = encryptor.wrap_output(armored).context(CTX_IO_ENCRYPT)?;

    let bytes = io::copy(input, &mut writer).context(CTX_IO_ENCRYPT)?;

    // Both finishes are mandatory, in this order: the stream writer flushes
    // the final short chunk, the armor writer closes the armor framing (a
    // no-op in binary mode). Skipping either truncates the file.
    writer
        .finish()
        .and_then(|armored| armored.finish())
        .context(CTX_IO_ENCRYPT)?;

    Ok(bytes)
}

/// Decrypts age data from `input` with `identities`, streaming the plaintext
/// into `output`; armored and binary inputs are auto-detected. Returns the
/// number of plaintext bytes written. Passphrase-encrypted (scrypt) files
/// are refused before any key or payload work.
///
/// Any `Err` poisons the output: plaintext may already have streamed out
/// before a corruption was detected, so the caller must discard whatever the
/// output received. Failure wording comes from [`crate::errors`].
pub fn decrypt(
    identities: &[Identity],
    input: &mut impl io::Read,
    output: &mut impl io::Write,
) -> anyhow::Result<u64> {
    let decryptor =
        age::Decryptor::new(ArmoredReader::new(&mut *input)).map_err(map_decrypt_error)?;

    if decryptor.is_scrypt() {
        bail!(MSG_PASSPHRASE_REFUSED);
    }

    let mut reader = decryptor
        .decrypt(identities.iter().map(|i| i as &dyn age::Identity))
        .map_err(map_decrypt_error)?;

    let bytes = io::copy(&mut reader, output).map_err(map_copy_error)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::{
        MSG_CORRUPTED, MSG_NO_MATCHING_IDENTITY, MSG_NO_RECIPIENTS, MSG_NOT_AGE_DATA,
        MSG_PASSPHRASE_REFUSED,
    };
    use age::secrecy::SecretString;
    use rstest::rstest;
    use std::io::Write;

    const ARMOR_BEGIN: &str = "-----BEGIN AGE ENCRYPTED FILE-----";
    const CHUNK: usize = 64 * 1024;

    fn recipients_of(identities: &[&Identity]) -> Vec<Recipient> {
        identities.iter().map(|i| i.to_public()).collect()
    }

    /// Deterministic pseudo-random payload (xorshift64) so a chunk-boundary
    /// failure reproduces byte-for-byte.
    fn payload(len: usize) -> Vec<u8> {
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut out = Vec::with_capacity(len + 8);
        while out.len() < len {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            out.extend_from_slice(&state.to_le_bytes());
        }
        out.truncate(len);
        out
    }

    fn encrypt_vec(recipients: &[Recipient], plaintext: &[u8], armor: bool) -> (Vec<u8>, u64) {
        let mut ciphertext = Vec::new();
        let mut input = plaintext;
        let bytes = encrypt(recipients, &mut input, &mut ciphertext, armor).unwrap();
        (ciphertext, bytes)
    }

    fn decrypt_vec(identities: &[Identity], ciphertext: &[u8]) -> anyhow::Result<(Vec<u8>, u64)> {
        let mut plaintext = Vec::new();
        let mut input = ciphertext;
        let bytes = decrypt(identities, &mut input, &mut plaintext)?;
        Ok((plaintext, bytes))
    }

    /// Asserts the rendered chain carries the expected wording and that
    /// neither rendering leaks a secret key (the redaction invariant).
    fn assert_failure_wording(err: &anyhow::Error, expected: &str) {
        let display = format!("{err:#}");
        let debug = format!("{err:?}");
        assert!(display.contains(expected), "chain was: {display}");
        assert!(!display.contains("AGE-SECRET-KEY"));
        assert!(!debug.contains("AGE-SECRET-KEY"));
    }

    #[rstest]
    #[case::empty(0)]
    #[case::one_byte(1)]
    #[case::exactly_one_chunk(CHUNK)]
    #[case::chunk_boundary_plus_one(CHUNK + 1)]
    #[case::multi_chunk(3 * 1024 * 1024 + 17)]
    fn round_trip_restores_the_plaintext_and_reports_its_length(#[case] len: usize) {
        let id = Identity::generate();
        let plaintext = payload(len);

        let (ciphertext, reported_in) = encrypt_vec(&recipients_of(&[&id]), &plaintext, false);
        let (decrypted, reported_out) = decrypt_vec(&[id], &ciphertext).unwrap();

        assert_eq!(decrypted, plaintext);
        assert_eq!(reported_in, len as u64);
        assert_eq!(reported_out, len as u64);
    }

    #[test]
    fn armored_output_round_trips_and_survives_a_crlf_rewrite() {
        let id = Identity::generate();
        let plaintext = payload(CHUNK + 100);

        let (armored, _) = encrypt_vec(&recipients_of(&[&id]), &plaintext, true);
        let text = String::from_utf8(armored).unwrap();
        assert!(text.starts_with(ARMOR_BEGIN));

        let (decrypted, _) = decrypt_vec(std::slice::from_ref(&id), text.as_bytes()).unwrap();
        assert_eq!(decrypted, plaintext);

        // Both line-ending shapes must decrypt whatever this build's armor
        // writer emitted (LF on Unix, CRLF on Windows): LF is the age
        // normal form, CRLF the Windows-clipboard rewrite.
        let lf = text.replace("\r\n", "\n");
        let (decrypted, _) = decrypt_vec(std::slice::from_ref(&id), lf.as_bytes()).unwrap();
        assert_eq!(decrypted, plaintext);

        let crlf = lf.replace('\n', "\r\n");
        let (decrypted, _) = decrypt_vec(&[id], crlf.as_bytes()).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn every_listed_recipient_can_decrypt_and_an_unlisted_one_cannot() {
        let a = Identity::generate();
        let b = Identity::generate();
        let c = Identity::generate();
        let plaintext = b"sealed for two people";

        let (ciphertext, _) = encrypt_vec(&recipients_of(&[&a, &b]), plaintext, false);

        for id in [a, b] {
            let (decrypted, _) = decrypt_vec(&[id], &ciphertext).unwrap();
            assert_eq!(decrypted, plaintext);
        }

        let err = decrypt_vec(&[c], &ciphertext).unwrap_err();
        assert_failure_wording(&err, MSG_NO_MATCHING_IDENTITY);
    }

    #[test]
    fn passphrase_files_are_refused_before_any_payload_reaches_the_output() {
        let mut scrypt =
            age::scrypt::Recipient::new(SecretString::from("sixteen pelicans".to_owned()));
        // The default work factor targets ~1s; 2 keeps the fixture fast.
        scrypt.set_work_factor(2);

        let encryptor =
            age::Encryptor::with_recipients(std::iter::once(&scrypt as &dyn age::Recipient))
                .unwrap();
        let mut ciphertext = Vec::new();
        let mut writer = encryptor.wrap_output(&mut ciphertext).unwrap();
        writer.write_all(b"locked behind a passphrase").unwrap();
        writer.finish().unwrap();

        let mut output = Vec::new();
        let mut input = ciphertext.as_slice();
        let err = decrypt(&[Identity::generate()], &mut input, &mut output).unwrap_err();

        assert_failure_wording(&err, MSG_PASSPHRASE_REFUSED);
        assert!(output.is_empty(), "refusal must precede any payload work");
    }

    #[test]
    fn a_flipped_ciphertext_byte_reports_corruption() {
        let id = Identity::generate();
        let (mut ciphertext, _) = encrypt_vec(&recipients_of(&[&id]), &payload(200), false);

        let last = ciphertext.len() - 1;
        ciphertext[last] ^= 0x01;

        let err = decrypt_vec(&[id], &ciphertext).unwrap_err();
        assert_failure_wording(&err, MSG_CORRUPTED);
    }

    #[test]
    fn a_truncated_stream_reports_corruption() {
        let id = Identity::generate();
        let (mut ciphertext, _) = encrypt_vec(&recipients_of(&[&id]), &payload(200), false);

        ciphertext.truncate(ciphertext.len() - 8);

        let err = decrypt_vec(&[id], &ciphertext).unwrap_err();
        assert_failure_wording(&err, MSG_CORRUPTED);
    }

    #[rstest]
    #[case::shorter_than_the_armor_sniff(b"tiny".as_slice())]
    #[case::long_enough_to_parse_as_a_header(
        b"garbage that is comfortably longer than the 36-byte armor sniff window".as_slice()
    )]
    fn garbage_input_is_reported_as_not_age_data(#[case] garbage: &[u8]) {
        let err = decrypt_vec(&[Identity::generate()], garbage).unwrap_err();
        assert_failure_wording(&err, MSG_NOT_AGE_DATA);
    }

    #[test]
    fn an_empty_recipient_set_gets_the_no_recipients_guidance() {
        let mut input: &[u8] = b"anything";
        let mut output = Vec::new();

        let err = encrypt(&[], &mut input, &mut output, false).unwrap_err();

        assert_failure_wording(&err, MSG_NO_RECIPIENTS);
    }
}

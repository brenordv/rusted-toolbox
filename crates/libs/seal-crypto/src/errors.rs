//! The one home for failure wording: public message constants and builders
//! that the CLI, the GUI, and the tests all quote, plus the `pub(crate)`
//! helpers the engine routes raw `age` errors through so none of age's own
//! terse messages reaches a user unwrapped.
//!
//! By owner decision this crate exposes no error enum. Internal workspace
//! libs follow the repo convention of `anyhow::Result` + `.context()`;
//! consumers react to the wording (and their own call-site checks), never to
//! error types.

use std::io;
use std::path::Path;

use anyhow::anyhow;

pub const MSG_NO_RECIPIENTS: &str =
    "no recipients specified; pass -r <age1...> or -R <recipients-file>";

pub const MSG_NOT_AGE_DATA: &str = "not an age file (or corrupted at the start)";

pub const MSG_NO_MATCHING_IDENTITY: &str = "none of the provided identities can open this file";

pub const MSG_PASSPHRASE_REFUSED: &str =
    "this file is passphrase-encrypted; seal only supports key-based files";

pub const MSG_CORRUPTED: &str =
    "file is damaged or was tampered with; the output is incomplete and must be discarded";

pub const MSG_RECIPIENTS_REJECTED: &str = "the recipient set was rejected";

/// Context wording for I/O failures on the encrypt path; shared by the
/// engine's stream stages and [`map_encrypt_error`]'s `Io` arm.
pub(crate) const CTX_IO_ENCRYPT: &str = "I/O failure while encrypting";

/// Context wording for I/O failures on the decrypt path; shared by
/// [`map_decrypt_error`]'s `Io` arm and [`map_copy_error`].
pub(crate) const CTX_IO_DECRYPT: &str = "I/O failure while decrypting";

/// The input path yields no default output name: either it has no file name
/// at all, or a decrypt input does not end in `.age` (or is nothing but
/// `.age`). The caller must ask for an explicit output path.
pub fn no_default_output_name(path: &Path) -> String {
    format!(
        "cannot derive an output name from {}; pass -o <output> to choose one",
        path.display()
    )
}

pub fn invalid_recipient_literal(index: usize) -> String {
    format!(
        "recipient argument {} is not a valid age public key (expected an age1... value)",
        index + 1
    )
}

pub fn invalid_recipient_in_file(path: &Path, line: usize) -> String {
    format!(
        "{}: line {}: not a valid age public key (expected an age1... value)",
        path.display(),
        line
    )
}

pub fn recipient_file_unreadable(path: &Path) -> String {
    format!("could not read recipients file {}", path.display())
}

pub fn identity_exists(path: &Path) -> String {
    format!(
        "identity file {} already exists; pass --force to overwrite it",
        path.display()
    )
}

pub fn identity_file_unreadable(path: &Path) -> String {
    format!("could not read identity file {}", path.display())
}

/// Reports only the file and 1-based line number, never the line's content: a
/// malformed identity line could be a mistyped secret key.
pub fn identity_file_malformed(path: &Path, line: usize) -> String {
    format!(
        "{}: line {}: not a valid age identity",
        path.display(),
        line
    )
}

/// Maps an [`age::EncryptError`] from `Encryptor::with_recipients` onto the
/// shared wording. With an all-x25519 recipient set only `MissingRecipients`
/// and `Io` can occur; the remaining arms (mixed or incompatible recipient
/// labels need scrypt or plugin recipients) are mapped defensively, with
/// age's own message kept in the chain.
pub(crate) fn map_encrypt_error(err: age::EncryptError) -> anyhow::Error {
    use age::EncryptError as E;
    match err {
        E::MissingRecipients => anyhow!(MSG_NO_RECIPIENTS),
        E::Io(e) => anyhow::Error::new(e).context(CTX_IO_ENCRYPT),
        other => anyhow::Error::msg(other.to_string()).context(MSG_RECIPIENTS_REJECTED),
    }
}

/// Maps an [`age::DecryptError`] from `Decryptor::new` or `Decryptor::decrypt`
/// onto the shared wording. Payload-stage corruption never arrives here: the
/// stream reader reports it as `io::Error` during the plaintext copy, which
/// goes through [`map_copy_error`] instead.
pub(crate) fn map_decrypt_error(err: age::DecryptError) -> anyhow::Error {
    use age::DecryptError as E;
    match err {
        E::InvalidHeader | E::UnknownFormat => anyhow!(MSG_NOT_AGE_DATA),
        E::InvalidMac | E::DecryptionFailed => anyhow!(MSG_CORRUPTED),
        E::NoMatchingKeys | E::KeyDecryptionFailed => anyhow!(MSG_NO_MATCHING_IDENTITY),
        // Only scrypt files produce ExcessiveWork, and the is_scrypt gate in
        // the engine fires first; same refusal either way.
        E::ExcessiveWork { .. } => anyhow!(MSG_PASSPHRASE_REFUSED),
        // An input that ends before the header does (the armor sniff alone
        // needs 36 bytes) surfaces as an EOF `Io`, not as `InvalidHeader`;
        // to the user that is not-age-data, never an I/O failure.
        E::Io(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
            anyhow::Error::new(e).context(MSG_NOT_AGE_DATA)
        }
        E::Io(e) => anyhow::Error::new(e).context(CTX_IO_DECRYPT),
        // The enum is #[non_exhaustive]; this arm also absorbs the plugin
        // variants that only exist with the (unused) `plugin` feature.
        _ => anyhow!(MSG_NOT_AGE_DATA),
    }
}

/// Classifies an `io::Error` from decrypt's plaintext copy stage. The age
/// stream reader reports an AEAD failure as `InvalidData` and a truncated
/// stream as `UnexpectedEof`; any other kind is a genuine I/O failure and
/// must not be reported as tampering.
pub(crate) fn map_copy_error(err: io::Error) -> anyhow::Error {
    match err.kind() {
        io::ErrorKind::InvalidData | io::ErrorKind::UnexpectedEof => {
            anyhow::Error::new(err).context(MSG_CORRUPTED)
        }
        _ => anyhow::Error::new(err).context(CTX_IO_DECRYPT),
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use std::io::{Error as IoError, ErrorKind};

    use super::*;

    fn rendered(err: anyhow::Error) -> String {
        format!("{err:#}")
    }

    #[rstest]
    #[case::invalid_header(age::DecryptError::InvalidHeader, MSG_NOT_AGE_DATA)]
    #[case::unknown_format(age::DecryptError::UnknownFormat, MSG_NOT_AGE_DATA)]
    #[case::invalid_mac(age::DecryptError::InvalidMac, MSG_CORRUPTED)]
    #[case::decryption_failed(age::DecryptError::DecryptionFailed, MSG_CORRUPTED)]
    #[case::no_matching_keys(age::DecryptError::NoMatchingKeys, MSG_NO_MATCHING_IDENTITY)]
    #[case::key_decryption_failed(age::DecryptError::KeyDecryptionFailed, MSG_NO_MATCHING_IDENTITY)]
    #[case::excessive_work(
        age::DecryptError::ExcessiveWork { required: 22, target: 18 },
        MSG_PASSPHRASE_REFUSED
    )]
    #[case::input_shorter_than_a_header(
        age::DecryptError::Io(IoError::new(
            ErrorKind::UnexpectedEof,
            "failed to fill whole buffer"
        )),
        MSG_NOT_AGE_DATA
    )]
    fn decrypt_error_maps_to_shared_wording(
        #[case] err: age::DecryptError,
        #[case] expected: &str,
    ) {
        assert!(rendered(map_decrypt_error(err)).contains(expected));
    }

    #[test]
    fn decrypt_io_error_keeps_the_os_error_and_avoids_tamper_wording() {
        let err = age::DecryptError::Io(IoError::new(ErrorKind::PermissionDenied, "denied"));
        let chain = rendered(map_decrypt_error(err));
        assert!(chain.contains("I/O failure while decrypting"));
        assert!(chain.contains("denied"));
        assert!(!chain.contains(MSG_CORRUPTED));
    }

    #[test]
    fn encrypt_missing_recipients_maps_to_the_no_recipients_guidance() {
        let chain = rendered(map_encrypt_error(age::EncryptError::MissingRecipients));
        assert!(chain.contains(MSG_NO_RECIPIENTS));
    }

    #[test]
    fn encrypt_io_error_keeps_the_os_error() {
        let err = age::EncryptError::Io(IoError::new(ErrorKind::StorageFull, "disk full"));
        let chain = rendered(map_encrypt_error(err));
        assert!(chain.contains("I/O failure while encrypting"));
        assert!(chain.contains("disk full"));
    }

    #[test]
    fn encrypt_unexpected_variant_carries_age_text_under_the_rejection_wording() {
        let err = age::EncryptError::MixedRecipientAndPassphrase;
        let chain = rendered(map_encrypt_error(err));
        assert!(chain.starts_with(MSG_RECIPIENTS_REJECTED));
        assert!(chain.len() > MSG_RECIPIENTS_REJECTED.len());
    }

    #[rstest]
    #[case::aead_failure(ErrorKind::InvalidData)]
    #[case::truncation(ErrorKind::UnexpectedEof)]
    fn corruption_shaped_copy_error_gets_the_corrupted_wording(#[case] kind: ErrorKind) {
        let chain = rendered(map_copy_error(IoError::new(kind, "decryption error")));
        assert!(chain.contains(MSG_CORRUPTED));
        assert!(chain.contains("decryption error"));
    }

    #[test]
    fn other_copy_errors_stay_plain_io() {
        let chain = rendered(map_copy_error(IoError::new(
            ErrorKind::BrokenPipe,
            "pipe closed",
        )));
        assert!(chain.contains("I/O failure while decrypting"));
        assert!(chain.contains("pipe closed"));
        assert!(!chain.contains(MSG_CORRUPTED));
    }
}

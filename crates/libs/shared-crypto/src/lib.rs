//! The encryption engine behind the `seal` CLI and GUI. Wraps the `age`
//! crate (X25519 key agreement, ChaCha20-Poly1305 in 64 KiB authenticated
//! chunks, the published age format) behind a small API: identity generation
//! and files, recipient parsing, and streaming encrypt/decrypt. Key-based
//! only; passphrase-encrypted age files are detected and refused. No CLI
//! parsing, no UI, no printing.
//!
//! # Security posture
//!
//! Protected: confidentiality and integrity of file contents against anyone
//! holding the ciphertext without a matching identity. Every chunk is
//! authenticated, so truncation and tampering surface as decryption errors.
//!
//! Not protected: sender authenticity (age does not sign; decryption proves
//! the file was sealed to your key and arrived intact, not who sent it),
//! file names and sizes, plaintext the caller leaves on disk, or a
//! compromised machine that can read identity files. Secret keys only exist
//! as [`SecretString`]; nothing here logs, prints, or serializes one.
//!
//! # Poisoned output
//!
//! [`engine::decrypt`] streams, so any `Err` means plaintext bytes may
//! already have reached the output; consumers discard the output on any
//! failure. The engine cannot un-write a stream.

use std::path::{Path, PathBuf};

use anyhow::bail;

pub mod engine;
pub mod errors;
pub mod keys;

// Load-bearing re-exports: phases 2-5 keep their "zero `age::` imports" rule
// only through these names.
pub use age::secrecy::SecretString;
pub use age::x25519::{Identity, Recipient};

/// Which way [`output_name_for`] maps a file name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Encrypt,
    Decrypt,
}

/// Maps an input path to its default output path, the one rule the CLI and
/// the GUI both apply. Encrypt appends `.age` to the whole file name
/// (`x.tar.gz` becomes `x.tar.gz.age`, never `x.tar.age`); decrypt strips
/// exactly one trailing `.age`. Fails, asking for an explicit output path,
/// when the input has no file name or a decrypt input does not end in `.age`
/// (or is nothing but `.age`).
pub fn output_name_for(input: &Path, direction: Direction) -> anyhow::Result<PathBuf> {
    let Some(name) = input.file_name() else {
        bail!(errors::no_default_output_name(input));
    };

    match direction {
        Direction::Encrypt => {
            let mut output = name.to_os_string();
            output.push(".age");
            Ok(input.with_file_name(output))
        }
        Direction::Decrypt => match name.to_str().and_then(|n| n.strip_suffix(".age")) {
            Some(stem) if !stem.is_empty() => Ok(input.with_file_name(stem)),
            _ => bail!(errors::no_default_output_name(input)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::plain("file.txt", "file.txt.age")]
    #[case::multi_extension("x.tar.gz", "x.tar.gz.age")]
    #[case::already_age("file.age", "file.age.age")]
    #[case::with_parent("dir/report.pdf", "dir/report.pdf.age")]
    fn encrypt_appends_age_to_the_whole_file_name(#[case] input: &str, #[case] expected: &str) {
        let output = output_name_for(Path::new(input), Direction::Encrypt).unwrap();
        assert_eq!(output, Path::new(expected));
    }

    #[rstest]
    #[case::plain("file.txt.age", "file.txt")]
    #[case::strips_exactly_one("archive.age.age", "archive.age")]
    #[case::with_parent("dir/file.txt.age", "dir/file.txt")]
    fn decrypt_strips_one_age_suffix(#[case] input: &str, #[case] expected: &str) {
        let output = output_name_for(Path::new(input), Direction::Decrypt).unwrap();
        assert_eq!(output, Path::new(expected));
    }

    #[rstest]
    #[case::no_age_suffix("file.enc")]
    #[case::nothing_but_the_suffix(".age")]
    fn decrypt_without_a_strippable_suffix_asks_for_an_explicit_output(#[case] input: &str) {
        let err = output_name_for(Path::new(input), Direction::Decrypt).unwrap_err();
        assert!(format!("{err:#}").contains("-o <output>"));
    }

    #[test]
    fn a_path_without_a_file_name_asks_for_an_explicit_output() {
        let err = output_name_for(Path::new(".."), Direction::Encrypt).unwrap_err();
        assert!(format!("{err:#}").contains("-o <output>"));
    }
}

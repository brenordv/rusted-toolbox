//! Content-based binary detection for tools that skip non-text files.
//!
//! This is a fail-open heuristic: a file that cannot be opened or read
//! reports as text, and NUL-free content that is not human-readable (ANSI
//! escape sequences, single-byte encodings) also reports as text. It must
//! not be used as a security or sanitization control.

use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Number of leading bytes sampled when sniffing a file for binary content.
const BINARY_SNIFF_LEN: u64 = 8 * 1024;

/// Reads up to the first 8 KiB of the file and reports whether it looks like
/// binary content: any NUL byte in the sample marks the file as binary. Files
/// that cannot be opened or read are reported as not binary so the caller's
/// own open/read error handling stays in charge of them.
pub fn is_probably_binary(path: &Path) -> bool {
    let Ok(file) = File::open(path) else {
        return false;
    };
    let mut sample = Vec::with_capacity(BINARY_SNIFF_LEN as usize);
    if file
        .take(BINARY_SNIFF_LEN)
        .read_to_end(&mut sample)
        .is_err()
    {
        return false;
    }
    sample.contains(&0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn is_probably_binary_false_for_text_file() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("plain.txt");
        fs::write(&file, "just some text\nwith two lines\n").unwrap();

        assert!(!is_probably_binary(&file));
    }

    #[test]
    fn is_probably_binary_true_for_nul_byte_file() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("data.bin");
        fs::write(&file, b"text before\x00text after").unwrap();

        assert!(is_probably_binary(&file));
    }

    #[test]
    fn is_probably_binary_false_for_empty_file() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("empty.txt");
        fs::write(&file, "").unwrap();

        assert!(!is_probably_binary(&file));
    }

    #[test]
    fn is_probably_binary_false_when_nul_is_past_the_sample() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("late-nul.txt");
        let mut content = vec![b'a'; 8 * 1024];
        content.push(0);
        fs::write(&file, content).unwrap();

        assert!(!is_probably_binary(&file));
    }

    #[test]
    fn is_probably_binary_false_for_nonexistent_path() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("does-not-exist.bin");

        assert!(!is_probably_binary(&missing));
    }
}

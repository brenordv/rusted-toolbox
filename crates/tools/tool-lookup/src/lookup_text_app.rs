use crate::lookup_shared::{list_files, normalize_extensions, path_matches_allowed};
use crate::models::TextLookupConfig;
use anyhow::{anyhow, Result};
use common_cli::broken_pipe::write_out;
use common_file_utils::binary_sniff::is_probably_binary;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::time::Instant;
use tracing::{debug, error};

/// Counters accumulated by [`run_text_lookup`]; they feed the summary line and
/// let tests assert the outcome of a search.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct TextLookupCounts {
    pub files_read: u64,
    pub files_skipped_binary: u64,
    pub lines_scanned: u64,
    pub lines_skipped_invalid_utf8: u64,
    pub matches: u64,
}

/// Scans the search path for the configured text, writing each matching line
/// to `output`, and returns the accumulated counters.
///
/// # Errors
/// Fails when the search path does not exist, with the
/// [`common_cli::broken_pipe::BrokenPipe`] marker when the consumer closes
/// the pipe, and with the underlying I/O error for any other write failure.
pub fn run_text_lookup(
    config: &TextLookupConfig,
    output: &mut impl Write,
) -> Result<TextLookupCounts> {
    let start = Instant::now();

    let base_path = PathBuf::from(&config.path);
    if !base_path.exists() {
        return Err(anyhow!("Path does not exist: {}", base_path.display()));
    }

    let normalized_extensions = normalize_extensions(&config.file_extensions);
    let needle = config.text.to_ascii_lowercase();

    let files_iter = list_files(&base_path, config.current_only)?;
    let mut counts = TextLookupCounts::default();

    for file_path in files_iter {
        if !path_matches_allowed(&file_path, &normalized_extensions) {
            continue;
        }

        if is_probably_binary(&file_path) {
            counts.files_skipped_binary += 1;
            debug!(file = %file_path.display(), "skipping binary file");
            continue;
        }

        let file = match File::open(&file_path) {
            Ok(f) => f,
            Err(e) => {
                error!("Failed to open file '{}': {}", file_path.display(), e);
                continue;
            }
        };
        counts.files_read += 1;

        let reader = BufReader::new(file);
        for (idx, line_res) in reader.lines().enumerate() {
            let line = match line_res {
                Ok(l) => l,
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::InvalidData {
                        counts.lines_skipped_invalid_utf8 += 1;
                    }
                    continue;
                }
            };
            counts.lines_scanned += 1;

            if line.to_ascii_lowercase().contains(&needle) {
                counts.matches += 1;
                if config.line_only {
                    write_out(output, format!("{}\n", line).as_bytes())?;
                } else {
                    write_out(
                        output,
                        format!("{}:{}| {}\n", file_path.display(), idx + 1, line).as_bytes(),
                    )?;
                }
            }
        }
    }

    if !config.no_summary {
        let elapsed = start.elapsed();
        eprintln!(
            "Searched in {} files, {} lines, {} matches. Skipped {} binary files, {} invalid UTF-8 lines. Took {:?}.",
            counts.files_read,
            counts.lines_scanned,
            counts.matches,
            counts.files_skipped_binary,
            counts.lines_skipped_invalid_utf8,
            elapsed
        );
    }

    Ok(counts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn quiet_config(path: PathBuf, text: &str) -> TextLookupConfig {
        TextLookupConfig::new(path, text.to_string(), vec![], false, true, true)
    }

    #[test]
    fn run_text_lookup_skips_binary_files_and_reports_counts() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("plain.txt"),
            "hello needle world\nsecond line\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("data.bin"),
            b"hello needle world\x00\nsecond line\n",
        )
        .unwrap();

        let config = quiet_config(dir.path().to_path_buf(), "needle");

        let counts = run_text_lookup(&config, &mut Vec::new()).unwrap();

        assert_eq!(
            counts,
            TextLookupCounts {
                files_read: 1,
                files_skipped_binary: 1,
                lines_scanned: 2,
                lines_skipped_invalid_utf8: 0,
                matches: 1,
            }
        );
    }

    #[test]
    fn run_text_lookup_counts_invalid_utf8_lines() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("mixed.txt"),
            b"good needle line\n\xFF\xFE broken line\n",
        )
        .unwrap();

        let config = quiet_config(dir.path().to_path_buf(), "needle");

        let counts = run_text_lookup(&config, &mut Vec::new()).unwrap();

        assert_eq!(counts.files_read, 1);
        assert_eq!(counts.files_skipped_binary, 0);
        assert_eq!(counts.lines_scanned, 1);
        assert_eq!(counts.lines_skipped_invalid_utf8, 1);
        assert_eq!(counts.matches, 1);
    }

    #[test]
    fn run_text_lookup_errors_on_missing_path() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("missing");

        let config = quiet_config(missing, "needle");

        assert!(run_text_lookup(&config, &mut Vec::new()).is_err());
    }

    #[test]
    fn run_text_lookup_writes_matching_lines_to_the_output() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("notes.txt"), "has needle here\nno match\n").unwrap();
        let config = quiet_config(dir.path().to_path_buf(), "needle");
        let mut output = Vec::new();

        let counts = run_text_lookup(&config, &mut output).unwrap();

        let text = String::from_utf8(output).unwrap();
        assert_eq!(counts.matches, 1);
        assert!(text.contains("has needle here"));
        assert!(!text.contains("no match"));
    }

    #[test]
    fn run_text_lookup_prefixes_file_and_line_without_line_only() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("notes.txt"), "has needle here\n").unwrap();
        let mut config = quiet_config(dir.path().to_path_buf(), "needle");
        config.line_only = false;
        let mut output = Vec::new();

        run_text_lookup(&config, &mut output).unwrap();

        let text = String::from_utf8(output).unwrap();
        assert!(text.contains("notes.txt:1| has needle here"));
    }

    struct ClosedPipe;

    impl Write for ClosedPipe {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "closed",
            ))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "closed",
            ))
        }
    }

    #[test]
    fn run_text_lookup_maps_closed_pipe_to_the_marker() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("notes.txt"), "has needle here\n").unwrap();
        let config = quiet_config(dir.path().to_path_buf(), "needle");

        let err = run_text_lookup(&config, &mut ClosedPipe).unwrap_err();

        assert!(err.is::<common_cli::broken_pipe::BrokenPipe>());
    }
}

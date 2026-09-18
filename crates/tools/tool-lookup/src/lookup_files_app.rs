use crate::models::{FilesLookupConfig, PatternMode};
use anyhow::{Result, anyhow};
use common_cli::broken_pipe::write_out;
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use regex::{Regex, RegexBuilder, RegexSet, RegexSetBuilder};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tracing::warn;
use walkdir::WalkDir;

const CLEAR_LINE: &str = "\r\x1b[2K";

fn clear_progress_line() {
    eprint!("{}", CLEAR_LINE);
    let _ = std::io::stderr().flush();
}

fn clean_path_for_display(p: &Path) -> String {
    #[cfg(windows)]
    {
        let s = p.display().to_string();
        if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{}", rest);
        }
        if let Some(rest) = s.strip_prefix(r"\\?\") {
            return rest.to_string();
        }
        s
    }
    #[cfg(not(windows))]
    {
        p.display().to_string()
    }
}

/// Walks the search path, writing each matched file's absolute path to
/// `output` one per line; only matches touch `output`. The summary line goes
/// to stderr unless suppressed, and traversal diagnostics go to the warn log.
///
/// # Errors
/// Fails with the [`common_cli::broken_pipe::BrokenPipe`] marker when the
/// consumer closes the pipe, with the underlying I/O error for any other
/// write failure, and when a search pattern cannot be compiled.
pub fn run_files_lookup(cfg: &FilesLookupConfig, output: &mut impl Write) -> Result<()> {
    let start = Instant::now();

    let base_path = PathBuf::from(&cfg.path);
    if !base_path.exists() {
        return Err(anyhow!(format!(
            "Path does not exist: {}",
            base_path.display()
        )));
    }

    // Build matchers
    let matcher = build_matcher(&cfg.patterns, &cfg.pattern_mode, cfg.case_sensitive)?;

    let mut folders_count: u64 = 0;
    let mut files_count: u64 = 0;
    let mut matches_count: u64 = 0;

    if cfg.no_recursive {
        // Current folder only
        folders_count = 1; // base folder
        if !cfg.no_progress {
            eprint!("{}Reading: {}", CLEAR_LINE, base_path.display());
            let _ = std::io::stderr().flush();
        }
        let dir_iter = match fs::read_dir(&base_path) {
            Ok(it) => it,
            Err(e) => {
                if !cfg.no_errors {
                    if !cfg.no_progress {
                        clear_progress_line();
                    }
                    warn!("{}: {}", e, base_path.display());
                }
                return Ok(());
            }
        };
        for ent in dir_iter.flatten() {
            let path = ent.path();
            if path.is_file() {
                files_count += 1;
                let name = match path.file_name().and_then(|s| s.to_str()) {
                    Some(s) => s,
                    None => continue,
                };
                if is_match(&matcher, name) {
                    matches_count += 1;
                    if !cfg.no_progress {
                        clear_progress_line();
                    }
                    let abs = absolute_path_str(&path);
                    write_out(output, format!("{}\n", abs).as_bytes())?;
                }
            }
        }
    } else {
        // Use WalkDir to report progress and errors
        let mut last_dir_printed: Option<PathBuf> = None;
        for entry_res in WalkDir::new(&base_path).into_iter() {
            match entry_res {
                Ok(entry) => {
                    if entry.file_type().is_dir() {
                        folders_count += 1;
                        if !cfg.no_progress {
                            print_progress_once(&mut last_dir_printed, entry.path());
                        }
                        continue;
                    }

                    // File
                    files_count += 1;
                    let name = match entry.file_name().to_str() {
                        Some(s) => s,
                        None => continue, // skip invalid utf-8 names
                    };
                    if is_match(&matcher, name) {
                        matches_count += 1;
                        // Clear progress line before printing a match to avoid overlap
                        if !cfg.no_progress {
                            clear_progress_line();
                        }
                        let abs = absolute_path_str(entry.path());
                        write_out(output, format!("{}\n", abs).as_bytes())?;
                    }
                }
                Err(e) => {
                    if !cfg.no_errors {
                        // Clear the progress line before logging the error
                        if !cfg.no_progress {
                            clear_progress_line();
                        }
                        let p = e
                            .path()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| "<unknown>".to_string());
                        warn!("{}: {}", brief_walkdir_error(&e), p);
                    }
                    // keep going
                }
            }
        }
    }

    // ensure we end the progress line with a newline
    if !cfg.no_progress {
        eprintln!();
    }

    if !cfg.no_summary {
        let elapsed = start.elapsed();
        eprintln!(
            "Summary: dirs={}, files={}, matches={}, elapsed={:?}",
            folders_count, files_count, matches_count, elapsed
        );
    }

    Ok(())
}

fn print_progress_once(last_dir_printed: &mut Option<PathBuf>, current: &Path) {
    // Only print when the directory changes to reduce noise.
    let cur_dir = if current.is_dir() {
        current
    } else {
        current.parent().unwrap_or(current)
    };
    if last_dir_printed.as_ref().map(|p| p.as_path()) != Some(cur_dir) {
        *last_dir_printed = Some(cur_dir.to_path_buf());
        eprint!("{}Reading: {}", CLEAR_LINE, cur_dir.display());
        let _ = std::io::stderr().flush();
    }
}

enum Matcher {
    Glob(GlobSet),
    RegexSet(RegexSet),
    RegexList(Vec<Regex>),
}

fn build_matcher(patterns: &[String], mode: &PatternMode, case_sensitive: bool) -> Result<Matcher> {
    match mode {
        PatternMode::Wildcard => build_globset(patterns, case_sensitive).map(Matcher::Glob),
        PatternMode::Regex => build_regexset(patterns, case_sensitive),
    }
}

fn build_globset(patterns: &[String], case_sensitive: bool) -> Result<GlobSet> {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        let mut gb = GlobBuilder::new(p);
        if !case_sensitive {
            gb.case_insensitive(true);
        }
        // Since we will match on file names (not full path), the pattern is applied as-is
        let g = gb.build()?;
        b.add(g);
    }
    Ok(b.build()?)
}

fn build_regexset(patterns: &[String], case_sensitive: bool) -> Result<Matcher> {
    // Prefer RegexSet when all patterns are valid as a set; if any pattern fails we fall back to individual regexes.
    let mut rsb = RegexSetBuilder::new(patterns);
    rsb.case_insensitive(!case_sensitive);
    match rsb.build() {
        Ok(rs) => Ok(Matcher::RegexSet(rs)),
        Err(_) => {
            let mut list = Vec::with_capacity(patterns.len());
            for p in patterns {
                let re = RegexBuilder::new(p)
                    .case_insensitive(!case_sensitive)
                    .build()?;
                list.push(re);
            }
            Ok(Matcher::RegexList(list))
        }
    }
}

fn is_match(m: &Matcher, file_name: &str) -> bool {
    match m {
        Matcher::Glob(gs) => gs.is_match(file_name),
        Matcher::RegexSet(rs) => rs.is_match(file_name),
        Matcher::RegexList(list) => list.iter().any(|r| r.is_match(file_name)),
    }
}

fn absolute_path(p: &Path) -> PathBuf {
    fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

fn absolute_path_str(p: &Path) -> String {
    let abs = absolute_path(p);
    clean_path_for_display(&abs)
}

fn brief_walkdir_error(e: &walkdir::Error) -> String {
    // Walkdir's error Display is already brief; keep it simple.
    e.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use common_cli::test_writers::ClosedPipe;

    fn matcher(patterns: &[&str], mode: PatternMode, case_sensitive: bool) -> Matcher {
        let patterns: Vec<String> = patterns.iter().map(|p| p.to_string()).collect();
        build_matcher(&patterns, &mode, case_sensitive).unwrap()
    }

    #[test]
    fn wildcard_matcher_is_case_insensitive_by_default() {
        let m = matcher(&["*.rs"], PatternMode::Wildcard, false);

        assert!(is_match(&m, "main.rs"));
        assert!(is_match(&m, "MAIN.RS"));
        assert!(!is_match(&m, "main.py"));
    }

    #[test]
    fn wildcard_matcher_honors_case_sensitivity() {
        let m = matcher(&["*.RS"], PatternMode::Wildcard, true);

        assert!(is_match(&m, "MAIN.RS"));
        assert!(!is_match(&m, "main.rs"));
    }

    #[test]
    fn wildcard_matcher_accepts_multiple_patterns() {
        let m = matcher(&["README.*", "LICENSE*"], PatternMode::Wildcard, false);

        assert!(is_match(&m, "readme.md"));
        assert!(is_match(&m, "license"));
        assert!(!is_match(&m, "changelog.md"));
    }

    #[test]
    fn regex_matcher_is_case_insensitive_by_default() {
        let m = matcher(&[r"^mydoc\.(pdf|epub)$"], PatternMode::Regex, false);

        assert!(matches!(m, Matcher::RegexSet(_)));
        assert!(is_match(&m, "mydoc.pdf"));
        assert!(is_match(&m, "MYDOC.EPUB"));
        assert!(!is_match(&m, "mydoc.txt"));
    }

    #[test]
    fn regex_matcher_honors_case_sensitivity() {
        let m = matcher(&[r"^readme\.md$"], PatternMode::Regex, true);

        assert!(is_match(&m, "readme.md"));
        assert!(!is_match(&m, "README.MD"));
    }

    #[test]
    fn regex_matcher_rejects_invalid_patterns() {
        let patterns = vec!["[unclosed".to_string()];

        assert!(build_matcher(&patterns, &PatternMode::Regex, false).is_err());
    }

    fn quiet_files_config(path: &Path, patterns: &[&str]) -> FilesLookupConfig {
        FilesLookupConfig::new(
            path.to_path_buf(),
            patterns.iter().map(|p| p.to_string()).collect(),
            PatternMode::Wildcard,
            false,
            false,
            true,
            false,
            true,
        )
    }

    #[test]
    fn run_files_lookup_current_only_keeps_the_summary_off_the_output() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("hit.log"), "x").unwrap();
        let mut cfg = quiet_files_config(dir.path(), &["*.log"]);
        cfg.no_recursive = true;
        cfg.no_summary = false;
        let mut output = Vec::new();

        run_files_lookup(&cfg, &mut output).unwrap();

        // The summary goes to stderr; only matches belong to the output sink.
        let text = String::from_utf8(output).unwrap();
        assert!(text.contains("hit.log"));
        assert!(!text.contains("Summary:"));
    }

    #[test]
    fn run_files_lookup_writes_matched_paths_to_the_output() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("hit.log"), "x").unwrap();
        std::fs::write(dir.path().join("miss.txt"), "x").unwrap();
        let cfg = quiet_files_config(dir.path(), &["*.log"]);
        let mut output = Vec::new();

        run_files_lookup(&cfg, &mut output).unwrap();

        let text = String::from_utf8(output).unwrap();
        assert!(text.contains("hit.log"));
        assert!(!text.contains("miss.txt"));
    }

    #[test]
    fn run_files_lookup_maps_closed_pipe_to_the_marker() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("hit.log"), "x").unwrap();
        let cfg = quiet_files_config(dir.path(), &["*.log"]);

        let err = run_files_lookup(&cfg, &mut ClosedPipe).unwrap_err();

        assert!(err.is::<common_cli::broken_pipe::BrokenPipe>());
    }

    #[test]
    fn regex_list_fallback_matches_like_the_set() {
        // Exercises the Matcher::RegexList arm that build_regexset falls back to
        // when the combined RegexSet cannot be built.
        let list = vec![
            RegexBuilder::new(r"^a\.txt$")
                .case_insensitive(true)
                .build()
                .unwrap(),
            RegexBuilder::new(r"^b\.txt$")
                .case_insensitive(true)
                .build()
                .unwrap(),
        ];
        let m = Matcher::RegexList(list);

        assert!(is_match(&m, "a.txt"));
        assert!(is_match(&m, "B.TXT"));
        assert!(!is_match(&m, "c.txt"));
    }
}

use crate::models::{FilesLookupConfig, PatternMode};
use anyhow::{anyhow, Result};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use regex::{Regex, RegexBuilder, RegexSet, RegexSetBuilder};
use shared::constants::general::DASH_LINE;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;
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

pub fn print_header(args: &FilesLookupConfig) {
    println!("Lookup v{}", env!("CARGO_PKG_VERSION"));
    println!("{}", DASH_LINE);
    println!("Mode: files (by filename)");
    println!("Path: {}", args.path);
    println!("Patterns: {:?}", args.patterns);
    println!(
        "Pattern type: {} | Case-sensitive: {} | Recursive: {}",
        match args.pattern_mode {
            PatternMode::Wildcard => "wildcard",
            PatternMode::Regex => "regex",
        },
        args.case_sensitive,
        args.recursive
    );
}

pub fn run_files_lookup(cfg: &FilesLookupConfig) -> Result<()> {
    let start = Instant::now();

    let base_path = PathBuf::from(&cfg.path);
    if !base_path.exists() {
        return Err(anyhow!(format!(
            "Path does not exist: {}",
            base_path.display()
        )));
    }

    // Build matchers
    let matcher = build_matcher(&cfg.patterns, cfg.pattern_mode, cfg.case_sensitive)?;

    let mut folders_count: u64 = 0;
    let mut files_count: u64 = 0;
    let mut matches_count: u64 = 0;

    if cfg.recursive {
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
                        println!("{}", abs);
                    }
                }
                Err(e) => {
                    if !cfg.no_errors {
                        // Clear the progress line before printing the error
                        if !cfg.no_progress {
                            clear_progress_line();
                        }
                        let p = e.path().map(|p| p.display().to_string()).unwrap_or_else(|| "<unknown>".to_string());
                        println!("{}: {}", brief_walkdir_error(&e), p);
                    }
                    // keep going
                }
            }
        }
        // ensure we end the progress line with a newline
        if !cfg.no_progress {
            eprintln!();
        }
    } else {
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
                    if !cfg.no_progress { clear_progress_line(); }
                    println!("{}: {}", e, base_path.display());
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
                    if !cfg.no_progress { clear_progress_line(); }
                    let abs = absolute_path_str(&path);
                    println!("{}", abs);
                }
            }
        }
        if !cfg.no_progress {
            eprintln!();
        }
    }

    if !cfg.no_summary {
        let elapsed = start.elapsed();
        println!(
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

fn build_matcher(patterns: &[String], mode: PatternMode, case_sensitive: bool) -> Result<Matcher> {
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
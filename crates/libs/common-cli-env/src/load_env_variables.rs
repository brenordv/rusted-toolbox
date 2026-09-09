use anyhow::{Context, Result};
use dotenvy::from_path;
use std::env;
use std::path::PathBuf;

/// Loads environment variables from the first `.env` file found, searching in
/// order:
/// 1. the executable's directory,
/// 2. the current working directory.
///
/// Call this early in `main`, before argument parsing, so later code sees the
/// loaded variables. Variables already present in the process environment win,
/// so a loaded `.env` can set unset variables but never overrides the caller's
/// environment.
///
/// # Errors
/// Returns an error when no candidate directory contains a `.env` file (the
/// message lists every directory that was checked), or when a found `.env` fails
/// to load. A candidate directory that cannot be resolved (the current
/// executable or working directory is unavailable) is skipped rather than
/// aborting the search.
///
/// # Caveats
/// - The current-working-directory fallback means running a tool inside an
///   untrusted directory imports that directory's `.env` into the process
///   environment.
/// - dotenvy echoes a malformed line back verbatim inside its parse error, so a
///   mistyped secret can surface on stderr or in a log file.
pub fn load_env_variables() -> Result<()> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Ok(exe_path) = env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            candidates.push(exe_dir.to_path_buf());
        }
    }

    if let Ok(cwd) = env::current_dir() {
        candidates.push(cwd);
    }

    load_env_from_dirs(&candidates)
}

/// Loads the `.env` from the first directory in `dirs` that has one, returning
/// its load result. Bails, listing every directory checked, when none has a
/// `.env`.
fn load_env_from_dirs(dirs: &[PathBuf]) -> Result<()> {
    for dir in dirs {
        let env_path = dir.join(".env");
        if env_path.exists() {
            return from_path(&env_path)
                .with_context(|| format!("failed to load .env from {}", dir.display()));
        }
    }

    anyhow::bail!(
        "no .env file found in any of the checked directories: {}",
        format_checked_dirs(dirs)
    )
}

fn format_checked_dirs(dirs: &[PathBuf]) -> String {
    if dirs.is_empty() {
        return "(none resolvable)".to_string();
    }

    dirs.iter()
        .map(|dir| dir.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn loads_env_from_a_single_directory() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join(".env"), "CCE_TEST_A=1\n").unwrap();

        load_env_from_dirs(&[dir.path().to_path_buf()]).unwrap();

        assert_eq!(std::env::var("CCE_TEST_A").unwrap(), "1");
    }

    #[test]
    fn first_directory_with_env_wins() {
        let first = tempdir().unwrap();
        let second = tempdir().unwrap();
        fs::write(first.path().join(".env"), "CCE_TEST_B=first\n").unwrap();
        fs::write(second.path().join(".env"), "CCE_TEST_B=second\n").unwrap();

        load_env_from_dirs(&[first.path().to_path_buf(), second.path().to_path_buf()]).unwrap();

        assert_eq!(std::env::var("CCE_TEST_B").unwrap(), "first");
    }

    #[test]
    fn missing_env_reports_every_checked_directory() {
        let first = tempdir().unwrap();
        let second = tempdir().unwrap();

        let error = load_env_from_dirs(&[first.path().to_path_buf(), second.path().to_path_buf()])
            .unwrap_err();

        let message = format!("{error:#}");
        assert!(message.contains(&first.path().display().to_string()));
        assert!(message.contains(&second.path().display().to_string()));
    }

    #[test]
    fn process_environment_wins_over_env_file() {
        // CCE_TEST_C is touched only by this test (the suite's one-test-per-var
        // convention for process-global state).
        std::env::set_var("CCE_TEST_C", "from-process");
        let dir = tempdir().unwrap();
        fs::write(dir.path().join(".env"), "CCE_TEST_C=from-file\n").unwrap();

        load_env_from_dirs(&[dir.path().to_path_buf()]).unwrap();

        assert_eq!(std::env::var("CCE_TEST_C").unwrap(), "from-process");
    }

    #[test]
    fn malformed_env_returns_context_error() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join(".env"), "CCE_TEST_D=\"unclosed\n").unwrap();

        let error = load_env_from_dirs(&[dir.path().to_path_buf()]).unwrap_err();

        let message = format!("{error:#}");
        assert!(message.contains("failed to load .env from"));
    }
}

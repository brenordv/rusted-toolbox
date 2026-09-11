use globset::GlobBuilder;
use std::collections::HashSet;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, error, warn};
use walkdir::{DirEntry, WalkDir};

/// The resolved target list plus whether any target argument failed.
#[derive(Debug)]
pub struct Expansion {
    pub targets: Vec<PathBuf>,
    pub failed: bool,
}

/// Resolves each target argument in order: an existing file is included
/// as-is, a directory is an error, anything else with glob metacharacters is
/// expanded (sorted within its argument), and a missing literal is an error.
/// The final list is deduplicated lexically by parsed path components, first
/// occurrence kept.
pub fn expand_targets(args: &[String]) -> Expansion {
    let mut failed = false;
    let mut collected: Vec<PathBuf> = Vec::new();

    for arg in args {
        expand_one(arg, &mut collected, &mut failed);
    }

    let mut seen: HashSet<OsString> = HashSet::new();
    let targets = collected
        .into_iter()
        .filter(|path| seen.insert(dedup_key(path)))
        .collect();

    Expansion { targets, failed }
}

/// Dedup key built from the path's parsed components, so one file reached
/// with different separators (`dir\a.txt` vs `dir/a.txt` on Windows) or
/// redundant `.` segments (a leading `./` included, which `components()`
/// alone would keep) counts once. Purely lexical: spellings that differ
/// through symlinks or `..` traversal still count separately.
fn dedup_key(path: &Path) -> OsString {
    path.components()
        .filter(|component| !matches!(component, std::path::Component::CurDir))
        .collect::<PathBuf>()
        .into_os_string()
}

fn expand_one(arg: &str, results: &mut Vec<PathBuf>, failed: &mut bool) {
    match fs::metadata(Path::new(arg)) {
        Ok(metadata) if metadata.is_file() => results.push(PathBuf::from(arg)),
        Ok(metadata) if metadata.is_dir() => {
            error!(target = %arg, "is a directory; pass a pattern like dir/*.log");
            *failed = true;
        }
        Ok(_) => {
            error!(target = %arg, "is not a regular file");
            *failed = true;
        }
        Err(open_error) => {
            if has_glob_metachar(arg) {
                expand_glob(arg, results, failed);
            } else {
                error!(target = %arg, "does not exist: {open_error}");
                *failed = true;
            }
        }
    }
}

/// The step-3 check is a plain contains-test that deliberately ignores escape
/// sequences; an argument like `foo\*.txt` enters glob expansion, where
/// globset's per-platform escape semantics resolve it.
fn has_glob_metachar(value: &str) -> bool {
    value.contains(['*', '?', '[', '{'])
}

fn expand_glob(arg: &str, results: &mut Vec<PathBuf>, failed: &mut bool) {
    // globset disables backslash escapes on Windows, so a backslash there is
    // always a path separator; on Unix it stays the escape character.
    #[cfg(windows)]
    let pattern = arg.replace('\\', "/");
    #[cfg(not(windows))]
    let pattern = arg.to_string();

    let (prefix, remainder) = split_literal_prefix(&pattern);
    let glob = match GlobBuilder::new(&remainder)
        .literal_separator(true)
        .case_insensitive(cfg!(windows))
        .build()
    {
        Ok(glob) => glob,
        Err(glob_error) => {
            error!(target = %arg, "invalid pattern: {glob_error}");
            *failed = true;
            return;
        }
    };
    let matcher = glob.compile_matcher();

    let components: Vec<&str> = remainder.split('/').collect();
    let unbounded = components.contains(&"**");
    let mut walker = WalkDir::new(&prefix).min_depth(1);
    if !unbounded {
        walker = walker.max_depth(components.len());
    }

    let mut matches: Vec<PathBuf> = Vec::new();
    for entry in walker {
        let entry = match entry {
            Ok(entry) => entry,
            Err(walk_error) => {
                debug!(prefix = %prefix.display(), "walk error: {walk_error}");
                continue;
            }
        };
        if !entry_is_file(&entry) {
            continue;
        }
        let Ok(relative) = entry.path().strip_prefix(&prefix) else {
            continue;
        };
        if matcher.is_match(to_slash_string(relative)) {
            matches.push(entry.path().to_path_buf());
        }
    }

    matches.sort();
    if matches.is_empty() {
        warn!(target = %arg, "matched no files");
    }
    results.extend(matches);
}

/// Splits a pattern into its leading metacharacter-free directory components
/// (the walk root; `.` when there are none) and the glob remainder.
fn split_literal_prefix(pattern: &str) -> (PathBuf, String) {
    let mut prefix_parts: Vec<&str> = Vec::new();
    let mut rest: Vec<&str> = Vec::new();
    for component in pattern.split('/') {
        if !rest.is_empty() || has_glob_metachar(component) {
            rest.push(component);
        } else {
            prefix_parts.push(component);
        }
    }

    let prefix = if prefix_parts.is_empty() {
        PathBuf::from(".")
    } else {
        let mut joined = prefix_parts.join("/");
        if joined.is_empty() {
            // The pattern was absolute with a metacharacter in its first
            // real component (`/{a,b}/...`): walk from the root.
            joined.push('/');
        } else if joined.ends_with(':') {
            // A bare drive designator (`C:`) is drive-relative on Windows;
            // the separator anchors the walk at the drive root.
            joined.push('/');
        }
        PathBuf::from(joined)
    };
    (prefix, rest.join("/"))
}

/// A yielded entry counts as a file when it is one itself, or when it is a
/// symlink whose target is a file (`fs::metadata` traverses the link). The
/// walker never follows symlinked directories, so the walk cannot cycle.
fn entry_is_file(entry: &DirEntry) -> bool {
    if entry.file_type().is_file() {
        return true;
    }
    if entry.path_is_symlink() {
        return match fs::metadata(entry.path()) {
            Ok(metadata) => metadata.is_file(),
            Err(link_error) => {
                debug!(path = %entry.path().display(), "broken symlink skipped: {link_error}");
                false
            }
        };
    }
    false
}

/// Renders a relative path with `/` separators for glob matching on every
/// platform.
fn to_slash_string(path: &Path) -> String {
    let parts: Vec<String> = path
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect();
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn touch(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, b"x").unwrap();
        path
    }

    fn expand(args: &[String]) -> Expansion {
        expand_targets(args)
    }

    fn names(expansion: &Expansion) -> Vec<String> {
        expansion
            .targets
            .iter()
            .map(|p| {
                p.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            })
            .collect()
    }

    #[test]
    fn literal_file_is_included_as_given() {
        let dir = tempdir().unwrap();
        let file = touch(dir.path(), "a.txt");
        let expansion = expand(&[file.to_string_lossy().into_owned()]);
        assert!(!expansion.failed);
        assert_eq!(expansion.targets, vec![file]);
    }

    #[test]
    fn directory_argument_is_an_error() {
        let dir = tempdir().unwrap();
        let expansion = expand(&[dir.path().to_string_lossy().into_owned()]);
        assert!(expansion.failed);
        assert!(expansion.targets.is_empty());
    }

    #[test]
    fn missing_literal_is_an_error() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("nope.txt");
        let expansion = expand(&[missing.to_string_lossy().into_owned()]);
        assert!(expansion.failed);
        assert!(expansion.targets.is_empty());
    }

    #[test]
    fn star_matches_only_one_level() {
        let dir = tempdir().unwrap();
        touch(dir.path(), "top.txt");
        touch(dir.path(), "sub/nested.txt");

        let pattern = format!("{}/*.txt", dir.path().display());
        let expansion = expand(&[pattern]);
        assert!(!expansion.failed);
        assert_eq!(names(&expansion), vec!["top.txt"]);
    }

    #[test]
    fn literal_prefix_component_scopes_the_walk() {
        let dir = tempdir().unwrap();
        touch(dir.path(), "logs/a.txt");
        touch(dir.path(), "other/b.txt");

        let pattern = format!("{}/logs/*.txt", dir.path().display());
        let expansion = expand(&[pattern]);
        assert_eq!(names(&expansion), vec!["a.txt"]);
    }

    #[test]
    fn double_star_recurses() {
        let dir = tempdir().unwrap();
        touch(dir.path(), "top.txt");
        touch(dir.path(), "a/b/deep.txt");
        touch(dir.path(), "a/skip.md");

        let pattern = format!("{}/**/*.txt", dir.path().display());
        let expansion = expand(&[pattern]);
        assert_eq!(names(&expansion), vec!["deep.txt", "top.txt"]);
    }

    #[test]
    fn question_mark_matches_single_character() {
        let dir = tempdir().unwrap();
        touch(dir.path(), "data-1.csv");
        touch(dir.path(), "data-22.csv");

        let pattern = format!("{}/data-?.csv", dir.path().display());
        let expansion = expand(&[pattern]);
        assert_eq!(names(&expansion), vec!["data-1.csv"]);
    }

    #[test]
    fn brace_alternation_matches() {
        let dir = tempdir().unwrap();
        touch(dir.path(), "a.txt");
        touch(dir.path(), "b.txt");
        touch(dir.path(), "c.txt");

        let pattern = format!("{}/{{a,b}}.txt", dir.path().display());
        let expansion = expand(&[pattern]);
        assert_eq!(names(&expansion), vec!["a.txt", "b.txt"]);
    }

    #[test]
    fn character_class_matches() {
        let dir = tempdir().unwrap();
        touch(dir.path(), "a.txt");
        touch(dir.path(), "b.txt");
        touch(dir.path(), "c.txt");

        let pattern = format!("{}/[ab].txt", dir.path().display());
        let expansion = expand(&[pattern]);
        assert_eq!(names(&expansion), vec!["a.txt", "b.txt"]);
    }

    #[test]
    fn malformed_pattern_errors_and_the_run_continues() {
        let dir = tempdir().unwrap();
        let good = touch(dir.path(), "good.txt");

        let bad = format!("{}/[unclosed", dir.path().display());
        let expansion = expand(&[bad, good.to_string_lossy().into_owned()]);
        assert!(expansion.failed);
        assert_eq!(expansion.targets, vec![good]);
    }

    #[test]
    fn zero_matches_warns_but_does_not_fail() {
        let dir = tempdir().unwrap();
        let pattern = format!("{}/*.log", dir.path().display());
        let expansion = expand(&[pattern]);
        assert!(!expansion.failed);
        assert!(expansion.targets.is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn glob_matching_is_case_insensitive_on_windows() {
        let dir = tempdir().unwrap();
        touch(dir.path(), "MiXeD.TXT");

        let pattern = format!("{}/*.txt", dir.path().display());
        let expansion = expand(&[pattern]);
        assert_eq!(names(&expansion), vec!["MiXeD.TXT"]);
    }

    #[cfg(not(windows))]
    #[test]
    fn glob_matching_is_case_sensitive_off_windows() {
        let dir = tempdir().unwrap();
        touch(dir.path(), "MiXeD.TXT");
        touch(dir.path(), "plain.txt");

        let pattern = format!("{}/*.txt", dir.path().display());
        let expansion = expand(&[pattern]);
        assert_eq!(names(&expansion), vec!["plain.txt"]);
    }

    #[cfg(windows)]
    #[test]
    fn backslash_pattern_is_a_path_separator_on_windows() {
        let dir = tempdir().unwrap();
        touch(dir.path(), "logs/a.txt");

        let pattern = format!("{}\\logs\\*.txt", dir.path().display());
        let expansion = expand(&[pattern]);
        assert_eq!(names(&expansion), vec!["a.txt"]);
    }

    #[cfg(not(windows))]
    #[test]
    fn backslash_escapes_a_metacharacter_off_windows() {
        let dir = tempdir().unwrap();
        touch(dir.path(), "foo*.txt");
        touch(dir.path(), "foox.txt");

        let pattern = format!("{}/foo\\*.txt", dir.path().display());
        let expansion = expand(&[pattern]);
        assert_eq!(names(&expansion), vec!["foo*.txt"]);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_files_match_and_symlinked_directories_are_not_walked() {
        let dir = tempdir().unwrap();
        let real_file = touch(dir.path(), "real.txt");
        let outside = tempdir().unwrap();
        touch(outside.path(), "hidden.txt");

        std::os::unix::fs::symlink(&real_file, dir.path().join("link.txt")).unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("subdir")).unwrap();

        let pattern = format!("{}/**/*.txt", dir.path().display());
        let expansion = expand(&[pattern]);
        assert_eq!(names(&expansion), vec!["link.txt", "real.txt"]);
    }

    #[cfg(unix)]
    #[test]
    fn broken_symlink_is_skipped() {
        let dir = tempdir().unwrap();
        touch(dir.path(), "real.txt");
        std::os::unix::fs::symlink(dir.path().join("gone"), dir.path().join("broken.txt")).unwrap();

        let pattern = format!("{}/*.txt", dir.path().display());
        let expansion = expand(&[pattern]);
        assert_eq!(names(&expansion), vec!["real.txt"]);
    }

    #[test]
    fn literal_prefix_split_anchors_roots_and_drives() {
        let (prefix, rest) = split_literal_prefix("C:/*.txt");
        assert_eq!(prefix, PathBuf::from("C:/"));
        assert_eq!(rest, "*.txt");

        let (prefix, rest) = split_literal_prefix("/{a,b}/x.txt");
        assert_eq!(prefix, PathBuf::from("/"));
        assert_eq!(rest, "{a,b}/x.txt");

        let (prefix, rest) = split_literal_prefix("logs/*.txt");
        assert_eq!(prefix, PathBuf::from("logs"));
        assert_eq!(rest, "*.txt");

        let (prefix, rest) = split_literal_prefix("*.txt");
        assert_eq!(prefix, PathBuf::from("."));
        assert_eq!(rest, "*.txt");
    }

    #[test]
    fn dedup_key_normalizes_curdir_prefix_to_the_plain_spelling() {
        assert_eq!(
            dedup_key(Path::new("./logs/a.txt")),
            dedup_key(Path::new("logs/a.txt"))
        );
    }

    #[test]
    fn duplicate_coverage_across_arguments_dedups() {
        let dir = tempdir().unwrap();
        let file = touch(dir.path(), "a.txt");

        let pattern = format!("{}/*.txt", dir.path().display());
        let expansion = expand(&[file.to_string_lossy().into_owned(), pattern]);
        assert!(!expansion.failed);
        assert_eq!(expansion.targets.len(), 1);
    }

    #[test]
    fn order_is_argument_order_then_sorted_within_argument() {
        let dir = tempdir().unwrap();
        touch(dir.path(), "z1.log");
        touch(dir.path(), "a2.log");
        let literal = touch(dir.path(), "zz-first.txt");

        let pattern = format!("{}/*.log", dir.path().display());
        let expansion = expand(&[literal.to_string_lossy().into_owned(), pattern]);
        assert_eq!(names(&expansion), vec!["zz-first.txt", "a2.log", "z1.log"]);
    }
}

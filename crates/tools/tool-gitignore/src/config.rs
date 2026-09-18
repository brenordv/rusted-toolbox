use std::collections::{BTreeMap, BTreeSet, HashSet};
use tracing::info;

/// Raw URL of the upstream AI agent artifacts template. Its uncommented
/// entries cover local agent state (Aider histories, Claude Code local
/// settings and logs, Gemini CLI debug files); the other agents it lists are
/// commented-out examples upstream, and the sanitize step drops comments.
pub const AI_ARTIFACTS_TEMPLATE_URL: &str =
    "https://raw.githubusercontent.com/github/gitignore/main/Global/Agents.gitignore";

/// Objective-C template, queued for `.mm` files and for `.m` files resolved to
/// Objective-C by [`Config::resolve_dot_m`].
pub const OBJECTIVE_C_TEMPLATE_URL: &str =
    "https://raw.githubusercontent.com/github/gitignore/main/Objective-C.gitignore";

/// Matlab template, queued for `.mat` files and for `.m` files resolved to
/// Matlab by [`Config::resolve_dot_m`].
pub const MATLAB_TEMPLATE_URL: &str =
    "https://raw.githubusercontent.com/brenordv/gitignore-files/refs/heads/master/Matlab.gitignore";

/// `.m` belongs to both Objective-C and Matlab, so it has no entry in the
/// static map: detection only records it, and [`Config::resolve_dot_m`] picks
/// the template(s) from companion evidence after the walk.
const DOT_M_KEY: &str = ".m";

/// Extension-to-template map. A `BTreeMap` iterates in sorted key order, so
/// key matching and the queued-for-download log stay deterministic run to run.
pub struct Config {
    mappings: BTreeMap<String, String>,
}

impl Config {
    pub fn new() -> Self {
        let mut mappings = BTreeMap::new();

        mappings.insert(
            ".py".to_string(),
            "https://raw.githubusercontent.com/github/gitignore/main/Python.gitignore".to_string(),
        );
        mappings.insert(
            ".meta".to_string(),
            "https://raw.githubusercontent.com/github/gitignore/main/Unity.gitignore".to_string(),
        );
        mappings.insert(
            ".java".to_string(),
            "https://raw.githubusercontent.com/github/gitignore/main/Java.gitignore".to_string(),
        );
        mappings.insert(
            ".js".to_string(),
            "https://raw.githubusercontent.com/github/gitignore/main/Node.gitignore".to_string(),
        );
        mappings.insert(
            ".ts".to_string(),
            "https://raw.githubusercontent.com/microsoft/TypeScript/main/.gitignore".to_string(),
        );
        mappings.insert(
            ".go".to_string(),
            "https://raw.githubusercontent.com/github/gitignore/main/Go.gitignore".to_string(),
        );
        mappings.insert(
            ".php".to_string(),
            "https://raw.githubusercontent.com/php/php-src/refs/heads/master/.gitignore"
                .to_string(),
        );
        mappings.insert(
            ".rb".to_string(),
            "https://raw.githubusercontent.com/github/gitignore/main/Ruby.gitignore".to_string(),
        );
        mappings.insert(
            ".swift".to_string(),
            "https://raw.githubusercontent.com/github/gitignore/main/Swift.gitignore".to_string(),
        );
        mappings.insert(
            ".dart".to_string(),
            "https://raw.githubusercontent.com/github/gitignore/main/Dart.gitignore".to_string(),
        );
        mappings.insert(
            ".scala".to_string(),
            "https://raw.githubusercontent.com/github/gitignore/main/Scala.gitignore".to_string(),
        );
        mappings.insert(
            ".tex".to_string(),
            "https://raw.githubusercontent.com/github/gitignore/main/TeX.gitignore".to_string(),
        );
        mappings.insert(
            ".jl".to_string(),
            "https://raw.githubusercontent.com/github/gitignore/main/Julia.gitignore".to_string(),
        );
        mappings.insert(
            ".hs".to_string(),
            "https://raw.githubusercontent.com/github/gitignore/main/Haskell.gitignore".to_string(),
        );
        mappings.insert(".vscode".to_string(), "https://raw.githubusercontent.com/github/gitignore/main/Global/VisualStudioCode.gitignore".to_string());

        mappings.insert(
            ".vs".to_string(),
            "https://raw.githubusercontent.com/github/gitignore/main/VisualStudio.gitignore"
                .to_string(),
        );
        mappings.insert(
            ".ds_store".to_string(),
            "https://raw.githubusercontent.com/github/gitignore/main/Global/macOS.gitignore"
                .to_string(),
        );
        mappings.insert(
            ".emacs.d".to_string(),
            "https://raw.githubusercontent.com/github/gitignore/main/Global/Emacs.gitignore"
                .to_string(),
        );
        mappings.insert(
            "next.config.js".to_string(),
            "https://raw.githubusercontent.com/github/gitignore/refs/heads/main/Nextjs.gitignore"
                .to_string(),
        );

        mappings.insert(
            ".r".to_string(),
            "https://raw.githubusercontent.com/github/gitignore/main/R.gitignore".to_string(),
        );

        ".cs|.sln|.csproj|.slnx".split("|").for_each(|key| {
            mappings.insert(
                key.to_string(),
                "https://raw.githubusercontent.com/dotnet/core/main/.gitignore".to_string(),
            );
        });

        ".sqlite|.sqlite3|.db|.db3|.s3db|.sdb|.sl3|.db-shm|.db-wal|.db-journal".split("|").for_each(|key| {
            mappings.insert(
                key.to_string(),
                "https://raw.githubusercontent.com/brenordv/gitignore-files/refs/heads/master/sqlite.gitignore".to_string(),
            );
        });

        ".gd|.godot".split("|").for_each(|key| {
            mappings.insert(
                key.to_string(),
                "https://raw.githubusercontent.com/github/gitignore/main/Godot.gitignore"
                    .to_string(),
            );
        });

        ".cpp|.hpp|.h".split("|").for_each(|key| {
            mappings.insert(
                key.to_string(),
                "https://raw.githubusercontent.com/github/gitignore/main/C%2B%2B.gitignore"
                    .to_string(),
            );
        });

        ".kt|.kts".split("|").for_each(|key| {
            mappings.insert(
                key.to_string(),
                "https://raw.githubusercontent.com/github/gitignore/main/Kotlin.gitignore"
                    .to_string(),
            );
        });

        ".tsx|.jsx".split("|").for_each(|key| {
            mappings.insert(
                key.to_string(),
                "https://raw.githubusercontent.com/facebook/react/main/.gitignore".to_string(),
            );
        });

        mappings.insert(".mm".to_string(), OBJECTIVE_C_TEMPLATE_URL.to_string());

        mappings.insert(".mat".to_string(), MATLAB_TEMPLATE_URL.to_string());

        ".pl|.pm".split("|").for_each(|key| {
            mappings.insert(
                key.to_string(),
                "https://raw.githubusercontent.com/github/gitignore/main/Perl.gitignore"
                    .to_string(),
            );
        });

        ".erl|.ex|.exs".split("|").for_each(|key| {
            mappings.insert(
                key.to_string(),
                "https://raw.githubusercontent.com/github/gitignore/main/Elixir.gitignore"
                    .to_string(),
            );
        });

        ".rs|.rs.bk".split("|").for_each(|key| {
            mappings.insert(
                key.to_string(),
                "https://raw.githubusercontent.com/github/gitignore/main/Rust.gitignore"
                    .to_string(),
            );
        });

        ".uproject|.umap|.uasset|.ubulk|.uexp|.uplugin|.usf|.ush".split("|").for_each(|key| {
            mappings.insert(key.to_string(), "https://raw.githubusercontent.com/github/gitignore/main/UnrealEngine.gitignore".to_string());
        });

        "hugo_stats.json|.hugo_build.lock|hugo.exe|hugo.darwin|hugo.linux".split("|").for_each(|key| {
            mappings.insert(key.to_string(), "https://raw.githubusercontent.com/github/gitignore/main/community/Golang/Hugo.gitignore".to_string());
        });

        // AI agent footprints: local state dirs, histories, and logs. Each
        // queues the shared artifacts template so agent-local files stay out
        // of git.
        ".claude|claude.local.md|.aider.chat.history.md|.aider.input.history|.cursor|.cursorrules|.windsurf|.codeium|.gemini|gemini-debug.log|.gemini-clipboard|.continue|.cline|.codex"
            .split("|")
            .for_each(|key| {
                mappings.insert(key.to_string(), AI_ARTIFACTS_TEMPLATE_URL.to_string());
            });

        ".idea|.fleet".split("|").for_each(|key| {
            mappings.insert(key.to_string(), "https://raw.githubusercontent.com/github/gitignore/main/Global/JetBrains.gitignore".to_string());
        });

        "thumbs.db|desktop.ini".split("|").for_each(|key| {
            mappings.insert(
                key.to_string(),
                "https://raw.githubusercontent.com/github/gitignore/main/Global/Windows.gitignore"
                    .to_string(),
            );
        });

        ".swp|.swo".split("|").for_each(|key| {
            mappings.insert(
                key.to_string(),
                "https://raw.githubusercontent.com/github/gitignore/main/Global/Vim.gitignore"
                    .to_string(),
            );
        });

        Self { mappings }
    }

    pub fn update_map_keys_for_file(
        &self,
        file: &str,
        keys_found: &mut HashSet<String>,
        pending_urls: &mut BTreeSet<String>,
    ) -> Vec<String> {
        let mut new_keys: Vec<String> = vec![];
        let file_lower = file.to_lowercase();

        for (key, url) in &self.mappings {
            if file_lower.ends_with(key) {
                if keys_found.insert(key.to_string()) {
                    new_keys.push(key.to_string());
                };
                pending_urls.insert(url.to_string());
            }
        }

        // `.m` queues nothing here (and stays out of the queued-for-download
        // log); resolve_dot_m picks its template(s) once the walk is done.
        if file_lower.ends_with(DOT_M_KEY) {
            keys_found.insert(DOT_M_KEY.to_string());
        }

        new_keys
    }

    /// Queues the template(s) for a detected `.m` footprint from companion
    /// evidence gathered during the walk: `.mm` means Objective-C, `.mat`
    /// means Matlab. With no companion (or both), both templates are queued:
    /// both URLs are reachable through the static map anyway, and
    /// over-covering beats silently dropping a detected footprint.
    pub fn resolve_dot_m(&self, keys_found: &HashSet<String>, pending_urls: &mut BTreeSet<String>) {
        if !keys_found.contains(DOT_M_KEY) {
            return;
        }

        match (keys_found.contains(".mm"), keys_found.contains(".mat")) {
            (true, false) => {
                info!(".m resolved to the Objective-C template (companion .mm found)");
                pending_urls.insert(OBJECTIVE_C_TEMPLATE_URL.to_string());
            }
            (false, true) => {
                info!(".m resolved to the Matlab template (companion .mat found)");
                pending_urls.insert(MATLAB_TEMPLATE_URL.to_string());
            }
            (true, true) => {
                info!(
                    ".m has both .mm and .mat companions; queuing the Objective-C and Matlab templates"
                );
                pending_urls.insert(OBJECTIVE_C_TEMPLATE_URL.to_string());
                pending_urls.insert(MATLAB_TEMPLATE_URL.to_string());
            }
            (false, false) => {
                info!(
                    ".m is ambiguous (no .mm or .mat companion); queuing the Objective-C and Matlab templates"
                );
                pending_urls.insert(OBJECTIVE_C_TEMPLATE_URL.to_string());
                pending_urls.insert(MATLAB_TEMPLATE_URL.to_string());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(entries: &[&str]) -> HashSet<String> {
        entries.iter().map(|k| k.to_string()).collect()
    }

    #[test]
    fn new_builds_non_empty_mappings_without_a_dot_m_entry() {
        let config = Config::new();

        assert!(!config.mappings.is_empty());
        assert!(!config.mappings.contains_key(DOT_M_KEY));
    }

    #[test]
    fn update_map_keys_for_file_matches_known_extension() {
        let config = Config::new();
        let mut keys_found = HashSet::new();
        let mut pending_urls = BTreeSet::new();

        let new_keys =
            config.update_map_keys_for_file("main.py", &mut keys_found, &mut pending_urls);

        assert!(new_keys.contains(&".py".to_string()));
        assert!(keys_found.contains(".py"));
        assert!(pending_urls.iter().any(|u| u.contains("Python.gitignore")));
    }

    #[test]
    fn update_map_keys_for_file_ignores_case() {
        let config = Config::new();
        let mut keys_found = HashSet::new();
        let mut pending_urls = BTreeSet::new();

        let new_keys =
            config.update_map_keys_for_file("MAIN.PY", &mut keys_found, &mut pending_urls);

        assert!(new_keys.contains(&".py".to_string()));
    }

    #[test]
    fn update_map_keys_for_file_reports_each_key_once() {
        let config = Config::new();
        let mut keys_found = HashSet::new();
        let mut pending_urls = BTreeSet::new();

        let first = config.update_map_keys_for_file("a.py", &mut keys_found, &mut pending_urls);
        let second = config.update_map_keys_for_file("b.py", &mut keys_found, &mut pending_urls);

        assert!(first.contains(&".py".to_string()));
        assert!(second.is_empty());
        assert!(pending_urls.iter().any(|u| u.contains("Python.gitignore")));
    }

    #[test]
    fn update_map_keys_for_file_matches_compound_key_by_whole_filename() {
        let config = Config::new();
        let mut keys_found = HashSet::new();
        let mut pending_urls = BTreeSet::new();

        let new_keys = config.update_map_keys_for_file(
            "my-app/next.config.js",
            &mut keys_found,
            &mut pending_urls,
        );

        // The whole-filename key matches, and so does the plain ".js" suffix
        // key, so both templates get queued.
        assert!(new_keys.contains(&"next.config.js".to_string()));
        assert!(new_keys.contains(&".js".to_string()));
        assert!(pending_urls.iter().any(|u| u.contains("Nextjs.gitignore")));
        assert!(pending_urls.iter().any(|u| u.contains("Node.gitignore")));
    }

    #[test]
    fn update_map_keys_for_file_matches_multi_extension_only_by_full_suffix() {
        let config = Config::new();
        let mut keys_found = HashSet::new();
        let mut pending_urls = BTreeSet::new();

        let new_keys =
            config.update_map_keys_for_file("archive.rs.bk", &mut keys_found, &mut pending_urls);

        // Matching is a plain ends_with, so "archive.rs.bk" hits the ".rs.bk"
        // key but not ".rs" (the string does not end with ".rs").
        assert_eq!(new_keys, vec![".rs.bk".to_string()]);
        assert!(!keys_found.contains(".rs"));
        assert!(pending_urls.iter().any(|u| u.contains("Rust.gitignore")));
    }

    #[test]
    fn update_map_keys_for_file_records_dot_m_without_queueing() {
        let config = Config::new();
        let mut keys_found = HashSet::new();
        let mut pending_urls = BTreeSet::new();

        let new_keys =
            config.update_map_keys_for_file("analysis.m", &mut keys_found, &mut pending_urls);

        assert!(new_keys.is_empty());
        assert!(keys_found.contains(DOT_M_KEY));
        assert!(pending_urls.is_empty());
    }

    #[test]
    fn update_map_keys_for_file_matches_dot_mm_via_the_static_map() {
        let config = Config::new();
        let mut keys_found = HashSet::new();
        let mut pending_urls = BTreeSet::new();

        let new_keys =
            config.update_map_keys_for_file("view.mm", &mut keys_found, &mut pending_urls);

        assert_eq!(new_keys, vec![".mm".to_string()]);
        assert!(!keys_found.contains(DOT_M_KEY));
        assert!(pending_urls.contains(OBJECTIVE_C_TEMPLATE_URL));
    }

    #[test]
    fn resolve_dot_m_alone_queues_both_templates() {
        let config = Config::new();
        let mut pending_urls = BTreeSet::new();

        config.resolve_dot_m(&keys(&[".m"]), &mut pending_urls);

        assert!(pending_urls.contains(OBJECTIVE_C_TEMPLATE_URL));
        assert!(pending_urls.contains(MATLAB_TEMPLATE_URL));
        assert_eq!(pending_urls.len(), 2);
    }

    #[test]
    fn resolve_dot_m_with_mm_companion_queues_objective_c_only() {
        let config = Config::new();
        let mut pending_urls = BTreeSet::new();

        config.resolve_dot_m(&keys(&[".m", ".mm"]), &mut pending_urls);

        assert_eq!(
            pending_urls.into_iter().collect::<Vec<_>>(),
            vec![OBJECTIVE_C_TEMPLATE_URL.to_string()]
        );
    }

    #[test]
    fn resolve_dot_m_with_mat_companion_queues_matlab_only() {
        let config = Config::new();
        let mut pending_urls = BTreeSet::new();

        config.resolve_dot_m(&keys(&[".m", ".mat"]), &mut pending_urls);

        assert_eq!(
            pending_urls.into_iter().collect::<Vec<_>>(),
            vec![MATLAB_TEMPLATE_URL.to_string()]
        );
    }

    #[test]
    fn resolve_dot_m_with_both_companions_queues_both_templates() {
        let config = Config::new();
        let mut pending_urls = BTreeSet::new();

        config.resolve_dot_m(&keys(&[".m", ".mm", ".mat"]), &mut pending_urls);

        assert!(pending_urls.contains(OBJECTIVE_C_TEMPLATE_URL));
        assert!(pending_urls.contains(MATLAB_TEMPLATE_URL));
        assert_eq!(pending_urls.len(), 2);
    }

    #[test]
    fn resolve_dot_m_is_a_noop_when_dot_m_was_not_detected() {
        let config = Config::new();
        let mut pending_urls = BTreeSet::new();

        config.resolve_dot_m(&keys(&[".mat"]), &mut pending_urls);

        assert!(pending_urls.is_empty());
    }

    #[test]
    fn update_map_keys_for_file_detects_ai_agent_state_footprints() {
        let config = Config::new();
        let mut keys_found = HashSet::new();
        let mut pending_urls = BTreeSet::new();

        let dir_keys =
            config.update_map_keys_for_file("my-app/.claude", &mut keys_found, &mut pending_urls);
        let history_keys = config.update_map_keys_for_file(
            "my-app/.aider.chat.history.md",
            &mut keys_found,
            &mut pending_urls,
        );

        assert!(dir_keys.contains(&".claude".to_string()));
        assert!(history_keys.contains(&".aider.chat.history.md".to_string()));
        assert!(pending_urls.contains(AI_ARTIFACTS_TEMPLATE_URL));
    }

    #[test]
    fn update_map_keys_for_file_maps_cursor_footprints_to_the_artifacts_template() {
        let config = Config::new();
        let mut keys_found = HashSet::new();
        let mut pending_urls = BTreeSet::new();

        let new_keys = config.update_map_keys_for_file(
            "my-app/.cursorrules",
            &mut keys_found,
            &mut pending_urls,
        );

        assert!(new_keys.contains(&".cursorrules".to_string()));
        assert!(pending_urls.contains(AI_ARTIFACTS_TEMPLATE_URL));
    }

    #[test]
    fn update_map_keys_for_file_does_not_match_cursor_tmp_or_cursor_output() {
        let config = Config::new();
        let mut keys_found = HashSet::new();
        let mut pending_urls = BTreeSet::new();

        let tmp_keys = config.update_map_keys_for_file(
            "my-app/.cursor-tmp",
            &mut keys_found,
            &mut pending_urls,
        );
        let output_keys = config.update_map_keys_for_file(
            "my-app/cursor-output",
            &mut keys_found,
            &mut pending_urls,
        );

        assert!(tmp_keys.is_empty());
        assert!(output_keys.is_empty());
        assert!(pending_urls.is_empty());
    }

    #[test]
    fn mappings_reference_no_oslook_urls() {
        let config = Config::new();

        assert!(config.mappings.values().all(|url| !url.contains("oslook")));
    }

    #[test]
    fn update_map_keys_for_file_unknown_extension_matches_nothing() {
        let config = Config::new();
        let mut keys_found = HashSet::new();
        let mut pending_urls = BTreeSet::new();

        let new_keys =
            config.update_map_keys_for_file("notes.unknownext", &mut keys_found, &mut pending_urls);

        assert!(new_keys.is_empty());
        assert!(keys_found.is_empty());
        assert!(pending_urls.is_empty());
    }
}

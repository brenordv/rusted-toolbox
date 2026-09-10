use std::collections::{BTreeSet, HashMap, HashSet};

/// Raw URL of the upstream AI agent artifacts template. Its uncommented
/// entries cover local agent state (Aider histories, Claude Code local
/// settings and logs, Gemini CLI debug files); the other agents it lists are
/// commented-out examples upstream, and the sanitize step drops comments.
pub const AI_ARTIFACTS_TEMPLATE_URL: &str =
    "https://raw.githubusercontent.com/github/gitignore/main/Global/Agents.gitignore";

pub struct Config {
    mappings: HashMap<String, String>,
    map_keys: Vec<String>,
}

impl Config {
    pub fn new() -> Self {
        let mut mappings = HashMap::new();

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

        ".m|.mm".split("|").for_each(|key| {
            mappings.insert(
                key.to_string(),
                "https://raw.githubusercontent.com/github/gitignore/main/Objective-C.gitignore"
                    .to_string(),
            );
        });

        ".mat|.m".split("|").for_each(|key| {
            mappings.insert(key.to_string(), "https://raw.githubusercontent.com/brenordv/gitignore-files/refs/heads/master/Matlab.gitignore".to_string());
        });

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

        // Sorted so key matching (and the queued-for-download log) does not
        // depend on HashMap iteration order.
        let mut map_keys = mappings
            .keys()
            .map(|x| x.to_string())
            .collect::<Vec<String>>();
        map_keys.sort();

        Self { mappings, map_keys }
    }

    pub fn update_map_keys_for_file(
        &self,
        file: &str,
        keys_found: &mut HashSet<String>,
        pending_urls: &mut BTreeSet<String>,
    ) -> Vec<String> {
        let mut new_keys: Vec<String> = vec![];

        self.map_keys.iter().for_each(|key| {
            if file.to_lowercase().ends_with(key) {
                if keys_found.insert(key.to_string()) {
                    new_keys.push(key.to_string());
                };
                pending_urls.insert(self.mappings.get(key).unwrap().to_string());
            }
        });

        new_keys
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_builds_non_empty_mappings_with_one_key_per_entry() {
        let config = Config::new();

        assert!(!config.mappings.is_empty());
        assert_eq!(config.mappings.len(), config.map_keys.len());
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

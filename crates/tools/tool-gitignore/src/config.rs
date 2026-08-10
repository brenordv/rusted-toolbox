use std::collections::{HashMap, HashSet};

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

        ".cursor|.cursor-tmp|cursor-output|.cursorrules".split("|").for_each(|key| {
            mappings.insert(key.to_string(), "https://raw.githubusercontent.com/oslook/cursor-ai-downloads/refs/heads/main/.gitignore".to_string());
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

        let map_keys = mappings
            .keys()
            .map(|x| x.to_string())
            .collect::<Vec<String>>();

        Self { mappings, map_keys }
    }

    pub fn update_map_keys_for_file(
        &self,
        file: &str,
        keys_found: &mut HashSet<String>,
        pending_urls: &mut HashSet<String>,
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
        let mut pending_urls = HashSet::new();

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
        let mut pending_urls = HashSet::new();

        let new_keys =
            config.update_map_keys_for_file("MAIN.PY", &mut keys_found, &mut pending_urls);

        assert!(new_keys.contains(&".py".to_string()));
    }

    #[test]
    fn update_map_keys_for_file_reports_each_key_once() {
        let config = Config::new();
        let mut keys_found = HashSet::new();
        let mut pending_urls = HashSet::new();

        let first = config.update_map_keys_for_file("a.py", &mut keys_found, &mut pending_urls);
        let second = config.update_map_keys_for_file("b.py", &mut keys_found, &mut pending_urls);

        assert!(first.contains(&".py".to_string()));
        assert!(second.is_empty());
        assert!(pending_urls.iter().any(|u| u.contains("Python.gitignore")));
    }

    #[test]
    fn update_map_keys_for_file_unknown_extension_matches_nothing() {
        let config = Config::new();
        let mut keys_found = HashSet::new();
        let mut pending_urls = HashSet::new();

        let new_keys =
            config.update_map_keys_for_file("notes.unknownext", &mut keys_found, &mut pending_urls);

        assert!(new_keys.is_empty());
        assert!(keys_found.is_empty());
        assert!(pending_urls.is_empty());
    }
}

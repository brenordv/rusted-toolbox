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

        ".cs|.sln|.csproj".to_string().split("|").for_each(|key| {
            mappings.insert(
                key.to_string(),
                "https://raw.githubusercontent.com/dotnet/core/main/.gitignore".to_string(),
            );
        });

        ".sqlite|.sqlite3|.db|.db3|.s3db|.sdb|.sl3|.db-shm|.db-wal|.db-journal".to_string().split("|").for_each(|key| {
            mappings.insert(
                key.to_string(),
                "https://raw.githubusercontent.com/brenordv/gitignore-files/refs/heads/master/sqlite.gitignore".to_string(),
            );
        });

        ".gd|.godot".to_string().split("|").for_each(|key| {
            mappings.insert(
                key.to_string(),
                "https://raw.githubusercontent.com/github/gitignore/main/Godot.gitignore"
                    .to_string(),
            );
        });

        ".cpp|.hpp|.h".to_string().split("|").for_each(|key| {
            mappings.insert(
                key.to_string(),
                "https://raw.githubusercontent.com/github/gitignore/main/C%2B%2B.gitignore"
                    .to_string(),
            );
        });

        ".kt|.kts".to_string().split("|").for_each(|key| {
            mappings.insert(
                key.to_string(),
                "https://raw.githubusercontent.com/github/gitignore/main/Kotlin.gitignore"
                    .to_string(),
            );
        });

        ".tsx|.jsx".to_string().split("|").for_each(|key| {
            mappings.insert(
                key.to_string(),
                "https://raw.githubusercontent.com/facebook/react/main/.gitignore".to_string(),
            );
        });

        ".m|.mm".to_string().split("|").for_each(|key| {
            mappings.insert(
                key.to_string(),
                "https://raw.githubusercontent.com/github/gitignore/main/Objective-C.gitignore"
                    .to_string(),
            );
        });

        ".mat|.m".to_string().split("|").for_each(|key| {
            mappings.insert(key.to_string(), "https://raw.githubusercontent.com/brenordv/gitignore-files/refs/heads/master/Matlab.gitignore".to_string());
        });

        ".pl|.pm".to_string().split("|").for_each(|key| {
            mappings.insert(
                key.to_string(),
                "https://raw.githubusercontent.com/github/gitignore/main/Perl.gitignore"
                    .to_string(),
            );
        });

        ".erl|.ex|.exs".to_string().split("|").for_each(|key| {
            mappings.insert(
                key.to_string(),
                "https://raw.githubusercontent.com/github/gitignore/main/Elixir.gitignore"
                    .to_string(),
            );
        });

        ".rs|.rs.bk".to_string().split("|").for_each(|key| {
            mappings.insert(
                key.to_string(),
                "https://raw.githubusercontent.com/github/gitignore/main/Rust.gitignore"
                    .to_string(),
            );
        });

        ".uproject|.umap|.uasset|.ubulk|.uexp|.uplugin|.usf|.ush".to_string().split("|").for_each(|key| {
            mappings.insert(key.to_string(), "https://raw.githubusercontent.com/github/gitignore/main/UnrealEngine.gitignore".to_string());
        });

        "hugo_stats.json|.hugo_build.lock|hugo.exe|hugo.darwin|hugo.linux".to_string().split("|").for_each(|key| {
            mappings.insert(key.to_string(), "https://raw.githubusercontent.com/github/gitignore/main/community/Golang/Hugo.gitignore".to_string());
        });

        ".cursor|.cursor-tmp|cursor-output|.cursorrules".to_string().split("|").for_each(|key| {
            mappings.insert(key.to_string(), "https://raw.githubusercontent.com/oslook/cursor-ai-downloads/refs/heads/main/.gitignore".to_string());
        });

        ".idea|.fleet".to_string().split("|").for_each(|key| {
            mappings.insert(key.to_string(), "https://raw.githubusercontent.com/github/gitignore/main/Global/JetBrains.gitignore".to_string());
        });

        "thumbs.db|desktop.ini".to_string().split("|").for_each(|key| {
            mappings.insert(key.to_string(), "https://raw.githubusercontent.com/github/gitignore/main/Global/Windows.gitignore".to_string());
        });

        ".swp|.swo".to_string().split("|").for_each(|key| {
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

//! The package ecosystems a check can cover, and how each service names them.

use crate::package::quote_input;
use serde::Serialize;
use std::fmt;
use std::str::FromStr;

/// A package ecosystem all three HTTP sources cover.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Ecosystem {
    Cargo,
    Nuget,
    Pypi,
    Npm,
    Rubygems,
}

/// Every word [`Ecosystem::from_str`] takes, in lowercase, with the ecosystem it means.
pub const ALIASES: &[(&str, Ecosystem)] = &[
    ("cargo", Ecosystem::Cargo),
    ("rust", Ecosystem::Cargo),
    ("crates", Ecosystem::Cargo),
    ("crates.io", Ecosystem::Cargo),
    ("nuget", Ecosystem::Nuget),
    ("dotnet", Ecosystem::Nuget),
    ("csharp", Ecosystem::Nuget),
    ("c#", Ecosystem::Nuget),
    ("pypi", Ecosystem::Pypi),
    ("python", Ecosystem::Pypi),
    ("pip", Ecosystem::Pypi),
    ("npm", Ecosystem::Npm),
    ("node", Ecosystem::Npm),
    ("javascript", Ecosystem::Npm),
    ("js", Ecosystem::Npm),
    ("typescript", Ecosystem::Npm),
    ("ts", Ecosystem::Npm),
    ("rubygems", Ecosystem::Rubygems),
    ("ruby", Ecosystem::Rubygems),
    ("gem", Ecosystem::Rubygems),
];

impl Ecosystem {
    /// Every ecosystem, in display order.
    pub const ALL: [Ecosystem; 5] = [
        Ecosystem::Cargo,
        Ecosystem::Nuget,
        Ecosystem::Pypi,
        Ecosystem::Npm,
        Ecosystem::Rubygems,
    ];

    /// The ecosystem's own word, as the report and the command line write it.
    pub fn name(self) -> &'static str {
        match self {
            Ecosystem::Cargo => "cargo",
            Ecosystem::Nuget => "nuget",
            Ecosystem::Pypi => "pypi",
            Ecosystem::Npm => "npm",
            Ecosystem::Rubygems => "rubygems",
        }
    }

    /// The ecosystem's name as people write it.
    pub fn label(self) -> &'static str {
        match self {
            Ecosystem::Cargo => "Cargo",
            Ecosystem::Nuget => "NuGet",
            Ecosystem::Pypi => "PyPI",
            Ecosystem::Npm => "npm",
            Ecosystem::Rubygems => "RubyGems",
        }
    }

    /// The ecosystem string OSV.dev uses, which is case-sensitive.
    pub fn osv_name(self) -> &'static str {
        match self {
            Ecosystem::Cargo => "crates.io",
            Ecosystem::Nuget => "NuGet",
            Ecosystem::Pypi => "PyPI",
            Ecosystem::Npm => "npm",
            Ecosystem::Rubygems => "RubyGems",
        }
    }

    /// The system name deps.dev uses in its URLs.
    pub fn deps_dev_system(self) -> &'static str {
        match self {
            Ecosystem::Cargo => "cargo",
            Ecosystem::Nuget => "nuget",
            Ecosystem::Pypi => "pypi",
            Ecosystem::Npm => "npm",
            Ecosystem::Rubygems => "rubygems",
        }
    }

    /// The `ecosystem` value the GitHub Advisory Database takes.
    pub fn github_ecosystem(self) -> &'static str {
        match self {
            Ecosystem::Cargo => "rust",
            Ecosystem::Nuget => "nuget",
            Ecosystem::Pypi => "pip",
            Ecosystem::Npm => "npm",
            Ecosystem::Rubygems => "rubygems",
        }
    }

    /// `name` in the form under which the ecosystem's registry treats spellings as one package.
    ///
    /// PyPI lowercases and turns every run of `.`, `-`, and `_` into one `-` (PEP 503); Cargo
    /// lowercases and treats `_` as `-`; NuGet lowercases; npm and RubyGems match exactly.
    pub fn normalize_name(self, name: &str) -> String {
        match self {
            Ecosystem::Pypi => normalize_pypi(name),
            Ecosystem::Cargo => name.to_ascii_lowercase().replace('_', "-"),
            Ecosystem::Nuget => name.to_ascii_lowercase(),
            Ecosystem::Npm | Ecosystem::Rubygems => name.to_owned(),
        }
    }

    /// Whether `a` and `b` name the same package in this ecosystem.
    pub fn same_package(self, a: &str, b: &str) -> bool {
        self.normalize_name(a) == self.normalize_name(b)
    }
}

fn normalize_pypi(name: &str) -> String {
    let mut normalized = String::with_capacity(name.len());
    let mut in_separator_run = false;
    for c in name.chars() {
        if matches!(c, '.' | '-' | '_') {
            if !in_separator_run {
                normalized.push('-');
            }
            in_separator_run = true;
        } else {
            normalized.push(c.to_ascii_lowercase());
            in_separator_run = false;
        }
    }
    normalized
}

impl fmt::Display for Ecosystem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for Ecosystem {
    type Err = String;

    /// Takes any word in [`ALIASES`], in any case, with surrounding spaces ignored.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let word = text.trim().to_ascii_lowercase();
        ALIASES
            .iter()
            .find(|(alias, _)| *alias == word)
            .map(|(_, ecosystem)| *ecosystem)
            .ok_or_else(|| {
                let names = Ecosystem::ALL.map(Ecosystem::name).join(", ");
                format!(
                    "unknown ecosystem {}; use one of: {names}",
                    quote_input(text.trim())
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::cargo(Ecosystem::Cargo, "crates.io", "cargo", "rust")]
    #[case::nuget(Ecosystem::Nuget, "NuGet", "nuget", "nuget")]
    #[case::pypi(Ecosystem::Pypi, "PyPI", "pypi", "pip")]
    #[case::npm(Ecosystem::Npm, "npm", "npm", "npm")]
    #[case::rubygems(Ecosystem::Rubygems, "RubyGems", "rubygems", "rubygems")]
    fn service_names_match_each_service(
        #[case] ecosystem: Ecosystem,
        #[case] osv: &str,
        #[case] deps_dev: &str,
        #[case] github: &str,
    ) {
        let names = (
            ecosystem.osv_name(),
            ecosystem.deps_dev_system(),
            ecosystem.github_ecosystem(),
        );

        assert_eq!(names, (osv, deps_dev, github));
    }

    #[test]
    fn from_str_takes_every_name_and_alias_in_any_case_trimmed() {
        for (alias, expected) in ALIASES {
            let shouted = format!("  {}  ", alias.to_ascii_uppercase());

            let parsed: Ecosystem = shouted.parse().unwrap();

            assert_eq!(parsed, *expected, "{alias}");
        }
    }

    #[test]
    fn from_str_unknown_word_lists_the_five_names() {
        let error = "cobol".parse::<Ecosystem>().unwrap_err();

        assert_eq!(
            error,
            "unknown ecosystem \"cobol\"; use one of: cargo, nuget, pypi, npm, rubygems"
        );
    }

    #[rstest]
    #[case::pypi_runs_and_case(Ecosystem::Pypi, "Foo.Bar_baz", "foo-bar-baz", true)]
    #[case::pypi_separator_run(Ecosystem::Pypi, "foo._-bar", "foo-bar", true)]
    #[case::cargo_underscore_and_case(Ecosystem::Cargo, "serde_json", "Serde-JSON", true)]
    #[case::nuget_case(Ecosystem::Nuget, "Newtonsoft.Json", "newtonsoft.json", true)]
    #[case::npm_exact(Ecosystem::Npm, "Left-Pad", "left-pad", false)]
    #[case::rubygems_exact(Ecosystem::Rubygems, "Rails", "rails", false)]
    #[case::cargo_dot_differs(Ecosystem::Cargo, "serde.json", "serde-json", false)]
    fn same_package_follows_each_registrys_rule(
        #[case] ecosystem: Ecosystem,
        #[case] a: &str,
        #[case] b: &str,
        #[case] expected: bool,
    ) {
        assert_eq!(ecosystem.same_package(a, b), expected);
    }
}

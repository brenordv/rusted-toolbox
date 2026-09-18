use std::{collections::HashMap, fs, io};

use camino::{Utf8Path, Utf8PathBuf};
use hurl_core::ast::SourceInfo;
use tracing::warn;

use crate::files::resolve::{FileResolver, ResolvedInclude};

use super::graph::IncludeTracker;
use super::parse::{
    FeedAssignment, FileDirectives, IncludeDirective, VarsDirective, parse_top_comment_directives,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct IncludeBehavior {
    pub quiet: bool,
    pub silent: bool,
}

/// An `env=NAME` include option, with the directive that set it so errors and
/// logs can cite the exact line. Inherited entries keep the ancestor
/// directive's citation.
#[derive(Debug, Clone)]
pub struct EnvOverride {
    pub name: String,
    pub from_file: Utf8PathBuf,
    pub line: u32,
}

/// One include directive's feed clause, with the directive that wrote it so
/// origins and errors can cite the exact line.
#[derive(Debug, Clone)]
pub struct IncludeFeed {
    pub include_path: Utf8PathBuf,
    pub from_file: Utf8PathBuf,
    pub line: u32,
    pub assignments: Vec<FeedAssignment>,
}

#[derive(Debug)]
pub struct LineMapping {
    pub source: Utf8PathBuf,
    pub line: u32,
}

#[derive(Debug)]
pub struct IncludeResult {
    pub merged: String,
    pub line_map: Vec<LineMapping>,
    pub behaviors: HashMap<Utf8PathBuf, IncludeBehavior>,
    pub vars: HashMap<Utf8PathBuf, Vec<VarsDirective>>,
    /// Environment overrides per resolved include path, in registration
    /// (directive-encounter) order. Only paths with an effective override
    /// appear; the first registration wins per path.
    pub env_overrides: Vec<(Utf8PathBuf, EnvOverride)>,
    /// Feed clauses in directive-encounter order. Every occurrence records,
    /// including repeats of an already-expanded include.
    pub feeds: Vec<IncludeFeed>,
}

impl IncludeResult {
    pub fn map_line(&self, merged_line: usize) -> Option<&LineMapping> {
        if merged_line == 0 {
            return None;
        }
        let idx = merged_line - 1;
        let mapping = self.line_map.get(idx)?;
        if mapping.line == 0 {
            None
        } else {
            Some(mapping)
        }
    }

    pub fn map_source(&self, source: &SourceInfo) -> Option<&LineMapping> {
        self.map_line(source.start.line)
    }

    pub fn behavior_for(&self, path: &Utf8Path) -> IncludeBehavior {
        self.behaviors.get(path).copied().unwrap_or_default()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum IncluderError {
    #[error("include cycle detected: {0}")]
    Cycle(String),
    #[error("failed to read {path}: {source}")]
    Io {
        path: Utf8PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to resolve include '{include}' referenced from {file} line {line}: {source}")]
    Resolve {
        file: Utf8PathBuf,
        include: String,
        line: u32,
        #[source]
        source: crate::files::resolve::ResolveError,
    },
    #[error("invalid directive in {path} line {line}: {message}")]
    Directive {
        path: Utf8PathBuf,
        line: u32,
        message: String,
    },
}

#[derive(Debug, Clone)]
pub struct Includer {
    resolver: FileResolver,
    show_boundaries: bool,
}

impl Includer {
    pub fn new(resolver: FileResolver) -> Self {
        Self {
            resolver,
            show_boundaries: true,
        }
    }

    pub fn with_boundaries(mut self, show_boundaries: bool) -> Self {
        self.show_boundaries = show_boundaries;
        self
    }

    pub fn merge(&self, entry: &Utf8Path) -> Result<IncludeResult, IncluderError> {
        let mut tracker = IncludeTracker::default();
        let mut state = MergeState::new(self.show_boundaries);

        let entry_path = entry.to_path_buf();
        state.register_behavior(entry, IncludeBehavior::default());
        state.register_env(entry, None);

        if let Some(cycle) = tracker.begin(&entry_path) {
            return Err(IncluderError::Cycle(format_cycle(&cycle)));
        }

        self.expand(
            entry,
            &mut tracker,
            &mut state,
            IncludeBehavior::default(),
            None,
        )?;
        tracker.complete(entry_path);

        Ok(state.finish())
    }

    fn expand(
        &self,
        file_path: &Utf8Path,
        tracker: &mut IncludeTracker,
        state: &mut MergeState,
        inherited_behavior: IncludeBehavior,
        inherited_env: Option<&EnvOverride>,
    ) -> Result<(), IncluderError> {
        let contents = fs::read_to_string(file_path).map_err(|source| IncluderError::Io {
            path: file_path.to_path_buf(),
            source,
        })?;

        let FileDirectives { includes, vars } =
            parse_top_comment_directives(&contents).map_err(|error| IncluderError::Directive {
                path: file_path.to_path_buf(),
                line: error.line,
                message: error.message,
            })?;
        state.register_vars(file_path, &vars);

        for directive in includes {
            let resolved = self.resolve_include(file_path, &directive)?;

            let directive_behavior =
                IncludeBehavior::from_options(&directive.options).combine(&inherited_behavior);
            state.register_behavior(&resolved.path, directive_behavior);

            // Every occurrence's feed applies, repeats of an already-expanded
            // include too, so this records before the is_expanded skip.
            if !directive.feed.is_empty() {
                state.register_feed(IncludeFeed {
                    include_path: resolved.path.clone(),
                    from_file: file_path.to_path_buf(),
                    line: directive.line_number,
                    assignments: directive.feed.clone(),
                });
            }

            // Nearest directive wins over the inherited override; absent one,
            // the subtree keeps following the ancestor's.
            let directive_env =
                parse_env_option(&directive.options, file_path, directive.line_number).map(
                    |name| EnvOverride {
                        name,
                        from_file: file_path.to_path_buf(),
                        line: directive.line_number,
                    },
                );
            let effective_env = directive_env.or_else(|| inherited_env.cloned());
            state.register_env(&resolved.path, effective_env.as_ref());

            if tracker.is_expanded(&resolved.path) {
                continue;
            }

            if let Some(cycle) = tracker.begin(&resolved.path) {
                return Err(IncluderError::Cycle(format_cycle(&cycle)));
            }

            let include_path = resolved.path.clone();
            let logical = resolved.logical.clone();

            if state.show_boundaries {
                state.push_boundary_start(&include_path, &logical);
            }
            self.expand(
                &resolved.path,
                tracker,
                state,
                directive_behavior,
                effective_env.as_ref(),
            )?;
            if state.show_boundaries {
                state.push_boundary_end(&include_path, &logical);
            }

            tracker.complete(resolved.path);
        }

        state.push_file_contents(file_path, &contents);
        Ok(())
    }

    fn resolve_include(
        &self,
        file_path: &Utf8Path,
        directive: &IncludeDirective,
    ) -> Result<ResolvedInclude, IncluderError> {
        let resolved = self
            .resolver
            .resolve_include(file_path, directive.path.as_str())
            .map_err(|source| IncluderError::Resolve {
                file: file_path.to_path_buf(),
                include: directive.path.clone(),
                line: directive.line_number,
                source,
            })?;

        Ok(resolved)
    }
}

struct MergeState {
    lines: Vec<LineRecord>,
    show_boundaries: bool,
    trailing_newline: bool,
    behaviors: HashMap<Utf8PathBuf, IncludeBehavior>,
    vars: HashMap<Utf8PathBuf, Vec<VarsDirective>>,
    env_registry: HashMap<Utf8PathBuf, Option<String>>,
    env_overrides: Vec<(Utf8PathBuf, EnvOverride)>,
    feeds: Vec<IncludeFeed>,
}

impl MergeState {
    fn new(show_boundaries: bool) -> Self {
        Self {
            lines: Vec::new(),
            show_boundaries,
            trailing_newline: false,
            behaviors: HashMap::new(),
            vars: HashMap::new(),
            env_registry: HashMap::new(),
            env_overrides: Vec::new(),
            feeds: Vec::new(),
        }
    }

    fn push_boundary_start(&mut self, path: &Utf8Path, logical: &str) {
        let content = format!("# --- begin include: {logical} ---");
        self.push_line_with_mapping(content, path.to_path_buf(), 0);
    }

    fn push_boundary_end(&mut self, path: &Utf8Path, logical: &str) {
        let content = format!("# --- end include: {logical} ---");
        self.push_line_with_mapping(content, path.to_path_buf(), 0);
    }

    fn push_file_contents(&mut self, path: &Utf8Path, contents: &str) {
        let source = path.to_path_buf();
        for (idx, line) in contents.lines().enumerate() {
            self.push_line_with_mapping(line.to_string(), source.clone(), idx as u32 + 1);
        }

        self.trailing_newline = contents.ends_with('\n');
    }

    fn push_line_with_mapping(&mut self, content: String, source: Utf8PathBuf, line: u32) {
        self.lines.push(LineRecord {
            content,
            mapping: LineMapping { source, line },
        });
        self.trailing_newline = false;
    }

    fn register_behavior(&mut self, path: &Utf8Path, behavior: IncludeBehavior) {
        self.behaviors
            .entry(path.to_path_buf())
            .and_modify(|existing| *existing = existing.combine(&behavior))
            .or_insert(behavior);
    }

    fn register_vars(&mut self, path: &Utf8Path, directives: &[VarsDirective]) {
        if directives.is_empty() {
            return;
        }

        let entry = self.vars.entry(path.to_path_buf()).or_default();
        entry.extend(directives.iter().cloned());
    }

    fn register_feed(&mut self, feed: IncludeFeed) {
        self.feeds.push(feed);
    }

    /// Records a path's effective environment override. The first registration
    /// wins; a later occurrence with a different effective value is dropped
    /// with a warning (its subtree was only ever expanded under the first
    /// occurrence's environment anyway).
    fn register_env(&mut self, path: &Utf8Path, effective: Option<&EnvOverride>) {
        let effective_name = effective.map(|env_override| env_override.name.clone());

        if let Some(existing) = self.env_registry.get(path) {
            if *existing != effective_name {
                warn!(
                    include = %path,
                    kept = existing.as_deref().unwrap_or("<none>"),
                    ignored = effective_name.as_deref().unwrap_or("<none>"),
                    "conflicting environment override on repeated include; first occurrence wins"
                );
            }
            return;
        }

        self.env_registry.insert(path.to_path_buf(), effective_name);
        if let Some(env_override) = effective {
            self.env_overrides
                .push((path.to_path_buf(), env_override.clone()));
        }
    }

    fn finish(self) -> IncludeResult {
        let mut merged = self
            .lines
            .iter()
            .map(|line| line.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        if self.trailing_newline && !merged.is_empty() {
            merged.push('\n');
        }

        let line_map = self
            .lines
            .into_iter()
            .map(|record| record.mapping)
            .collect();

        IncludeResult {
            merged,
            line_map,
            behaviors: self.behaviors,
            vars: self.vars,
            env_overrides: self.env_overrides,
            feeds: self.feeds,
        }
    }
}

/// Extracts the `env=NAME` value from a directive's options, case preserved.
/// Any other `key=value` option warns and is ignored (bare words stay silent
/// for back-compat; `IncludeBehavior::from_options` handles the known ones).
fn parse_env_option(options: &[String], file: &Utf8Path, line: u32) -> Option<String> {
    let mut env = None;

    for option in options {
        let Some((key, value)) = option.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();

        if key.eq_ignore_ascii_case("env") {
            if value.is_empty() {
                warn!(file = %file, line, "ignoring empty `env=` include option");
                continue;
            }
            if env.is_some() {
                warn!(
                    file = %file,
                    line,
                    "multiple `env=` include options on one directive; the last one wins"
                );
            }
            env = Some(value.to_string());
        } else {
            // The key alone identifies the typo; the value stays out of the
            // stream like every other option payload.
            warn!(
                file = %file,
                line,
                key = %key,
                "unknown include option key; option ignored"
            );
        }
    }

    env
}

struct LineRecord {
    content: String,
    mapping: LineMapping,
}

fn format_cycle(paths: &[Utf8PathBuf]) -> String {
    paths
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(" -> ")
}

impl IncludeBehavior {
    fn from_options(options: &[String]) -> Self {
        let mut behavior = IncludeBehavior::default();
        for option in options {
            match option.to_ascii_lowercase().as_str() {
                "quiet" => behavior.quiet = true,
                "silent" => {
                    behavior.silent = true;
                    behavior.quiet = true;
                }
                _ => {}
            }
        }
        behavior
    }

    fn combine(&self, other: &IncludeBehavior) -> IncludeBehavior {
        IncludeBehavior {
            quiet: self.quiet || other.quiet,
            silent: self.silent || other.silent,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn setup() -> (tempfile::TempDir, FileResolver, Utf8PathBuf) {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("requests");
        fs::create_dir_all(root.join("api")).expect("api dir");
        let root_utf8 = Utf8PathBuf::from_path_buf(root).expect("utf8 root");
        let api_root = root_utf8.join("api");
        (temp, FileResolver::new(root_utf8), api_root)
    }

    fn write(api_root: &Utf8Path, name: &str, contents: &str) -> Utf8PathBuf {
        let path = api_root.join(name);
        fs::write(path.as_std_path(), contents).expect("write hurl file");
        path
    }

    #[test]
    fn nested_includes_emit_children_before_parents() {
        let (_temp, resolver, api_root) = setup();
        write(&api_root, "c.hurl", "GET https://example.com/c\n");
        write(
            &api_root,
            "b.hurl",
            "# @include c\nGET https://example.com/b\n",
        );
        let entry = write(
            &api_root,
            "a.hurl",
            "# @include b\nGET https://example.com/a\n",
        );

        let result = Includer::new(resolver)
            .with_boundaries(false)
            .merge(entry.as_path())
            .expect("merge");

        let c_pos = result.merged.find("/c").expect("c merged");
        let b_pos = result.merged.find("/b").expect("b merged");
        let a_pos = result.merged.find("/a").expect("a merged");
        assert!(c_pos < b_pos && b_pos < a_pos);
    }

    #[test]
    fn duplicate_include_expands_once() {
        let (_temp, resolver, api_root) = setup();
        write(&api_root, "b.hurl", "GET https://example.com/b\n");
        let entry = write(
            &api_root,
            "a.hurl",
            "# @include b\n# @include b\nGET https://example.com/a\n",
        );

        let result = Includer::new(resolver)
            .with_boundaries(false)
            .merge(entry.as_path())
            .expect("merge");

        assert_eq!(
            result.merged.matches("GET https://example.com/b").count(),
            1
        );
    }

    #[test]
    fn boundary_markers_follow_the_toggle() {
        let (_temp, resolver, api_root) = setup();
        let included = write(&api_root, "b.hurl", "GET https://example.com/b\n");
        let entry = write(
            &api_root,
            "a.hurl",
            "# @include b\nGET https://example.com/a\n",
        );

        let logical = included
            .strip_prefix(resolver.requests_root())
            .expect("logical include path")
            .to_string();
        let begin_marker = format!("# --- begin include: {logical} ---");
        let end_marker = format!("# --- end include: {logical} ---");

        let with_markers = Includer::new(resolver.clone())
            .merge(entry.as_path())
            .expect("merge with boundaries");
        assert!(with_markers.merged.contains(&begin_marker));
        assert!(with_markers.merged.contains(&end_marker));

        let without_markers = Includer::new(resolver)
            .with_boundaries(false)
            .merge(entry.as_path())
            .expect("merge without boundaries");
        assert!(!without_markers.merged.contains("begin include"));
        assert!(!without_markers.merged.contains("end include"));
    }

    #[test]
    fn parse_env_option_grammar() {
        let file = Utf8PathBuf::from("a.hurl");
        let parse = |options: &[&str]| {
            let options: Vec<String> = options.iter().map(|s| s.to_string()).collect();
            parse_env_option(&options, file.as_path(), 1)
        };

        assert_eq!(parse(&["env=Dev"]), Some("Dev".to_string()));
        assert_eq!(parse(&["quiet", "ENV=dev"]), Some("dev".to_string()));
        assert_eq!(parse(&["env="]), None);
        assert_eq!(parse(&["evn=dev"]), None);
        assert_eq!(parse(&["quiet"]), None);
        // Duplicate env= options: the last one wins.
        assert_eq!(parse(&["env=dev", "env=prod"]), Some("prod".to_string()));
    }

    #[test]
    fn env_override_is_parsed_case_preserved_and_composes_with_modifiers() {
        let (_temp, resolver, api_root) = setup();
        let included = write(&api_root, "b.hurl", "GET https://example.com/b\n");
        let entry = write(
            &api_root,
            "a.hurl",
            "# @include:[quiet, env=Dev] b\nGET https://example.com/a\n",
        );

        let result = Includer::new(resolver)
            .with_boundaries(false)
            .merge(entry.as_path())
            .expect("merge");

        assert_eq!(result.env_overrides.len(), 1);
        let (path, env_override) = &result.env_overrides[0];
        assert_eq!(path, &included);
        assert_eq!(env_override.name, "Dev");
        assert_eq!(env_override.from_file, entry);
        assert_eq!(env_override.line, 1);
        assert!(result.behavior_for(included.as_path()).quiet);
    }

    #[test]
    fn env_override_inherits_down_the_subtree_and_nearest_directive_wins() {
        let (_temp, resolver, api_root) = setup();
        let d = write(&api_root, "d.hurl", "GET https://example.com/d\n");
        let c = write(&api_root, "c.hurl", "GET https://example.com/c\n");
        let b = write(
            &api_root,
            "b.hurl",
            "# @include c\n# @include:[env=prod] d\nGET https://example.com/b\n",
        );
        let entry = write(
            &api_root,
            "a.hurl",
            "# @include:[env=dev] b\nGET https://example.com/a\n",
        );

        let result = Includer::new(resolver)
            .with_boundaries(false)
            .merge(entry.as_path())
            .expect("merge");

        let by_path: HashMap<_, _> = result
            .env_overrides
            .iter()
            .map(|(path, env_override)| (path.clone(), env_override.clone()))
            .collect();

        assert_eq!(by_path[&b].name, "dev");
        assert_eq!(by_path[&c].name, "dev");
        // Inherited entries keep the ancestor directive's citation.
        assert_eq!(by_path[&c].from_file, entry);
        assert_eq!(by_path[&c].line, 1);
        assert_eq!(by_path[&d].name, "prod");
        assert_eq!(by_path[&d].from_file, b);
    }

    #[test]
    fn feeds_record_for_every_occurrence_in_order() {
        let (_temp, resolver, api_root) = setup();
        let b = write(&api_root, "b.hurl", "GET https://example.com/b\n");
        let entry = write(
            &api_root,
            "a.hurl",
            "# @include b -> { first=1 }\n# @include b -> { second=2 }\nGET https://example.com/a\n",
        );

        let result = Includer::new(resolver)
            .with_boundaries(false)
            .merge(entry.as_path())
            .expect("merge");

        assert_eq!(result.feeds.len(), 2);
        assert_eq!(result.feeds[0].include_path, b);
        assert_eq!(result.feeds[0].from_file, entry);
        assert_eq!(result.feeds[0].line, 1);
        assert_eq!(result.feeds[0].assignments[0].key, "first");
        assert_eq!(result.feeds[1].line, 2);
        assert_eq!(result.feeds[1].assignments[0].key, "second");
    }

    #[test]
    fn malformed_feed_surfaces_as_a_directive_error_with_the_file() {
        let (_temp, resolver, api_root) = setup();
        write(&api_root, "b.hurl", "GET https://example.com/b\n");
        let entry = write(
            &api_root,
            "a.hurl",
            "# @include b -> { broken }\nGET https://example.com/a\n",
        );

        let err = Includer::new(resolver)
            .merge(entry.as_path())
            .expect_err("malformed feed must fail");

        match err {
            IncluderError::Directive { path, line, .. } => {
                assert_eq!(path, entry);
                assert_eq!(line, 1);
            }
            other => panic!("expected directive error, got {other:?}"),
        }
    }

    #[test]
    fn conflicting_override_on_repeated_include_keeps_the_first() {
        let (_temp, resolver, api_root) = setup();
        let b = write(&api_root, "b.hurl", "GET https://example.com/b\n");
        let entry = write(
            &api_root,
            "a.hurl",
            "# @include:[env=dev] b\n# @include:[env=prod] b\nGET https://example.com/a\n",
        );

        let result = Includer::new(resolver)
            .with_boundaries(false)
            .merge(entry.as_path())
            .expect("merge");

        let overrides: Vec<_> = result
            .env_overrides
            .iter()
            .filter(|(path, _)| path == &b)
            .collect();
        assert_eq!(overrides.len(), 1);
        assert_eq!(overrides[0].1.name, "dev");
    }

    #[test]
    fn line_map_points_back_to_source_files() {
        let (_temp, resolver, api_root) = setup();
        let included = write(&api_root, "b.hurl", "GET https://example.com/b\n");
        let entry = write(
            &api_root,
            "a.hurl",
            "# @include b\nGET https://example.com/a\n",
        );

        let result = Includer::new(resolver)
            .merge(entry.as_path())
            .expect("merge");

        // Merged layout: begin marker, b line 1, end marker, then a lines 1-2.
        assert!(result.map_line(1).is_none());
        let included_mapping = result.map_line(2).expect("included mapping");
        assert_eq!(included_mapping.source, included);
        assert_eq!(included_mapping.line, 1);
        assert!(result.map_line(3).is_none());
        let entry_mapping = result.map_line(5).expect("entry mapping");
        assert_eq!(entry_mapping.source, entry);
        assert_eq!(entry_mapping.line, 2);
    }
}

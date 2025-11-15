use std::{collections::HashMap, fs, io};

use camino::{Utf8Path, Utf8PathBuf};
use hurl_core::ast::SourceInfo;

use crate::files::resolve::{FileResolver, ResolvedInclude};

use super::graph::IncludeTracker;
use super::parse::{parse_top_comment_directives, FileDirectives, IncludeDirective, VarsDirective};

#[derive(Debug, Clone, Copy, Default)]
pub struct IncludeBehavior {
    pub quiet: bool,
    pub silent: bool,
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

        if let Some(cycle) = tracker.begin(&entry_path) {
            return Err(IncluderError::Cycle(format_cycle(&cycle)));
        }

        self.expand(entry, &mut tracker, &mut state, IncludeBehavior::default())?;
        tracker.complete(entry_path);

        Ok(state.finish())
    }

    fn expand(
        &self,
        file_path: &Utf8Path,
        tracker: &mut IncludeTracker,
        state: &mut MergeState,
        inherited_behavior: IncludeBehavior,
    ) -> Result<(), IncluderError> {
        let contents = fs::read_to_string(file_path).map_err(|source| IncluderError::Io {
            path: file_path.to_path_buf(),
            source,
        })?;

        let FileDirectives { includes, vars } = parse_top_comment_directives(&contents);
        state.register_vars(file_path, &vars);

        for directive in includes {
            let resolved = self
                .resolve_include(file_path, &directive)
                .map_err(|source_error| source_error)?;

            let directive_behavior =
                IncludeBehavior::from_options(&directive.options).combine(&inherited_behavior);
            state.register_behavior(&resolved.path, directive_behavior);

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
            self.expand(&resolved.path, tracker, state, directive_behavior)?;
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
}

impl MergeState {
    fn new(show_boundaries: bool) -> Self {
        Self {
            lines: Vec::new(),
            show_boundaries,
            trailing_newline: false,
            behaviors: HashMap::new(),
            vars: HashMap::new(),
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

        let entry = self.vars.entry(path.to_path_buf()).or_insert_with(Vec::new);
        entry.extend(directives.iter().cloned());
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
        }
    }
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

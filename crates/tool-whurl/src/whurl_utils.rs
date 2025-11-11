use crate::files::FileResolver;
use camino::Utf8Path;

pub fn display_relative_path(resolver: &FileResolver, path: &Utf8Path) -> String {
    path.strip_prefix(resolver.requests_root())
        .map(|relative| relative.to_string())
        .unwrap_or_else(|_| path.to_string())
}

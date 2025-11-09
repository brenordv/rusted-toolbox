use std::collections::HashSet;

use camino::Utf8PathBuf;

#[derive(Debug, Default)]
pub struct IncludeTracker {
    stack: Vec<Utf8PathBuf>,
    expanded: HashSet<Utf8PathBuf>,
}

impl IncludeTracker {
    pub fn begin(&mut self, path: &Utf8PathBuf) -> Option<Vec<Utf8PathBuf>> {
        if let Some(position) = self.stack.iter().position(|p| p == path) {
            let mut cycle = self.stack[position..].to_vec();
            cycle.push(path.clone());
            return Some(cycle);
        }

        self.stack.push(path.clone());
        None
    }

    pub fn complete(&mut self, path: Utf8PathBuf) {
        self.expanded.insert(path.clone());
        self.stack.pop();
    }

    pub fn is_expanded(&self, path: &Utf8PathBuf) -> bool {
        self.expanded.contains(path)
    }
}

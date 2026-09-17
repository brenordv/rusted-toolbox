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

#[cfg(test)]
mod tests {
    use super::*;

    fn path(name: &str) -> Utf8PathBuf {
        Utf8PathBuf::from(name)
    }

    #[test]
    fn reports_cycle_path_when_a_file_reenters_the_stack() {
        let mut tracker = IncludeTracker::default();
        assert!(tracker.begin(&path("a.hurl")).is_none());
        assert!(tracker.begin(&path("b.hurl")).is_none());

        let cycle = tracker.begin(&path("a.hurl")).expect("cycle detected");
        assert_eq!(cycle, vec![path("a.hurl"), path("b.hurl"), path("a.hurl")]);
    }

    #[test]
    fn detects_self_include() {
        let mut tracker = IncludeTracker::default();
        assert!(tracker.begin(&path("a.hurl")).is_none());

        let cycle = tracker.begin(&path("a.hurl")).expect("self cycle detected");
        assert_eq!(cycle, vec![path("a.hurl"), path("a.hurl")]);
    }

    #[test]
    fn diamond_include_expands_shared_file_once() {
        let mut tracker = IncludeTracker::default();
        assert!(tracker.begin(&path("a.hurl")).is_none());

        assert!(tracker.begin(&path("b.hurl")).is_none());
        assert!(!tracker.is_expanded(&path("d.hurl")));
        assert!(tracker.begin(&path("d.hurl")).is_none());
        tracker.complete(path("d.hurl"));
        tracker.complete(path("b.hurl"));

        assert!(tracker.begin(&path("c.hurl")).is_none());
        assert!(tracker.is_expanded(&path("d.hurl")));
        tracker.complete(path("c.hurl"));
        tracker.complete(path("a.hurl"));
    }
}

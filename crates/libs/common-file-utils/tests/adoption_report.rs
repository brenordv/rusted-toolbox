//! `Adoption::report` writes through the global subscriber, so its lines are checked in a test
//! binary of their own with one test: a callsite's cached interest is process-wide, which makes
//! a scoped capture miss events when tests run in parallel.

use common_file_utils::adoption::{Adoption, KeptReason};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tracing_subscriber::fmt::MakeWriter;

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Captured {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

impl Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for Captured {
    type Writer = Captured;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

fn report_lines(adoption: &Adoption) -> Vec<String> {
    let captured = Captured::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(captured.clone())
        .with_ansi(false)
        .with_max_level(tracing::Level::TRACE)
        .finish();
    tracing::subscriber::with_default(subscriber, || {
        adoption.report("the test data", Path::new("new-place"));
    });
    captured.text().lines().map(str::to_owned).collect()
}

#[test]
fn a_move_or_data_left_behind_logs_one_warning_naming_both_places_and_the_rest_log_nothing() {
    let old = PathBuf::from("old-place");
    let silent = [Adoption::Nothing, Adoption::AlreadyThere];
    let warnings: [(Adoption, &[&str]); 6] = [
        (Adoption::Moved { from: old.clone() }, &["moved"]),
        (
            Adoption::BothPresent {
                legacy: old.clone(),
            },
            &["not read"],
        ),
        (
            Adoption::KeptLegacy {
                legacy: old.clone(),
                reason: KeptReason::SymbolicLink,
            },
            &["create a link at the new path"],
        ),
        (
            Adoption::KeptLegacy {
                legacy: old.clone(),
                reason: KeptReason::Failed(io::Error::other("disk on fire")),
            },
            &["disk on fire"],
        ),
        (
            Adoption::OldNameLeft {
                legacy: old.clone(),
                error: io::Error::other("held by a scanner"),
            },
            &["held by a scanner", "no longer read", "delete it by hand"],
        ),
        (
            Adoption::KeptLegacy {
                legacy: old.clone(),
                reason: KeptReason::InUse("a journal beside it".to_owned()),
            },
            &["a journal beside it"],
        ),
    ];

    for adoption in silent {
        let lines = report_lines(&adoption);

        assert!(lines.is_empty(), "{adoption:?}: {lines:?}");
    }
    for (adoption, words) in warnings {
        let lines = report_lines(&adoption);

        assert_eq!(lines.len(), 1, "{adoption:?}: {lines:?}");
        let line = &lines[0];
        assert!(line.contains("WARN"), "{line}");
        assert!(line.contains("the test data"), "{line}");
        assert!(line.contains("old-place"), "{line}");
        assert!(line.contains("new-place"), "{line}");
        assert!(words.iter().all(|word| line.contains(word)), "{line}");
    }
}

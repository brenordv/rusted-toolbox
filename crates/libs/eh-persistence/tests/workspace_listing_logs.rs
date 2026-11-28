//! Listing the workspace logs one summary line, not one "Loaded config.json"
//! line per profile. This is the only test in its binary, so it can install a
//! global subscriber.

use eh_persistence::config::CONFIG_FILE_NAME;
use eh_persistence::workspace::Workspace;
use std::io::Write;
use std::sync::{Arc, Mutex};

const READY_CONFIG: &str = r#"{"tool":"eh","schema_version":1,"target":{"namespace":"orders.servicebus.windows.net","event_hub":"orders","auth":{"method":"credential","credential":{"kind":"developer-tools"}}}}"#;

#[derive(Clone, Default)]
struct SharedBuffer(Arc<Mutex<Vec<u8>>>);

impl Write for SharedBuffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn listing_logs_a_summary_and_no_per_profile_line() {
    let buffer = SharedBuffer::default();
    let writer = buffer.clone();
    tracing::subscriber::set_global_default(
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish(),
    )
    .unwrap();
    let root = tempfile::tempdir().unwrap();
    for name in ["billing", "orders"] {
        let dir = root.path().join(name);
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join(CONFIG_FILE_NAME), READY_CONFIG).unwrap();
    }

    let entries = Workspace::at(root.path().to_owned()).list().await.unwrap();

    assert_eq!(entries.len(), 2);
    let logs = String::from_utf8(buffer.0.lock().unwrap().clone()).unwrap();
    assert!(logs.contains("Listed profiles"), "{logs}");
    assert!(logs.contains("ready=2"), "{logs}");
    assert!(!logs.contains("Loaded config.json"), "{logs}");
}

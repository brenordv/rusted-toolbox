use chrono::{DateTime, Duration, Utc};
use shared_eventhub_core::connection::EventHubIdentity;
use shared_eventhub_core::engine::{Checkpoint, MessageSink};
use shared_eventhub_core::models::{BodyKind, CapturedMessage, MessageKey};
use shared_eventhub_persistence::paths::DataPaths;
use shared_eventhub_persistence::store::MessageStore;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const EXE: &str = env!("CARGO_BIN_EXE_eh");
const SECRET_KEY: &str = "c2VjcmV0LWtleS1uZXZlci1wcmludGVk";
const NAMESPACE: &str = "orders.servicebus.windows.net";
const HUB: &str = "orders";

fn connection_string() -> String {
    format!(
        "Endpoint=sb://{NAMESPACE}/;SharedAccessKeyName=listen;SharedAccessKey={SECRET_KEY};EntityPath={HUB}"
    )
}

fn eh(cwd: &Path, args: &[&str]) -> Output {
    Command::new(EXE)
        .args(args)
        .current_dir(cwd)
        .env_remove("EH_CONNECTION_STRING")
        .env_remove("EH_CLIENT_SECRET")
        .env_remove("RUST_LOG")
        .output()
        .expect("failed to run eh")
}

fn stdout_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn files_under(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files = Vec::new();
    for entry in entries {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(files_under(&path));
        } else {
            files.push(path);
        }
    }
    files.sort();
    files
}

fn read_all(files: &[PathBuf]) -> String {
    files
        .iter()
        .map(|file| std::fs::read_to_string(file).unwrap())
        .collect::<Vec<_>>()
        .join("\n")
}

fn message(sequence_number: i64, enqueued_time: DateTime<Utc>, body: &str) -> CapturedMessage {
    CapturedMessage {
        key: MessageKey {
            partition_id: "0".to_owned(),
            sequence_number,
        },
        offset: Some(format!("offset-{sequence_number}")),
        partition_key: None,
        enqueued_time: Some(enqueued_time),
        processed_at: enqueued_time,
        message_id: None,
        correlation_id: None,
        content_type: None,
        properties: BTreeMap::new(),
        body: body.to_owned(),
        body_kind: BodyKind::Utf8,
    }
}

/// Archives three messages enqueued three, two, and one hour ago, then closes
/// the archive so the binary can open it.
async fn seed_archive(data: &Path) -> Vec<DateTime<Utc>> {
    let store = MessageStore::open(&DataPaths::new(data)).unwrap();
    store
        .bind(&EventHubIdentity {
            fully_qualified_namespace: NAMESPACE.to_owned(),
            event_hub_name: HUB.to_owned(),
        })
        .await
        .unwrap();
    let now = Utc::now();
    let times: Vec<DateTime<Utc>> = (1..=3)
        .map(|hours_ago| now - Duration::hours(4 - hours_ago))
        .collect();
    let bodies = ["order-1 created", "payment ok", "order-2 created"];
    for (index, (time, body)) in times.iter().zip(bodies).enumerate() {
        let sequence_number = i64::try_from(index).unwrap() + 1;
        let message = message(sequence_number, *time, body);
        let checkpoint = Checkpoint {
            partition_id: "0".to_owned(),
            sequence_number,
            offset: message.offset.clone(),
            enqueued_time: message.enqueued_time,
            updated_at: Utc::now(),
        };
        store.store(&message, &checkpoint, true).await.unwrap();
    }
    store.close().await.unwrap();
    times
}

#[test]
fn help_exits_zero_and_lists_the_subcommands_and_key_flags() {
    let root = tempfile::tempdir().unwrap();

    let top = eh(root.path(), &["--help"]);
    let read = eh(root.path(), &["read", "--help"]);
    let export = eh(root.path(), &["export", "--help"]);

    for output in [&top, &read, &export] {
        assert!(output.status.success(), "{}", stderr_of(output));
    }
    assert!(stdout_of(&top).contains("read") && stdout_of(&top).contains("export"));
    assert!(stdout_of(&read).contains("--connection-string"));
    assert!(stdout_of(&read).contains("--disk-mode"));
    assert!(stdout_of(&export).contains("--export-format"));
    assert!(stdout_of(&export).contains("--ignore-export-log"));
}

#[test]
fn read_without_a_target_fails_before_touching_anything() {
    let root = tempfile::tempdir().unwrap();

    let output = eh(root.path(), &["read"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_of(&output).contains("No event hub to read"),
        "{}",
        stderr_of(&output)
    );
    assert!(files_under(root.path()).is_empty());
}

#[test]
fn a_data_folder_that_is_not_an_eh_folder_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(project.join(".gitignore"), "target/\n").unwrap();
    std::fs::write(project.join("Cargo.toml"), "[package]\n").unwrap();

    let output = eh(
        root.path(),
        &[
            "read",
            "-s",
            &connection_string(),
            "-b",
            "project",
            "--no-config",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_of(&output).contains("is not an eh data folder"),
        "{}",
        stderr_of(&output)
    );
    assert!(!project.join("archive.redb").exists());
    assert!(!project.join("secrets.redb").exists());
    assert!(!stderr_of(&output).contains(SECRET_KEY));
}

#[tokio::test]
async fn export_writes_every_archived_message_and_the_export_log_skips_them_next_time() {
    let root = tempfile::tempdir().unwrap();
    seed_archive(&root.path().join(".eh-data")).await;

    let first = eh(root.path(), &["export"]);
    let second = eh(root.path(), &["export"]);
    let again = eh(root.path(), &["export", "-i"]);

    assert!(first.status.success(), "{}", stderr_of(&first));
    assert!(
        stdout_of(&first).contains("Exported: 3 | Skipped (already exported): 0 | Failed: 0"),
        "{}",
        stdout_of(&first)
    );
    assert!(
        stdout_of(&second).contains("Exported: 0 | Skipped (already exported): 3"),
        "{}",
        stdout_of(&second)
    );
    assert!(
        stdout_of(&again).contains("Exported: 3 |"),
        "{}",
        stdout_of(&again)
    );
    let files = files_under(&root.path().join(".eh-data").join("exports"));
    assert_eq!(files.len(), 3, "{files:?}");
    assert!(read_all(&files).contains("payment ok"));
}

#[tokio::test]
async fn csv_and_json_carry_the_bound_hub_and_metadata_can_be_left_out() {
    let root = tempfile::tempdir().unwrap();
    seed_archive(&root.path().join(".eh-data")).await;

    let csv = eh(
        root.path(),
        &[
            "export",
            "-f",
            "csv",
            "--export-folder",
            "csv",
            "--condense-output",
        ],
    );
    let json = eh(
        root.path(),
        &["export", "-f", "json", "--export-folder", "json", "-i"],
    );
    let bare = eh(
        root.path(),
        &[
            "export",
            "-f",
            "csv",
            "--export-folder",
            "bare",
            "--include-metadata",
            "false",
            "--condense-output",
            "-i",
        ],
    );

    for output in [&csv, &json, &bare] {
        assert!(output.status.success(), "{}", stderr_of(output));
    }
    let data = root.path().join(".eh-data");
    let csv_text = read_all(&files_under(&data.join("csv")));
    assert!(
        csv_text.starts_with("entity_path,partition_id,event_id,timestamp,message_content"),
        "{csv_text}"
    );
    assert!(csv_text.contains("orders,0,2,"), "{csv_text}");
    let json_text = read_all(&files_under(&data.join("json")));
    assert!(
        json_text.contains("\"entity_path\": \"orders\""),
        "{json_text}"
    );
    let bare_text = read_all(&files_under(&data.join("bare")));
    assert!(bare_text.starts_with("message_content\n"), "{bare_text}");
    assert!(!bare_text.contains("orders,"), "{bare_text}");
}

#[tokio::test]
async fn time_and_regex_filters_narrow_the_export() {
    let root = tempfile::tempdir().unwrap();
    let times = seed_archive(&root.path().join(".eh-data")).await;
    let after_first = (times[0] + Duration::minutes(30)).to_rfc3339();

    let newer = eh(
        root.path(),
        &[
            "export",
            "--export-folder",
            "newer",
            "--newer-than",
            &after_first,
        ],
    );
    let regex = eh(
        root.path(),
        &[
            "export",
            "--export-folder",
            "regex",
            "--filter-regex",
            "order-\\d+",
            "-i",
        ],
    );

    assert!(newer.status.success(), "{}", stderr_of(&newer));
    assert!(regex.status.success(), "{}", stderr_of(&regex));
    let data = root.path().join(".eh-data");
    let newer_text = read_all(&files_under(&data.join("newer")));
    assert_eq!(files_under(&data.join("newer")).len(), 2);
    assert!(!newer_text.contains("order-1 created"), "{newer_text}");
    let regex_text = read_all(&files_under(&data.join("regex")));
    assert_eq!(files_under(&data.join("regex")).len(), 2);
    assert!(!regex_text.contains("payment ok"), "{regex_text}");
}

#[test]
fn export_without_an_archive_fails_and_creates_nothing() {
    let root = tempfile::tempdir().unwrap();

    let output = eh(root.path(), &["export", "-b", "missing"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_of(&output).contains("No archive in"),
        "{}",
        stderr_of(&output)
    );
    assert!(stderr_of(&output).contains("run `eh read`"));
    assert!(!root.path().join("missing").exists());
}

#[tokio::test]
async fn export_while_the_archive_is_in_use_fails_naming_the_lock() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join(".eh-data");
    seed_archive(&data).await;
    let held = MessageStore::open(&DataPaths::new(&data)).unwrap();

    let output = eh(root.path(), &["export"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_of(&output).contains("in use by another eh or eh-gui"),
        "{}",
        stderr_of(&output)
    );
    held.close().await.unwrap();
}

#[tokio::test]
async fn save_config_stores_the_secret_apart_and_a_later_run_recalls_the_target() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join(".eh-data");
    seed_archive(&data).await;
    let held = MessageStore::open(&DataPaths::new(&data)).unwrap();
    let connection_string = connection_string();

    let saving = eh(
        root.path(),
        &[
            "--app-header",
            "read",
            "-s",
            &connection_string,
            "--namespace",
            NAMESPACE,
            "-e",
            HUB,
            "--save-config",
        ],
    );
    let recalling = eh(root.path(), &["--app-header", "read"]);
    held.close().await.unwrap();

    for output in [&saving, &recalling] {
        assert_eq!(output.status.code(), Some(1), "{}", stderr_of(output));
        assert!(
            stderr_of(output).contains("in use by another eh or eh-gui"),
            "{}",
            stderr_of(output)
        );
        assert!(!stdout_of(output).contains(SECRET_KEY));
        assert!(!stderr_of(output).contains(SECRET_KEY));
    }
    let saved = std::fs::read_to_string(root.path().join("config.json")).unwrap();
    assert!(!saved.contains(SECRET_KEY), "{saved}");
    assert!(saved.contains("\"stored\": \"local-db\""), "{saved}");
    assert!(data.join("secrets.redb").exists());
    assert!(
        stderr_of(&saving).contains("the connection string is stored in"),
        "{}",
        stderr_of(&saving)
    );
    let header = stdout_of(&recalling);
    assert!(
        header.contains(&format!(
            "  - Target: namespace {NAMESPACE}, event hub {HUB}"
        )),
        "{header}"
    );
    assert!(header.contains("  - Auth: connection string"), "{header}");
    assert!(header.contains("  - config.json: used"), "{header}");
    assert!(
        stderr_of(&recalling).contains("Using config.json:"),
        "{}",
        stderr_of(&recalling)
    );
}

#[test]
fn an_invalid_eh_config_json_fails_without_echoing_what_it_holds() {
    let root = tempfile::tempdir().unwrap();
    let pasted = connection_string();
    std::fs::write(
        root.path().join("config.json"),
        format!(
            r#"{{"tool":"eh","schema_version":1,"target":{{"auth":{{"method":"{pasted}"}}}}}}"#
        ),
    )
    .unwrap();

    let output = eh(root.path(), &["export"]);

    assert_eq!(output.status.code(), Some(1));
    let stderr = stderr_of(&output);
    assert!(
        stderr.contains("is an eh config, but it is not valid"),
        "{stderr}"
    );
    assert!(!stderr.contains(SECRET_KEY), "{stderr}");
    assert!(!root.path().join(".eh-data").exists());
}

#[test]
fn a_connection_string_variable_that_is_not_unicode_fails_naming_it() {
    let root = tempfile::tempdir().unwrap();

    let output = Command::new(EXE)
        .args(["read"])
        .current_dir(root.path())
        .env("EH_CONNECTION_STRING", not_unicode())
        .env_remove("EH_CLIENT_SECRET")
        .output()
        .expect("failed to run eh");

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_of(&output).contains("EH_CONNECTION_STRING is set but is not valid Unicode"),
        "{}",
        stderr_of(&output)
    );
}

#[cfg(windows)]
fn not_unicode() -> std::ffi::OsString {
    use std::os::windows::ffi::OsStringExt;

    std::ffi::OsString::from_wide(&[0xD800])
}

#[cfg(unix)]
fn not_unicode() -> std::ffi::OsString {
    use std::os::unix::ffi::OsStringExt;

    std::ffi::OsString::from_vec(vec![0xff])
}

#[test]
fn a_foreign_config_json_is_ignored_with_a_notice_and_never_overwritten() {
    let root = tempfile::tempdir().unwrap();
    let foreign = "{\"tool\": \"other\", \"port\": 8080}";
    std::fs::write(root.path().join("config.json"), foreign).unwrap();

    let reading = eh(root.path(), &["read"]);
    let saving = eh(
        root.path(),
        &["read", "-s", &connection_string(), "--save-config"],
    );

    assert_eq!(reading.status.code(), Some(1));
    assert!(
        stderr_of(&reading).contains("config.json here is not an eh config; ignored"),
        "{}",
        stderr_of(&reading)
    );
    assert!(stderr_of(&reading).contains("No event hub to read"));
    assert_eq!(saving.status.code(), Some(1));
    assert!(
        stderr_of(&saving).contains("belongs to another tool"),
        "{}",
        stderr_of(&saving)
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join("config.json")).unwrap(),
        foreign
    );
}

/// Ctrl-C delivered to a running `eh export` as a real SIGINT. Unix only:
/// sending Ctrl-C to a child process on Windows takes the
/// `GenerateConsoleCtrlEvent` system call, which needs unsafe FFI.
#[cfg(unix)]
mod interrupt {
    use super::*;
    use std::io::Read;
    use std::process::{ChildStdout, Stdio};

    const PARTITIONS: i64 = 200;
    const PER_PARTITION: i64 = 100;
    const SEEDED: u64 = 20_000;

    /// Archives `SEEDED` messages, one store task per partition so the
    /// archive's writer can group their commits.
    async fn seed_many(data: &Path) {
        let store = MessageStore::open(&DataPaths::new(data)).unwrap();
        store
            .bind(&EventHubIdentity {
                fully_qualified_namespace: NAMESPACE.to_owned(),
                event_hub_name: HUB.to_owned(),
            })
            .await
            .unwrap();
        let enqueued = Utc::now() - Duration::hours(1);
        let mut tasks = tokio::task::JoinSet::new();
        for partition in 0..PARTITIONS {
            let store = store.clone();
            tasks.spawn(async move {
                for sequence_number in 1..=PER_PARTITION {
                    let mut message = message(sequence_number, enqueued, "body");
                    message.key.partition_id = partition.to_string();
                    let checkpoint = Checkpoint {
                        partition_id: partition.to_string(),
                        sequence_number,
                        offset: None,
                        enqueued_time: Some(enqueued),
                        updated_at: Utc::now(),
                    };
                    store.store(&message, &checkpoint, true).await.unwrap();
                }
            });
        }
        while let Some(joined) = tasks.join_next().await {
            joined.unwrap();
        }
        store.close().await.unwrap();
    }

    /// Reads `stdout` until `needle` has appeared, returning everything read.
    fn read_until(stdout: &mut ChildStdout, needle: &str) -> Vec<u8> {
        let mut seen = Vec::new();
        let mut buffer = [0_u8; 4096];
        while !String::from_utf8_lossy(&seen).contains(needle) {
            let read = stdout.read(&mut buffer).unwrap();
            assert_ne!(read, 0, "eh closed stdout before printing {needle:?}");
            seen.extend_from_slice(&buffer[..read]);
        }
        seen
    }

    /// The exported count from the summary line
    /// `Exported: N | Skipped (already exported): ...`.
    fn summary_exported_count(stdout: &str) -> u64 {
        let line = stdout
            .lines()
            .find(|line| line.contains("| Skipped (already exported):"))
            .expect("the export summary line");
        line.trim_start_matches("Exported: ")
            .split(' ')
            .next()
            .unwrap()
            .parse()
            .unwrap()
    }

    #[tokio::test]
    async fn ctrl_c_stops_an_export_between_messages_and_exits_130() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join(".eh-data");
        seed_many(&data).await;
        let mut child = Command::new(EXE)
            .args(["export"])
            .current_dir(root.path())
            .env_remove("EH_CONNECTION_STRING")
            .env_remove("EH_CLIENT_SECRET")
            .env_remove("RUST_LOG")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdout = child.stdout.take().unwrap();
        let mut seen = read_until(&mut stdout, "Exported: ");

        let signalled = Command::new("kill")
            .args(["-INT", &child.id().to_string()])
            .status()
            .unwrap();
        stdout.read_to_end(&mut seen).unwrap();
        let status = child.wait().unwrap();

        assert!(signalled.success());
        let stdout = String::from_utf8_lossy(&seen).into_owned();
        assert_eq!(status.code(), Some(130), "{stdout}");
        assert!(
            stdout.contains("Cancelled; every written file is complete."),
            "{stdout}"
        );
        let exported = summary_exported_count(&stdout);
        assert!(exported < SEEDED, "the export ran to the end: {stdout}");
        let written = files_under(&data.join("exports"));
        assert_eq!(written.len() as u64, exported);
    }
}

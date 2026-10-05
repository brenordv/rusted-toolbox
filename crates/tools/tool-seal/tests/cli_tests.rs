use shared_crypto::errors::{
    MSG_CORRUPTED, MSG_NO_MATCHING_IDENTITY, MSG_NO_RECIPIENTS, MSG_PASSPHRASE_REFUSED,
};
use shared_crypto::keys::{generate_identity, load_identities, public_key_of, save_identity_file};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const EXE: &str = env!("CARGO_BIN_EXE_seal");

/// An armored, passphrase-encrypted (scrypt) age file; seal must refuse it
/// with the shared passphrase wording instead of attempting key decryption.
const PASSPHRASE_FIXTURE: &str = "-----BEGIN AGE ENCRYPTED FILE-----
YWdlLWVuY3J5cHRpb24ub3JnL3YxCi0+IHNjcnlwdCBpckNobUpjVytTSXA5RzZD
blBVQVNBIDIKbjY2UUhIaDgxSFBUaG1LODdqMUMxNVBhNzdiVCtIbEp3SzZkbE1P
OTB3cwotLS0gVUpyOG5yQmJKUXBmNTVHUHNjQ1pLRDVOSjhhTUFETi9POE16UHBl
dzBFRQo1vRm4dvYl+6UsoqrFf3viXkKqt5Tjr46bijEDnHehgJU4qwywmU4P/hvT
K1nPtvZv1tfOhPEtCjVY
-----END AGE ENCRYPTED FILE-----
";

fn run_tool(args: &[&str]) -> Output {
    Command::new(EXE)
        .args(args)
        .output()
        .expect("failed to spawn seal")
}

fn run_tool_with_stdin(args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(EXE)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn seal");
    child
        .stdin
        .take()
        .expect("stdin was piped")
        .write_all(input)
        .expect("failed to feed stdin");
    child.wait_with_output().expect("failed to wait for seal")
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn stdout_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Writes an identity file plus its public key for fixture purposes, through
/// the shared-crypto API rather than the binary under test.
fn fixture_identity(dir: &Path, name: &str) -> (PathBuf, String) {
    let identity = generate_identity();
    let path = dir.join(name);
    save_identity_file(&identity, &path, false).unwrap();
    (path, public_key_of(&identity))
}

fn entry_count(dir: &Path) -> usize {
    std::fs::read_dir(dir).unwrap().count()
}

#[test]
fn keygen_writes_a_loadable_identity_and_prints_only_the_public_key() {
    let dir = tempfile::tempdir().unwrap();
    let key_path = dir.path().join("key.txt");

    let output = run_tool(&["keygen", "-o", key_path.to_str().unwrap()]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr_of(&output));
    let stdout = stdout_of(&output);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 1, "stdout: {stdout}");
    assert!(lines[0].starts_with("age1"), "stdout: {stdout}");

    let loaded = load_identities(&[key_path]).unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(public_key_of(&loaded[0]), lines[0]);
}

#[test]
fn keygen_refuses_an_existing_file_without_force_and_replaces_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let key_path = dir.path().join("key.txt");
    let key = key_path.to_str().unwrap();
    run_tool(&["keygen", "-o", key]);
    let original = std::fs::read_to_string(&key_path).unwrap();

    let refused = run_tool(&["keygen", "-o", key]);
    assert_eq!(refused.status.code(), Some(1));
    assert!(stderr_of(&refused).contains("--force"));
    assert_eq!(std::fs::read_to_string(&key_path).unwrap(), original);

    let forced = run_tool(&["keygen", "-o", key, "--force"]);
    assert_eq!(forced.status.code(), Some(0));
    assert_ne!(std::fs::read_to_string(&key_path).unwrap(), original);
}

#[test]
fn keygen_without_output_prints_the_identity_file_and_a_stderr_notice() {
    let output = run_tool(&["keygen"]);

    assert_eq!(output.status.code(), Some(0));
    let stdout = stdout_of(&output);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 3, "stdout: {stdout}");
    assert!(lines[0].starts_with("# created: "));
    assert!(lines[1].starts_with("# public key: age1"));
    assert!(lines[2].starts_with("AGE-SECRET-KEY-1"));
    assert!(stderr_of(&output).contains("prefer -o"));
}

#[test]
fn file_round_trip_restores_text_and_binary_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let (key_path, public_key) = fixture_identity(dir.path(), "key.txt");
    let text: &[u8] = b"plain text payload\nwith two lines\n";
    let binary: Vec<u8> = (0..=255u8).cycle().take(70_000).collect();

    for (name, payload) in [("note.txt", text), ("blob.bin", &binary)] {
        let input = dir.path().join(name);
        std::fs::write(&input, payload).unwrap();
        let sealed = dir.path().join(format!("{name}.sealed"));
        let opened = dir.path().join(format!("{name}.opened"));

        let encrypt = run_tool(&[
            "encrypt",
            "-r",
            &public_key,
            "-o",
            sealed.to_str().unwrap(),
            input.to_str().unwrap(),
        ]);
        assert_eq!(encrypt.status.code(), Some(0), "{}", stderr_of(&encrypt));
        assert!(stdout_of(&encrypt).contains("sealed"));

        let decrypt = run_tool(&[
            "decrypt",
            "-i",
            key_path.to_str().unwrap(),
            "-o",
            opened.to_str().unwrap(),
            sealed.to_str().unwrap(),
        ]);
        assert_eq!(decrypt.status.code(), Some(0), "{}", stderr_of(&decrypt));
        assert!(stdout_of(&decrypt).contains("opened"));

        assert_eq!(std::fs::read(&opened).unwrap(), payload);
    }
}

#[test]
fn piped_round_trip_streams_armored_stdout_back_to_plaintext() {
    let dir = tempfile::tempdir().unwrap();
    let (key_path, public_key) = fixture_identity(dir.path(), "key.txt");
    let payload = b"streamed through two pipes";

    let encrypt = run_tool_with_stdin(&["encrypt", "-r", &public_key, "--armor"], payload);
    assert_eq!(encrypt.status.code(), Some(0), "{}", stderr_of(&encrypt));
    let armored = stdout_of(&encrypt);
    assert!(armored.starts_with("-----BEGIN AGE ENCRYPTED FILE-----"));
    assert!(stderr_of(&encrypt).contains("sealed stdin -> stdout"));

    let decrypt = run_tool_with_stdin(
        &["decrypt", "-i", key_path.to_str().unwrap()],
        armored.as_bytes(),
    );
    assert_eq!(decrypt.status.code(), Some(0), "{}", stderr_of(&decrypt));
    assert_eq!(decrypt.stdout, payload);
    assert!(stderr_of(&decrypt).contains("opened stdin -> stdout"));
}

#[test]
fn default_naming_appends_age_and_strips_it_back() {
    let dir = tempfile::tempdir().unwrap();
    let (key_path, public_key) = fixture_identity(dir.path(), "key.txt");
    let input = dir.path().join("file.txt");
    std::fs::write(&input, b"named by the default rule").unwrap();

    let encrypt = run_tool(&["encrypt", "-r", &public_key, input.to_str().unwrap()]);
    assert_eq!(encrypt.status.code(), Some(0), "{}", stderr_of(&encrypt));
    let sealed = dir.path().join("file.txt.age");
    assert!(sealed.exists());

    std::fs::remove_file(&input).unwrap();

    let decrypt = run_tool(&[
        "decrypt",
        "-i",
        key_path.to_str().unwrap(),
        sealed.to_str().unwrap(),
    ]);
    assert_eq!(decrypt.status.code(), Some(0), "{}", stderr_of(&decrypt));
    assert_eq!(std::fs::read(&input).unwrap(), b"named by the default rule");
}

#[test]
fn decrypt_without_an_age_suffix_asks_for_an_explicit_output() {
    let dir = tempfile::tempdir().unwrap();
    let (key_path, _) = fixture_identity(dir.path(), "key.txt");
    let odd_name = dir.path().join("file.enc");
    std::fs::write(&odd_name, b"irrelevant").unwrap();

    let output = run_tool(&[
        "decrypt",
        "-i",
        key_path.to_str().unwrap(),
        odd_name.to_str().unwrap(),
    ]);

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("-o <output>"));
}

#[test]
fn existing_output_is_refused_without_force_and_replaced_atomically_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let (_, public_key) = fixture_identity(dir.path(), "key.txt");
    let input = dir.path().join("file.txt");
    std::fs::write(&input, b"payload").unwrap();
    let sealed = dir.path().join("file.txt.age");

    let first = run_tool(&["encrypt", "-r", &public_key, input.to_str().unwrap()]);
    assert_eq!(first.status.code(), Some(0), "{}", stderr_of(&first));
    let first_bytes = std::fs::read(&sealed).unwrap();

    let refused = run_tool(&["encrypt", "-r", &public_key, input.to_str().unwrap()]);
    assert_eq!(refused.status.code(), Some(1));
    assert!(stderr_of(&refused).contains("--force"));
    assert_eq!(std::fs::read(&sealed).unwrap(), first_bytes);

    let forced = run_tool(&[
        "encrypt",
        "-r",
        &public_key,
        "--force",
        input.to_str().unwrap(),
    ]);
    assert_eq!(forced.status.code(), Some(0), "{}", stderr_of(&forced));
    assert_ne!(std::fs::read(&sealed).unwrap(), first_bytes);

    assert_eq!(
        entry_count(dir.path()),
        3,
        "only the key, input, and output may remain; no temp residue"
    );
}

#[test]
fn a_wrong_identity_reports_the_no_matching_identity_wording() {
    let dir = tempfile::tempdir().unwrap();
    let (_, public_key) = fixture_identity(dir.path(), "right.txt");
    let (wrong_key_path, _) = fixture_identity(dir.path(), "wrong.txt");
    let input = dir.path().join("file.txt");
    std::fs::write(&input, b"sealed away").unwrap();
    run_tool(&["encrypt", "-r", &public_key, input.to_str().unwrap()]);

    let output = run_tool(&[
        "decrypt",
        "-i",
        wrong_key_path.to_str().unwrap(),
        "-o",
        dir.path().join("out.txt").to_str().unwrap(),
        dir.path().join("file.txt.age").to_str().unwrap(),
    ]);

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains(MSG_NO_MATCHING_IDENTITY));
    assert!(!dir.path().join("out.txt").exists());
}

#[test]
fn a_tampered_file_reports_corruption_and_leaves_no_partial_output() {
    let dir = tempfile::tempdir().unwrap();
    let (key_path, public_key) = fixture_identity(dir.path(), "key.txt");
    let input = dir.path().join("file.txt");
    std::fs::write(&input, vec![0x5Au8; 100_000]).unwrap();
    run_tool(&["encrypt", "-r", &public_key, input.to_str().unwrap()]);

    let sealed = dir.path().join("file.txt.age");
    let mut bytes = std::fs::read(&sealed).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0x01;
    std::fs::write(&sealed, bytes).unwrap();

    let entries_before = entry_count(dir.path());
    let output = run_tool(&[
        "decrypt",
        "-i",
        key_path.to_str().unwrap(),
        "-o",
        dir.path().join("out.txt").to_str().unwrap(),
        sealed.to_str().unwrap(),
    ]);

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains(MSG_CORRUPTED));
    assert!(!dir.path().join("out.txt").exists());
    assert_eq!(
        entry_count(dir.path()),
        entries_before,
        "a failed decrypt must not leave an output or temp file behind"
    );
}

#[test]
fn a_passphrase_encrypted_file_is_refused_with_the_shared_wording() {
    let dir = tempfile::tempdir().unwrap();
    let (key_path, _) = fixture_identity(dir.path(), "key.txt");
    let sealed = dir.path().join("locked.age");
    std::fs::write(&sealed, PASSPHRASE_FIXTURE).unwrap();

    let output = run_tool(&[
        "decrypt",
        "-i",
        key_path.to_str().unwrap(),
        sealed.to_str().unwrap(),
    ]);

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains(MSG_PASSPHRASE_REFUSED));
    assert!(!dir.path().join("locked").exists());
}

#[test]
fn encrypt_without_recipients_gets_the_guidance_and_decrypt_without_identities_is_a_usage_error() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("file.txt");
    std::fs::write(&input, b"payload").unwrap();

    let no_recipients = run_tool(&["encrypt", input.to_str().unwrap()]);
    assert_eq!(no_recipients.status.code(), Some(1));
    assert!(stderr_of(&no_recipients).contains(MSG_NO_RECIPIENTS));

    let no_identities = run_tool(&["decrypt", "in.age"]);
    assert_eq!(no_identities.status.code(), Some(2));
}

#[test]
fn recipients_file_encrypts_and_explicit_dash_output_streams_to_stdout() {
    let dir = tempfile::tempdir().unwrap();
    let (key_path, public_key) = fixture_identity(dir.path(), "key.txt");
    let team = dir.path().join("team.txt");
    std::fs::write(&team, format!("# the team\n{public_key}\n")).unwrap();
    let input = dir.path().join("file.txt");
    std::fs::write(&input, b"via recipients file").unwrap();

    let encrypt = run_tool(&[
        "encrypt",
        "-R",
        team.to_str().unwrap(),
        input.to_str().unwrap(),
    ]);
    assert_eq!(encrypt.status.code(), Some(0), "{}", stderr_of(&encrypt));

    let decrypt = run_tool(&[
        "decrypt",
        "-i",
        key_path.to_str().unwrap(),
        "-o",
        "-",
        dir.path().join("file.txt.age").to_str().unwrap(),
    ]);
    assert_eq!(decrypt.status.code(), Some(0), "{}", stderr_of(&decrypt));
    assert_eq!(decrypt.stdout, b"via recipients file");
    assert!(stderr_of(&decrypt).contains("-> stdout"));
}

#[test]
fn watch_without_required_directories_is_a_usage_error() {
    let output = run_tool(&["watch"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(stderr_of(&output).contains("--watch-dir"));
}

#[test]
fn watch_refuses_a_missing_watch_dir() {
    let dir = tempfile::tempdir().unwrap();
    let (_key_path, public_key) = fixture_identity(dir.path(), "key.txt");
    let safe = dir.path().join("safe");
    std::fs::create_dir(&safe).unwrap();

    let output = run_tool(&[
        "watch",
        "-r",
        &public_key,
        "--watch-dir",
        dir.path().join("absent").to_str().unwrap(),
        "--safe-dir",
        safe.to_str().unwrap(),
    ]);

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("watch folder"));
}

#[test]
fn watch_refuses_a_safe_dir_inside_the_watch_dir() {
    let dir = tempfile::tempdir().unwrap();
    let (_key_path, public_key) = fixture_identity(dir.path(), "key.txt");
    let watch = dir.path().join("watch");
    let safe = watch.join("safe");
    std::fs::create_dir_all(&safe).unwrap();

    let output = run_tool(&[
        "watch",
        "-r",
        &public_key,
        "--watch-dir",
        watch.to_str().unwrap(),
        "--safe-dir",
        safe.to_str().unwrap(),
    ]);

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("inside the watch folder"));
}

#[test]
fn bare_invocation_shows_help_with_exit_two_and_help_exits_zero() {
    let bare = run_tool(&[]);
    assert_eq!(bare.status.code(), Some(2));
    assert!(stderr_of(&bare).contains("Usage"));

    let help = run_tool(&["--help"]);
    assert_eq!(help.status.code(), Some(0));
    assert!(!help.stdout.is_empty());
}

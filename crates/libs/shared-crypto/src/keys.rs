//! Identity and recipient handling: generation, the standard age identity
//! file (save and load), and recipient parsing from literals and files.
//! Secret key material only ever exists as `SecretString`; its string form
//! leaves that wrapper exactly once, inside the identity-file write.

use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use age::secrecy::{ExposeSecret, SecretString};
use age::x25519::{Identity, Recipient};
use anyhow::{Context, anyhow, bail};
use common_file_utils::permissions::{create_dir_all_owner_only, open_owner_only};

use crate::errors::{
    MSG_NO_RECIPIENTS, identity_exists, identity_file_malformed, identity_file_unreadable,
    invalid_recipient_in_file, invalid_recipient_literal, recipient_file_unreadable,
};

/// Generates a fresh X25519 identity. Thin passthrough over the age crate so
/// consumers never import `age` directly.
pub fn generate_identity() -> Identity {
    Identity::generate()
}

/// The identity's public key in its `age1...` string form.
pub fn public_key_of(identity: &Identity) -> String {
    identity.to_public().to_string()
}

/// Writes `identity` as a standard age identity file: a `# created:` RFC3339
/// timestamp, the `# public key:` line, then the secret key. On Unix the file
/// is owner-only (0o600) and any created parent directory 0o700; on Windows
/// both inherit the parent's ACL.
pub fn save_identity_file(identity: &Identity, path: &Path, overwrite: bool) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        create_dir_all_owner_only(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let mut options = OpenOptions::new();
    options.write(true);

    if overwrite {
        options.create(true).truncate(true);
    } else {
        // Atomic existence guard: the open itself fails if the target already
        // exists, with no check-then-write window.
        options.create_new(true);
    }

    let mut file = open_owner_only(&mut options, path).map_err(|error| {
        if error.kind() == ErrorKind::AlreadyExists {
            anyhow!(identity_exists(path))
        } else {
            anyhow::Error::new(error).context(format!("failed to create {}", path.display()))
        }
    })?;

    let created = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let public_key = identity.to_public().to_string();

    write_identity_lines(&mut file, &created, &public_key, &identity.to_string())
        .with_context(|| format!("failed to write {}", path.display()))?;

    Ok(())
}

/// Writes the three identity-file lines. The secret is written straight from
/// its `SecretString`; no intermediate `String` ever holds it.
fn write_identity_lines(
    file: &mut File,
    created: &str,
    public_key: &str,
    secret_key: &SecretString,
) -> io::Result<()> {
    writeln!(file, "# created: {created}")?;
    writeln!(file, "# public key: {public_key}")?;
    writeln!(file, "{}", secret_key.expose_secret())
}

/// Parses each file as an age identity file: blank lines and `#` comments are
/// ignored, every remaining line must parse as an identity. A malformed line
/// is reported by file and 1-based line number only, never by content.
pub fn load_identities(paths: &[PathBuf]) -> anyhow::Result<Vec<Identity>> {
    let mut identities = Vec::new();

    for path in paths {
        let content = fs::read_to_string(path).with_context(|| identity_file_unreadable(path))?;

        for (line_no, line) in content.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let identity = Identity::from_str(line)
                .map_err(|_| anyhow!(identity_file_malformed(path, line_no + 1)))?;
            identities.push(identity);
        }
    }

    Ok(identities)
}

/// Parses recipients from `age1...` literals and/or files with one recipient
/// per line (blank lines and `#` comments ignored), literals first. A
/// literal that fails to parse is reported by argument position, a file line
/// by path and 1-based line number. An empty combined result is an error
/// carrying the no-recipients guidance, never an empty vec.
pub fn parse_recipients(literals: &[String], files: &[PathBuf]) -> anyhow::Result<Vec<Recipient>> {
    let mut recipients = Vec::new();

    for (i, lit) in literals.iter().enumerate() {
        let r = Recipient::from_str(lit).map_err(|_| anyhow!(invalid_recipient_literal(i)))?;
        recipients.push(r);
    }

    for path in files {
        let content = fs::read_to_string(path).with_context(|| recipient_file_unreadable(path))?;

        for (line_no, line) in content.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let r = Recipient::from_str(line)
                .map_err(|_| anyhow!(invalid_recipient_in_file(path, line_no + 1)))?;
            recipients.push(r);
        }
    }

    if recipients.is_empty() {
        bail!(MSG_NO_RECIPIENTS);
    }

    Ok(recipients)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered(err: &anyhow::Error) -> String {
        format!("{err:#}")
    }

    fn secret_line(identity: &Identity) -> String {
        identity.to_string().expose_secret().to_owned()
    }

    /// `unwrap_err` needs `Vec<Identity>: Debug`, and `Identity` has no Debug
    /// impl (secret hygiene), so the failure is extracted by match instead.
    fn load_identities_err(paths: &[PathBuf]) -> anyhow::Error {
        match load_identities(paths) {
            Ok(_) => panic!("expected load_identities to fail"),
            Err(err) => err,
        }
    }

    #[test]
    fn save_then_load_round_trips_through_nested_directories() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keys").join("nested").join("id.txt");
        let identity = generate_identity();

        save_identity_file(&identity, &path, false).unwrap();
        let loaded = load_identities(&[path]).unwrap();

        assert_eq!(loaded.len(), 1);
        assert_eq!(public_key_of(&loaded[0]), public_key_of(&identity));
    }

    #[test]
    fn saved_file_uses_the_standard_age_identity_format() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("id.txt");
        let identity = generate_identity();

        save_identity_file(&identity, &path, false).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 3);

        let created = lines[0].strip_prefix("# created: ").unwrap();
        assert!(chrono::DateTime::parse_from_rfc3339(created).is_ok());
        assert_eq!(
            lines[1],
            format!("# public key: {}", public_key_of(&identity))
        );
        assert!(lines[2].starts_with("AGE-SECRET-KEY-1"));
    }

    #[test]
    fn save_without_overwrite_refuses_an_existing_file_and_keeps_its_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("id.txt");
        let first = generate_identity();
        save_identity_file(&first, &path, false).unwrap();
        let before = fs::read_to_string(&path).unwrap();

        let err = save_identity_file(&generate_identity(), &path, false).unwrap_err();

        assert!(
            rendered(&err).contains("--force"),
            "chain: {}",
            rendered(&err)
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), before);
    }

    #[test]
    fn save_with_overwrite_replaces_an_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("id.txt");
        save_identity_file(&generate_identity(), &path, false).unwrap();
        let replacement = generate_identity();

        save_identity_file(&replacement, &path, true).unwrap();

        let loaded = load_identities(&[path]).unwrap();
        assert_eq!(public_key_of(&loaded[0]), public_key_of(&replacement));
    }

    #[test]
    fn load_skips_comments_and_blank_lines_across_files() {
        let dir = tempfile::tempdir().unwrap();
        let first = generate_identity();
        let second = generate_identity();
        let path_a = dir.path().join("a.txt");
        let path_b = dir.path().join("b.txt");
        fs::write(
            &path_a,
            format!("# a comment\n\n   \n{}\n", secret_line(&first)),
        )
        .unwrap();
        fs::write(&path_b, format!("{}\n# trailing\n", secret_line(&second))).unwrap();

        let loaded = load_identities(&[path_a, path_b]).unwrap();

        let keys: Vec<String> = loaded.iter().map(public_key_of).collect();
        assert_eq!(keys, [public_key_of(&first), public_key_of(&second)]);
    }

    #[test]
    fn malformed_identity_line_is_reported_by_number_without_its_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("id.txt");
        fs::write(&path, "# comment\n\nAGE-SECRET-KEY-MISTYPED\n").unwrap();

        let err = load_identities_err(std::slice::from_ref(&path));
        let chain = rendered(&err);

        assert!(chain.contains("line 3"), "chain: {chain}");
        assert!(chain.contains(&path.display().to_string()));
        assert!(!chain.contains("MISTYPED"), "chain must not echo the line");
    }

    #[test]
    fn unreadable_identity_file_reports_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("absent.txt");

        let err = load_identities_err(std::slice::from_ref(&path));
        let chain = rendered(&err);

        assert!(chain.contains("could not read identity file"));
        assert!(chain.contains(&path.display().to_string()));
    }

    #[test]
    fn recipients_merge_literals_first_then_file_lines() {
        let dir = tempfile::tempdir().unwrap();
        let ids: Vec<Identity> = (0..3).map(|_| generate_identity()).collect();
        let keys: Vec<String> = ids.iter().map(public_key_of).collect();
        let file = dir.path().join("recipients.txt");
        fs::write(&file, format!("# team\n{}\n\n{}\n", keys[1], keys[2])).unwrap();

        let parsed = parse_recipients(&keys[..1], &[file]).unwrap();

        let parsed_keys: Vec<String> = parsed.iter().map(|r| r.to_string()).collect();
        assert_eq!(parsed_keys, keys);
    }

    #[test]
    fn a_bad_literal_is_reported_by_argument_position() {
        let literals = vec![
            public_key_of(&generate_identity()),
            "age1-definitely-not-a-key".to_owned(),
        ];

        let err = parse_recipients(&literals, &[]).unwrap_err();

        assert!(
            rendered(&err).contains("recipient argument 2"),
            "chain: {}",
            rendered(&err)
        );
    }

    #[test]
    fn a_bad_recipient_file_line_is_reported_by_path_and_number() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("recipients.txt");
        fs::write(
            &file,
            format!("{}\nnot-a-recipient\n", public_key_of(&generate_identity())),
        )
        .unwrap();

        let err = parse_recipients(&[], std::slice::from_ref(&file)).unwrap_err();
        let chain = rendered(&err);

        assert!(chain.contains("line 2"), "chain: {chain}");
        assert!(chain.contains(&file.display().to_string()));
    }

    #[test]
    fn unreadable_recipient_file_reports_the_path() {
        let file = PathBuf::from("recipients-that-do-not-exist.txt");

        let err = parse_recipients(&[], &[file]).unwrap_err();

        assert!(rendered(&err).contains("could not read recipients file"));
    }

    #[test]
    fn no_recipients_at_all_gets_the_guidance_message() {
        let err = parse_recipients(&[], &[]).unwrap_err();
        assert!(rendered(&err).contains(MSG_NO_RECIPIENTS));
    }

    #[test]
    fn secret_debug_rendering_is_redacted() {
        let identity = generate_identity();
        let secret = identity.to_string();

        assert!(!format!("{secret:?}").contains("AGE-SECRET-KEY"));
        assert!(secret.expose_secret().starts_with("AGE-SECRET-KEY-1"));
    }

    #[cfg(unix)]
    mod unix_permissions {
        use super::*;
        use std::os::unix::fs::PermissionsExt;

        #[test]
        fn saved_identity_file_is_owner_only_and_created_parents_are_private() {
            let root = tempfile::tempdir().unwrap();
            let parent = root.path().join("keys");
            let path = parent.join("id.txt");

            save_identity_file(&generate_identity(), &path, false).unwrap();

            // The open ends in a handle-based set_permissions (fchmod), which
            // umask does not filter, so the file assertion is exact.
            let file_mode = fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(file_mode & 0o777, 0o600, "identity file must be 0o600");

            // mkdir(2) applies `mode & !umask`; assert only that group/other
            // bits are clear.
            let dir_mode = fs::metadata(&parent).unwrap().permissions().mode();
            assert_eq!(dir_mode & 0o077, 0, "created parent must be private");
        }
    }
}

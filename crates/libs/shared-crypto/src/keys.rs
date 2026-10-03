use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use age::secrecy::ExposeSecret;
use age::x25519::{Identity, Recipient};
use anyhow::{Context, anyhow, bail};
use common_file_utils::permissions::{create_dir_all_owner_only, open_owner_only};

use crate::errors::{
    MSG_NO_RECIPIENTS, identity_exists, identity_file_malformed, identity_file_unreadable,
    invalid_recipient_in_file, invalid_recipient_literal, recipient_file_unreadable,
};

pub fn generate_identity() -> Identity {
    Identity::generate()
}

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
    let secret_key = identity.to_string();

    writeln!(file, "# created: {created}")?;
    writeln!(file, "# public key: {public_key}")?;
    writeln!(file, "{}", secret_key.expose_secret())?;

    Ok(())
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

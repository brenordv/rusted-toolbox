//! The data folder's secrets file: a small redb database mapping a secret's
//! name to its value. It keeps connection strings and client secrets out of
//! `config.json` and out of git; it is not encrypted.

use crate::paths::DataPaths;
use crate::store::open_database;
use anyhow::Context;
use common_utils::string_utils::SecretString;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::path::PathBuf;

const SECRETS: TableDefinition<&str, &str> = TableDefinition::new("secrets");

/// The name the event hub connection string is stored under.
pub const CONNECTION_STRING_SECRET: &str = "connection_string";

/// The name a service principal's client secret is stored under.
pub fn client_secret_name(client_id: &str) -> String {
    format!("client_secret:{client_id}")
}

/// An open secrets file. It is locked while open, so open it for one save or
/// one lookup and drop it.
pub struct SecretsStore {
    database: Database,
    path: PathBuf,
}

impl SecretsStore {
    /// Opens the data folder's secrets file, creating the folder and the file
    /// (owner-only on Unix) when missing.
    ///
    /// # Errors
    /// Fails when the folder or file cannot be created, or another process has
    /// the file open.
    pub fn open(paths: &DataPaths) -> anyhow::Result<Self> {
        paths.ensure_base()?;
        let path = paths.secrets_db();
        let database = open_database(&path, "secrets file")?;
        let transaction = database.begin_write().with_context(|| {
            format!("Failed to initialize the secrets file '{}'", path.display())
        })?;
        transaction.open_table(SECRETS).with_context(|| {
            format!("Failed to initialize the secrets file '{}'", path.display())
        })?;
        transaction.commit().with_context(|| {
            format!("Failed to initialize the secrets file '{}'", path.display())
        })?;

        Ok(Self { database, path })
    }

    /// The secret stored under `name`, or `None`.
    ///
    /// # Errors
    /// Fails when the secrets file cannot be read.
    pub fn get(&self, name: &str) -> anyhow::Result<Option<SecretString>> {
        let read = self
            .database
            .begin_read()
            .with_context(|| self.read_failed())?;
        let secrets = read
            .open_table(SECRETS)
            .with_context(|| self.read_failed())?;
        let stored = secrets.get(name).with_context(|| self.read_failed())?;

        Ok(stored.map(|guard| SecretString::from(guard.value().to_owned())))
    }

    /// Stores `value` under `name`, replacing any previous value.
    ///
    /// # Errors
    /// Fails when the secrets file cannot be written.
    pub fn put(&self, name: &str, value: &SecretString) -> anyhow::Result<()> {
        self.write(|secrets| {
            secrets.insert(name, value.expose())?;
            Ok(())
        })
    }

    /// Removes the secret stored under `name`, if any. The freed bytes are not
    /// scrubbed from the file.
    ///
    /// # Errors
    /// Fails when the secrets file cannot be written.
    pub fn remove(&self, name: &str) -> anyhow::Result<()> {
        self.write(|secrets| {
            secrets.remove(name)?;
            Ok(())
        })
    }

    /// The names of every stored secret.
    pub(crate) fn names(&self) -> anyhow::Result<Vec<String>> {
        let read = self
            .database
            .begin_read()
            .with_context(|| self.read_failed())?;
        let secrets = read
            .open_table(SECRETS)
            .with_context(|| self.read_failed())?;
        let mut names = Vec::new();
        for entry in secrets.iter().with_context(|| self.read_failed())? {
            let (name, _) = entry.with_context(|| self.read_failed())?;
            names.push(name.value().to_owned());
        }
        Ok(names)
    }

    fn write(
        &self,
        change: impl FnOnce(&mut redb::Table<'_, &'static str, &'static str>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        let failed = || format!("Failed to write the secrets file '{}'", self.path.display());
        let transaction = self.database.begin_write().with_context(failed)?;
        {
            let mut secrets = transaction.open_table(SECRETS).with_context(failed)?;
            change(&mut secrets).with_context(failed)?;
        }
        transaction.commit().with_context(failed)
    }

    fn read_failed(&self) -> String {
        format!("Failed to read the secrets file '{}'", self.path.display())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open(root: &tempfile::TempDir) -> SecretsStore {
        SecretsStore::open(&DataPaths::new(root.path().join("data"))).unwrap()
    }

    #[test]
    fn a_stored_secret_reads_back_and_survives_a_reopen() {
        let root = tempfile::tempdir().unwrap();
        let secrets = open(&root);

        secrets
            .put(
                CONNECTION_STRING_SECRET,
                &"Endpoint=sb://x/".to_owned().into(),
            )
            .unwrap();
        drop(secrets);
        let reopened = open(&root);

        assert_eq!(
            reopened
                .get(CONNECTION_STRING_SECRET)
                .unwrap()
                .unwrap()
                .expose(),
            "Endpoint=sb://x/"
        );
    }

    #[test]
    fn a_missing_secret_is_none() {
        let root = tempfile::tempdir().unwrap();

        assert!(open(&root).get("nothing").unwrap().is_none());
    }

    #[test]
    fn a_removed_secret_is_gone_and_others_stay() {
        let root = tempfile::tempdir().unwrap();
        let secrets = open(&root);
        secrets.put("a", &"one".to_owned().into()).unwrap();
        secrets.put("b", &"two".to_owned().into()).unwrap();

        secrets.remove("a").unwrap();

        assert_eq!(secrets.names().unwrap(), vec!["b".to_owned()]);
    }

    #[test]
    fn a_second_open_names_the_secrets_file() {
        let root = tempfile::tempdir().unwrap();
        let _first = open(&root);

        let error = SecretsStore::open(&DataPaths::new(root.path().join("data")))
            .err()
            .unwrap();

        assert!(
            error.to_string().contains("secrets file") && error.to_string().contains("in use"),
            "{error:#}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_secrets_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().unwrap();
        drop(open(&root));

        let mode = std::fs::metadata(root.path().join("data").join("secrets.redb"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}

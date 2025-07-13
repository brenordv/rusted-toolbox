use crate::shared::system::get_current_working_dir::get_current_working_dir;
use rusqlite::{Connection, Result, Row, ToSql};
use std::path::PathBuf;

/// The generic database wrapper struct.
pub struct GenericDb {
    conn: Connection,
}

impl GenericDb {
    /// Initializes a new GenericDb instance.
    pub fn new(db_path: String) -> anyhow::Result<Self> {
        if db_path.is_empty() {
            anyhow::bail!("Database path cannot be empty");
        }

        let path = db_path.trim();

        let conn = if path == ":memory:" || path.eq_ignore_ascii_case("memory") {
            Connection::open_in_memory()?
        } else {
            let mut pathbuf = PathBuf::from(path);

            if !pathbuf.is_absolute() {
                let cwd = get_current_working_dir();
                pathbuf = cwd.join(pathbuf);
            }

            if pathbuf.is_file() {
                Connection::open(&pathbuf)?
            } else if pathbuf.is_dir() {
                let db_file = pathbuf.join("data.db");
                Connection::open(db_file)?
            } else {
                anyhow::bail!("Invalid database path: {}", path);
            }
        };

        Ok(GenericDb { conn })
    }

    /// SELECT operation: accepts a SQL query and a row-mapping closure.
    pub fn select<T, F>(
        &self,
        query: &str,
        params: &[&dyn ToSql],
        map_row: F,
    ) -> anyhow::Result<Vec<T>>
    where
        F: FnMut(&Row<'_>) -> Result<T>,
    {
        let mut stmt = self.conn.prepare(query)?;

        let results = stmt
            .query_map(params, map_row)?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(results)
    }

    pub fn execute(&self, query: &str, params: &[&dyn ToSql]) -> anyhow::Result<usize> {
        Ok(self.conn.execute(query, params)?)
    }

    pub fn last_insert_rowid(&self) -> i64 {
        self.conn.last_insert_rowid()
    }
}

// Example usage with a user struct.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::sqlite::database_entity_traits::FromRow;
    #[derive(Debug, Clone)]
    struct User {
        id: i32,
        name: String,
    }

    impl FromRow for User {
        // Helper for mapping rows to User struct
        fn from_row(row: &Row) -> Result<User> {
            Ok(User {
                id: row.get(0)?,
                name: row.get(1)?,
            })
        }
    }

    #[test]
    fn test_crud() {
        let db = GenericDb::new(":memory:".to_string()).unwrap();
        db.conn
            .execute(
                "CREATE TABLE user (id INTEGER PRIMARY KEY, name TEXT NOT NULL)",
                [],
            )
            .unwrap();

        let u = User {
            id: 1,
            name: "Alice".to_string(),
        };
        let inserted = db
            .execute(
                "INSERT INTO user (id, name) VALUES (?1, ?2)",
                &[&u.id, &u.name],
            )
            .unwrap();
        assert_eq!(inserted, 1);

        let users: Vec<User> = db
            .select("SELECT id, name FROM user", &[], User::from_row)
            .unwrap();
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].name, "Alice");

        let deleted = db.execute("DELETE FROM user WHERE id=?1", &[&1]).unwrap();
        assert_eq!(deleted, 1);
    }
}

#![allow(dead_code)]
use crate::sqlite::database_entity_traits::FromRow;
use crate::sqlite::generic_db::GenericDb;
use crate::utils::sanitize_string_for_table_name::sanitize_string_for_table_name;
use rusqlite::{Error, Row};

struct ListItem {
    id: i64,
    timestamp: String,
    item: String,
}

impl FromRow for ListItem {
    fn from_row(row: &Row) -> Result<Self, Error> {
        Ok(ListItem {
            id: row.get(0)?,
            timestamp: row.get(1)?,
            item: row.get(2)?,
        })
    }
}

pub struct ListDb {
    db: GenericDb,
}

impl ListDb {
    pub fn new(db: GenericDb) -> Self {
        ListDb { db }
    }

    pub fn create_list(&self, list_name: &str) -> anyhow::Result<()> {
        let table = sanitize_string_for_table_name(list_name)?;

        let sql = format!(
            "CREATE TABLE IF NOT EXISTS \"{}\" (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
                item TEXT NOT NULL
            )",
            table
        );

        self.db.execute(&sql, &[])?;

        Ok(())
    }

    pub fn add(&self, list_name: &str, item: &str) -> anyhow::Result<i64> {
        let table = sanitize_string_for_table_name(list_name)?;

        let sql = format!("INSERT INTO \"{}\" (item) VALUES (?1)", table);

        self.db.execute(&sql, &[&item])?;

        let id = self.db.last_insert_rowid();

        Ok(id)
    }

    pub fn update(&self, list_name: &str, id: i64, item: &str) -> anyhow::Result<String> {
        let table = sanitize_string_for_table_name(list_name)?;

        let sql = format!(
            "UPDATE \"{}\" SET item=?1, timestamp=CURRENT_TIMESTAMP WHERE id=?2",
            table
        );

        let updated = self.db.execute(&sql, &[&item, &id])?;

        if updated > 0 {
            Ok(item.to_string())
        } else {
            anyhow::bail!("No item found with id={}", id);
        }
    }

    pub fn delete(&self, list_name: &str, id: i64) -> anyhow::Result<bool> {
        let table = sanitize_string_for_table_name(list_name)?;

        let sql = format!("DELETE FROM \"{}\" WHERE id=?1", table);

        let affected_rows = self.db.execute(sql.as_str(), &[&id])?;

        Ok(affected_rows > 0)
    }

    pub fn get_latest(&self, list_name: &str) -> anyhow::Result<Option<String>> {
        let table = sanitize_string_for_table_name(list_name)?;

        let sql = format!(
            "SELECT id, timestamp, item FROM \"{}\" ORDER BY timestamp DESC, id DESC LIMIT 1",
            table
        );

        let rows = self.db.select(&sql, &[], ListItem::from_row)?;

        if rows.is_empty() {
            return Ok(None);
        }

        if let Some(row) = rows.first() {
            Ok(Some(row.item.clone()))
        } else {
            Ok(None)
        }
    }

    pub fn exists(&self, list_name: &str, item: &str) -> anyhow::Result<bool> {
        let table = sanitize_string_for_table_name(list_name)?;

        let sql = format!(
            "SELECT id, timestamp, item FROM \"{}\" WHERE item = ?1 LIMIT 1",
            table
        );

        let rows = self.db.select(&sql, &[&item], ListItem::from_row)?;

        Ok(!rows.is_empty())
    }
}

pub struct SingleListDb {
    db: ListDb,
    list_name: String,
}

impl SingleListDb {
    pub fn new(list_name: String, db: GenericDb) -> anyhow::Result<Self> {
        let list_db = ListDb::new(db);

        list_db.create_list(&list_name)?;

        Ok(SingleListDb {
            db: list_db,
            list_name,
        })
    }

    pub fn add(&self, item: &str) -> anyhow::Result<i64> {
        self.db.add(&self.list_name, item)
    }

    pub fn update(&self, id: i64, item: &str) -> anyhow::Result<String> {
        self.db.update(&self.list_name, id, item)
    }

    pub fn delete(&self, id: i64) -> anyhow::Result<bool> {
        self.db.delete(&self.list_name, id)
    }

    pub fn get_latest(&self) -> anyhow::Result<Option<String>> {
        self.db.get_latest(&self.list_name)
    }

    pub fn exists(&self, item: &str) -> anyhow::Result<bool> {
        self.db.exists(&self.list_name, item)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_db() {
        let db = GenericDb::new(":memory:".to_string()).unwrap();
        let list_db = ListDb::new(db);

        list_db.create_list("my_list").unwrap();

        let id1 = list_db.add("my_list", "foo").unwrap();
        let id2 = list_db.add("my_list", "bar").unwrap();

        assert!(list_db.exists("my_list", "foo").unwrap());
        assert!(!list_db.exists("my_list", "baz").unwrap());

        let latest = list_db.get_latest("my_list").unwrap();
        assert_eq!(latest, Some("bar".to_string()));

        let updated = list_db.update("my_list", id1, "new_foo").unwrap();
        assert_eq!(updated, "new_foo");

        let deleted = list_db.delete("my_list", id2).unwrap();
        assert!(deleted);
    }

    #[test]
    fn test_single_list_db() {
        let db = GenericDb::new(":memory:".to_string()).unwrap();
        let list_db = SingleListDb::new("my_list".to_string(), db).unwrap();

        let id1 = list_db.add("foo").unwrap();
        let id2 = list_db.add("bar").unwrap();

        assert!(list_db.exists("foo").unwrap());
        assert!(!list_db.exists("baz").unwrap());

        let latest = list_db.get_latest().unwrap();
        assert_eq!(latest, Some("bar".to_string()));

        let updated = list_db.update(id1, "new_foo").unwrap();
        assert_eq!(updated, "new_foo");

        let deleted = list_db.delete(id2).unwrap();
        assert!(deleted);
    }
}

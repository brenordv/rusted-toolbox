use crate::sqlite::generic_db::GenericDb;
use crate::system::ensure_directory_exists::EnsureDirectoryExists;
use crate::utils::sanitize_string_for_table_name::sanitize_string_for_table_name;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
pub struct DictionaryDbItem<T> {
    pub timestamp: String,
    pub key: String,
    pub value: T,
}

pub struct DictionaryDb {
    db: GenericDb,
    dict_name: String,
}

impl DictionaryDb {
    pub fn new(db_path: String, dict_name: String) -> anyhow::Result<Self> {
        let path = PathBuf::from(&db_path);
        path.ensure_parent_exists()?;

        let db = GenericDb::new(db_path)?;
        let dict_name = sanitize_string_for_table_name(&dict_name)?;
        Self::create_dictionary(&db, &dict_name)?;
        Ok(Self { db, dict_name })
    }

    fn create_dictionary(db: &GenericDb, dict_name: &str) -> anyhow::Result<()> {
        let sql = format!(
            "CREATE TABLE IF NOT EXISTS \"{}\" (
                key TEXT PRIMARY KEY,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
                value TEXT NOT NULL
            )",
            dict_name
        );

        db.execute(&sql, &[])?;

        Ok(())
    }

    pub fn add<T: Serialize + for<'de> Deserialize<'de>>(
        &self,
        key: &str,
        value: &T,
    ) -> anyhow::Result<DictionaryDbItem<T>> {
        let value_str = serde_json::to_string(&value)?;
        let sql = format!(
            "INSERT INTO \"{}\" (key, value) VALUES (?1, ?2)",
            self.dict_name
        );

        self.db.execute(&sql, &[&key, &value_str])?;

        match self.get(&key) {
            Ok(Some(got)) => Ok(got),
            Ok(None) => anyhow::bail!(
                "Key not found. This is an indicative of a bug go check it out: [{}]",
                &key
            ),
            Err(e) => Err(e),
        }
    }

    pub fn update<T: Serialize + for<'de> Deserialize<'de>>(
        &self,
        key: &str,
        item: &T,
    ) -> anyhow::Result<DictionaryDbItem<T>> {
        let value_str = serde_json::to_string(&item)?;

        let sql = format!(
            "UPDATE \"{}\" SET value=?1, timestamp=CURRENT_TIMESTAMP WHERE key=?2",
            self.dict_name
        );

        let updated = self.db.execute(&sql, &[&value_str, &key])?;

        if updated == 0 {
            anyhow::bail!("No key found: {}", &key);
        }

        match self.get(&key) {
            Ok(Some(got)) => Ok(got),
            Ok(None) => anyhow::bail!(
                "Key not found. This is an indicative of a bug go check it out: [{}]",
                &key
            ),
            Err(e) => Err(e),
        }
    }

    pub fn get<T: for<'de> Deserialize<'de>>(
        &self,
        key: &str,
    ) -> anyhow::Result<Option<DictionaryDbItem<T>>> {
        let sql = format!(
            "SELECT timestamp, key, value FROM \"{}\" WHERE key=?1",
            self.dict_name
        );

        let mut rows = self.db.select(&sql, &[&key], |row| {
            let ts: String = row.get(0)?;
            let k: String = row.get(1)?;
            let value_str: String = row.get(2)?;
            let v: T = serde_json::from_str(&value_str).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    2,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?;
            Ok(DictionaryDbItem {
                timestamp: ts,
                key: k,
                value: v,
            })
        })?;

        if rows.is_empty() {
            return Ok(None);
        }

        // This is not efficient, but since the list will ever only have 1 record, if any, it's ok.
        Ok(Some(rows.remove(0)))
    }

    pub fn delete<T>(&self, key: &str) -> anyhow::Result<()> {
        let sql = format!("DELETE FROM \"{}\" WHERE key=?1", &self.dict_name);

        let affected = self.db.execute(&sql, &[&key])?;

        if affected == 0 {
            anyhow::bail!("Key not found: {key}");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
    struct ValueType {
        text: String,
        number: i32,
    }

    #[test]
    fn test_dictionary_db_basic_ops() {
        let db_path = ":memory:".to_string();
        let dict_name = "test_dict".to_string();
        let dict = DictionaryDb::new(db_path, dict_name).unwrap();

        // Add
        let key1 = "alpha";
        let value1 = ValueType {
            text: "one".into(),
            number: 1,
        };
        let item1 = dict.add(key1, &value1).unwrap();
        assert_eq!(item1.key, key1);
        assert_eq!(item1.value, value1);

        // Get
        let got1 = dict.get::<ValueType>(key1).unwrap().unwrap();
        assert_eq!(got1.value, value1);

        // Update
        let new_value1 = ValueType {
            text: "uno".into(),
            number: 111,
        };

        let updated = dict.update(key1, &new_value1).unwrap();

        assert_eq!(updated.value, new_value1);
        assert_eq!(updated.key, key1);

        // Get again
        let got1b = dict.get::<ValueType>(key1).unwrap().unwrap();
        assert_eq!(got1b.value, new_value1);

        // Add another
        let key2 = "beta";
        let value2 = ValueType {
            text: "two".into(),
            number: 2,
        };
        let item2 = dict.add(key2, &value2).unwrap();
        assert_eq!(item2.key, key2);
        assert_eq!(item2.value, value2);

        // Delete
        dict.delete::<ValueType>(key2).unwrap();
        let result = dict.get::<ValueType>(key2);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());


        // Delete missing should error
        let missing = dict.delete::<ValueType>("not_existing_key");
        assert!(missing.is_err());
    }

    #[test]
    fn test_dictionary_db_not_found_and_errors() {
        let db_path = ":memory:".to_string();
        let dict_name = "err_dict".to_string();
        let dict = DictionaryDb::new(db_path, dict_name).unwrap();

        // Get a missing key
        let missing = dict.get::<ValueType>("missing");
        assert!(missing.is_ok());
        assert!(missing.unwrap().is_none());


        // Update a missing key
        let result = dict.update(
            "nonexistent".into(),
            &ValueType {
                text: "nada".into(),
                number: 0,
            },
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_dictionary_db_handles_string() {
        let db_path = ":memory:".to_string();
        let dict = DictionaryDb::new(db_path, "list_of_strings".to_string()).unwrap();
        let key = "only_key";
        let value = "hello, world!".to_string();
        let item = dict.add(key, &value).unwrap();
        assert_eq!(item.key, key);
        assert_eq!(item.value, value);

        // Update
        let updated = dict.update(key, &"goodbye".to_string()).unwrap();

        assert_eq!(updated.value, "goodbye".to_string());
    }
}

use crate::models::created_event_item::{
    CreatedEventItem, CreatedEventItemFileType, CreatedEventItemMediaType, CreatedEventItemStatus,
};
use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::ToSql;
use shared::sqlite::generic_db::GenericDb;

pub struct FileStatusController {
    db: GenericDb,
}

impl FileStatusController {
    pub fn new(db_path: String) -> Result<Self> {
        let db = GenericDb::new(db_path)?;

        Self::create_table(&db)?;

        Ok(Self { db })
    }

    fn create_table(db: &GenericDb) -> Result<()> {
        let create_table_sql = r#"
            CREATE TABLE IF NOT EXISTS files (
                full_path TEXT PRIMARY KEY,
                file_name TEXT NOT NULL,
                parent TEXT NOT NULL,
                target_path TEXT NOT NULL,
                item_type TEXT NOT NULL,
                media_type TEXT NOT NULL,
                status TEXT NOT NULL,
                is_archive INTEGER NOT NULL,
                is_main_archive_file INTEGER NOT NULL,
                attempts INTEGER NOT NULL,
                title TEXT NOT NULL,
                year INTEGER,
                season INTEGER,
                episode INTEGER,
                timestamp TEXT NOT NULL
            )
        "#;

        db.execute(create_table_sql, &[])?;

        Ok(())
    }

    pub fn get_file_control(&self, full_path: &str) -> Result<Option<CreatedEventItem>> {
        let select_sql = r#"
            SELECT
                full_path,
                file_name,
                parent,
                target_path,
                item_type,
                media_type,
                status,
                is_archive,
                is_main_archive_file,
                attempts,
                title,
                year,
                season,
                episode,
                timestamp
            FROM files
            WHERE full_path = ?
        "#;

        let result = self.db.select(select_sql, &[&full_path], |row| {
            let full_path: String = row.get(0)?;
            let file_name: String = row.get(1)?;
            let parent: String = row.get(2)?;
            let target_path: String = row.get(3)?;

            let item_type_str: String = row.get(4)?;
            let item_type = match item_type_str.parse::<CreatedEventItemFileType>() {
                Ok(item_type) => item_type,
                Err(_) => {
                    return Err(rusqlite::Error::InvalidColumnType(
                        4,
                        "item_type".to_string(),
                        rusqlite::types::Type::Text,
                    ))
                }
            };

            let media_type_str: String = row.get(5)?;
            let media_type = match media_type_str.parse::<CreatedEventItemMediaType>() {
                Ok(media_type) => media_type,
                Err(_) => {
                    return Err(rusqlite::Error::InvalidColumnType(
                        5,
                        "media_type".to_string(),
                        rusqlite::types::Type::Text,
                    ))
                }
            };

            let status_str: String = row.get(6)?;
            let status = match status_str.parse::<CreatedEventItemStatus>() {
                Ok(status) => status,
                Err(_) => {
                    return Err(rusqlite::Error::InvalidColumnType(
                        6,
                        "status".to_string(),
                        rusqlite::types::Type::Text,
                    ))
                }
            };

            let is_archive: bool = row.get::<_, i32>(7)? == 1;
            let is_main_archive_file: bool = row.get::<_, i32>(8)? == 1;
            let attempts: usize = row.get(9)?;
            let title: String = row.get(10)?;
            let year: Option<u32> = row.get(11)?;
            let season: Option<u32> = row.get(12)?;
            let episode: Option<u32> = row.get(13)?;
            let timestamp: DateTime<Utc> = match row.get::<_, String>(14)?.parse() {
                Ok(ts) => ts,
                Err(_) => {
                    return Err(rusqlite::Error::InvalidColumnType(
                        14,
                        "timestamp".to_string(),
                        rusqlite::types::Type::Text,
                    ))
                }
            };

            Ok(CreatedEventItem {
                full_path,
                file_name,
                parent,
                target_path,
                item_type,
                media_type,
                status,
                is_archive,
                is_main_archive_file,
                attempts,
                title,
                year,
                season,
                episode,
                timestamp,
            })
        })?;

        if result.is_empty() {
            return Ok(None);
        }

        Ok(result.get(0).cloned())
    }

    pub fn add_file_control(&self, item: &CreatedEventItem) -> Result<()> {
        let insert_sql = r#"
            INSERT INTO files (
                full_path,
                file_name,
                parent,
                target_path,
                item_type,
                media_type,
                status,
                is_archive,
                is_main_archive_file,
                attempts,
                title,
                year,
                season,
                episode,
                timestamp,
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#;

        self.db.execute(insert_sql, &[
            &item.full_path,
            &item.file_name,
            &item.parent,
            &item.target_path,
            &format!("{:?}", item.item_type),
            &format!("{:?}", item.media_type),
            &format!("{:?}", item.status),
            &item.is_archive as &dyn ToSql,
            &item.is_main_archive_file as &dyn ToSql,
            &item.attempts,
            &item.title,
            &item.year,
            &item.season,
            &item.episode,
            &item.timestamp.to_rfc3339(),
        ])?;

        Ok(())
    }

    pub fn update_file_control(&self, item: &CreatedEventItem) -> Result<()> {
        let update_sql = r#"
            UPDATE files SET
                full_path = ?,
                file_name = ?,
                parent = ?,
                target_path = ?,
                item_type = ?,
                media_type = ?,
                status = ?,
                is_archive = ?,
                is_main_archive_file = ?,
                attempts = ?,
                title = ?,
                year = ?,
                season = ?,
                episode = ?,
                timestamp = ?
            WHERE full_path = ?
        "#;

        self.db.execute(update_sql, &[
            &item.full_path,
            &item.file_name,
            &item.parent,
            &item.target_path,
            &format!("{:?}", item.item_type),
            &format!("{:?}", item.media_type),
            &format!("{:?}", item.status),
            &item.is_archive as &dyn ToSql,
            &item.is_main_archive_file as &dyn ToSql,
            &item.attempts,
            &item.title,
            &item.year,
            &item.season,
            &item.episode,
            &item.timestamp.to_rfc3339(),
            &item.full_path, // Where
        ])?;

        Ok(())
    }
}
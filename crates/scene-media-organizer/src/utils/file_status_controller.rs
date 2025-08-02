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
                timestamp
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rstest::rstest;
    use tempfile::NamedTempFile;

    fn create_test_item() -> CreatedEventItem {
        CreatedEventItem {
            full_path: "/test/path/movie.mkv".to_string(),
            file_name: "movie.mkv".to_string(),
            parent: "/test/path".to_string(),
            target_path: "/target/path/movie.mkv".to_string(),
            item_type: CreatedEventItemFileType::File,
            media_type: CreatedEventItemMediaType::Movie,
            status: CreatedEventItemStatus::New,
            is_archive: false,
            is_main_archive_file: false,
            attempts: 0,
            title: "Test Movie".to_string(),
            year: Some(2023),
            season: None,
            episode: None,
            timestamp: Utc::now(),
        }
    }

    fn create_test_tv_item() -> CreatedEventItem {
        CreatedEventItem {
            full_path: "/test/tv/show.s01e01.mkv".to_string(),
            file_name: "show.s01e01.mkv".to_string(),
            parent: "/test/tv".to_string(),
            target_path: "/target/tv/show/season1/episode1.mkv".to_string(),
            item_type: CreatedEventItemFileType::File,
            media_type: CreatedEventItemMediaType::TvShow,
            status: CreatedEventItemStatus::Identified,
            is_archive: true,
            is_main_archive_file: true,
            attempts: 2,
            title: "Test TV Show".to_string(),
            year: Some(2022),
            season: Some(1),
            episode: Some(1),
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn test_new_controller_creates_database_and_table() {
        let temp_file = NamedTempFile::new().unwrap();
        let db_path = temp_file.path().to_string_lossy().to_string();

        let controller = FileStatusController::new(db_path).unwrap();

        // Verify the table was created by trying to query it
        let result = controller.get_file_control("nonexistent");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn test_new_controller_with_in_memory_database() {
        let controller = FileStatusController::new(":memory:".to_string());
        assert!(controller.is_ok());
    }

    #[test]
    fn test_add_file_control_success() {
        let controller = FileStatusController::new(":memory:".to_string()).unwrap();
        let test_item = create_test_item();

        let result = controller.add_file_control(&test_item);
        assert!(result.is_ok());
    }

    #[test]
    fn test_get_file_control_returns_none_for_nonexistent_file() {
        let controller = FileStatusController::new(":memory:".to_string()).unwrap();

        let result = controller.get_file_control("/nonexistent/path");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn test_add_and_get_file_control_movie() {
        let controller = FileStatusController::new(":memory:".to_string()).unwrap();
        let test_item = create_test_item();

        // Add the item
        controller.add_file_control(&test_item).unwrap();

        // Retrieve the item
        let retrieved = controller.get_file_control(&test_item.full_path).unwrap();
        assert!(retrieved.is_some());

        let retrieved_item = retrieved.unwrap();
        assert_eq!(retrieved_item.full_path, test_item.full_path);
        assert_eq!(retrieved_item.file_name, test_item.file_name);
        assert_eq!(retrieved_item.parent, test_item.parent);
        assert_eq!(retrieved_item.target_path, test_item.target_path);
        assert_eq!(retrieved_item.item_type, test_item.item_type);
        assert_eq!(retrieved_item.media_type, test_item.media_type);
        assert_eq!(retrieved_item.status, test_item.status);
        assert_eq!(retrieved_item.is_archive, test_item.is_archive);
        assert_eq!(retrieved_item.is_main_archive_file, test_item.is_main_archive_file);
        assert_eq!(retrieved_item.attempts, test_item.attempts);
        assert_eq!(retrieved_item.title, test_item.title);
        assert_eq!(retrieved_item.year, test_item.year);
        assert_eq!(retrieved_item.season, test_item.season);
        assert_eq!(retrieved_item.episode, test_item.episode);
        // Note: We might not want to check the timestamp because it might suffer slight differences.
        assert_eq!(retrieved_item.timestamp, test_item.timestamp);

    }

    #[test]
    fn test_add_and_get_file_control_tv_show() {
        let controller = FileStatusController::new(":memory:".to_string()).unwrap();
        let test_item = create_test_tv_item();

        // Add the item
        controller.add_file_control(&test_item).unwrap();

        // Retrieve the item
        let retrieved = controller.get_file_control(&test_item.full_path).unwrap();
        assert!(retrieved.is_some());

        let retrieved_item = retrieved.unwrap();
        assert_eq!(retrieved_item.full_path, test_item.full_path);
        assert_eq!(retrieved_item.media_type, CreatedEventItemMediaType::TvShow);
        assert_eq!(retrieved_item.status, CreatedEventItemStatus::Identified);
        assert_eq!(retrieved_item.is_archive, true);
        assert_eq!(retrieved_item.is_main_archive_file, true);
        assert_eq!(retrieved_item.attempts, 2);
        assert_eq!(retrieved_item.season, Some(1));
        assert_eq!(retrieved_item.episode, Some(1));
    }

    #[test]
    fn test_update_file_control() {
        let controller = FileStatusController::new(":memory:".to_string()).unwrap();
        let mut test_item = create_test_item();

        // Add the item
        controller.add_file_control(&test_item).unwrap();

        // Update the item
        test_item.status = CreatedEventItemStatus::Done;
        test_item.attempts = 5;
        test_item.title = "Updated Movie Title".to_string();

        let update_result = controller.update_file_control(&test_item);
        assert!(update_result.is_ok());

        // Retrieve and verify the update
        let retrieved = controller.get_file_control(&test_item.full_path).unwrap().unwrap();
        assert_eq!(retrieved.status, CreatedEventItemStatus::Done);
        assert_eq!(retrieved.attempts, 5);
        assert_eq!(retrieved.title, "Updated Movie Title");
    }

    #[test]
    fn test_update_nonexistent_file_control() {
        let controller = FileStatusController::new(":memory:".to_string()).unwrap();
        let test_item = create_test_item();

        // Try to update a non-existent item (should not fail, but won't affect any rows)
        let result = controller.update_file_control(&test_item);
        assert!(result.is_ok());

        // Verify the item still doesn't exist
        let retrieved = controller.get_file_control(&test_item.full_path).unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_add_duplicate_file_control_fails() {
        let controller = FileStatusController::new(":memory:".to_string()).unwrap();
        let test_item = create_test_item();

        // Add the item once
        controller.add_file_control(&test_item).unwrap();

        // Try to add the same item again (should fail due to PRIMARY KEY constraint)
        let result = controller.add_file_control(&test_item);
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_different_files() {
        let controller = FileStatusController::new(":memory:".to_string()).unwrap();

        let item1 = create_test_item();
        let mut item2 = create_test_tv_item();
        item2.full_path = "/different/path/file.mkv".to_string();

        // Add both items
        controller.add_file_control(&item1).unwrap();
        controller.add_file_control(&item2).unwrap();

        // Retrieve both items
        let retrieved1 = controller.get_file_control(&item1.full_path).unwrap();
        let retrieved2 = controller.get_file_control(&item2.full_path).unwrap();

        assert!(retrieved1.is_some());
        assert!(retrieved2.is_some());
        assert_eq!(retrieved1.unwrap().media_type, CreatedEventItemMediaType::Movie);
        assert_eq!(retrieved2.unwrap().media_type, CreatedEventItemMediaType::TvShow);
    }

    #[test]
    fn test_file_control_with_all_statuses() {
        let controller = FileStatusController::new(":memory:".to_string()).unwrap();

        let statuses = vec![
            CreatedEventItemStatus::New,
            CreatedEventItemStatus::Identified,
            CreatedEventItemStatus::Prepared,
            CreatedEventItemStatus::Copying,
            CreatedEventItemStatus::Done,
            CreatedEventItemStatus::Ignored,
        ];

        for (i, status) in statuses.iter().enumerate() {
            let mut test_item = create_test_item();
            test_item.full_path = format!("/test/path/file_{}.mkv", i);
            test_item.status = *status;

            controller.add_file_control(&test_item).unwrap();

            let retrieved = controller.get_file_control(&test_item.full_path).unwrap().unwrap();
            assert_eq!(retrieved.status, *status);
        }
    }

    #[test]
    fn test_file_control_with_all_media_types() {
        let controller = FileStatusController::new(":memory:".to_string()).unwrap();

        let media_types = vec![
            CreatedEventItemMediaType::Movie,
            CreatedEventItemMediaType::TvShow,
        ];

        for (i, media_type) in media_types.iter().enumerate() {
            let mut test_item = create_test_item();
            test_item.full_path = format!("/test/path/file_{}.mkv", i);
            test_item.media_type = *media_type;

            controller.add_file_control(&test_item).unwrap();

            let retrieved = controller.get_file_control(&test_item.full_path).unwrap().unwrap();
            assert_eq!(retrieved.media_type, *media_type);
        }
    }

    #[test]
    fn test_file_control_with_optional_fields() {
        let controller = FileStatusController::new(":memory:".to_string()).unwrap();

        // Test with all optional fields as None
        let mut test_item = create_test_item();
        test_item.full_path = "/test/no_optional_fields.mkv".to_string();
        test_item.year = None;
        test_item.season = None;
        test_item.episode = None;

        controller.add_file_control(&test_item).unwrap();

        let retrieved = controller.get_file_control(&test_item.full_path).unwrap().unwrap();
        assert_eq!(retrieved.year, None);
        assert_eq!(retrieved.season, None);
        assert_eq!(retrieved.episode, None);
    }

    #[rstest]
    #[case(1, false, false)]
    #[case(2, false, true)]
    #[case(3, true, false)]
    #[case(4, true, true)]
    fn test_file_control_with_boolean_flags(#[case] test_num: usize, #[case] is_archive: bool, #[case]  is_main_archive_file: bool) {
        let controller = FileStatusController::new(":memory:".to_string()).unwrap();

        let mut test_item = create_test_item();
        test_item.full_path = format!("/test/path/file_{}.mkv", test_num);
        test_item.is_archive = is_archive;
        test_item.is_main_archive_file = is_main_archive_file;

        controller.add_file_control(&test_item).unwrap();

        let retrieved = controller.get_file_control(&test_item.full_path).unwrap().unwrap();
        assert_eq!(retrieved.is_archive, is_archive);
        assert_eq!(retrieved.is_main_archive_file, is_main_archive_file);        
    }

    #[test]
    fn test_file_control_with_large_attempts_value() {
        let controller = FileStatusController::new(":memory:".to_string()).unwrap();

        let mut test_item = create_test_item();

        // usize::MAX_VALUE won't work for integer column in SQLite for 64bit systems.
        // also, I hope I don't have to try that many times, so It's ok...
        let large_attempt: usize = 4294967295;
        test_item.attempts = large_attempt;

        controller.add_file_control(&test_item).unwrap();

        let retrieved = controller.get_file_control(&test_item.full_path).unwrap().unwrap();
        assert_eq!(retrieved.attempts, large_attempt);
    }

    #[test]
    fn test_file_control_persistence() {
        let temp_file = NamedTempFile::new().unwrap();
        let db_path = temp_file.path().to_string_lossy().to_string();

        let test_item = create_test_item();

        // Create controller, add item, and drop it
        {
            let controller = FileStatusController::new(db_path.clone()).unwrap();
            controller.add_file_control(&test_item).unwrap();
        }

        // Create a new controller with the same database and verify item persists
        {
            let controller = FileStatusController::new(db_path).unwrap();
            let retrieved = controller.get_file_control(&test_item.full_path).unwrap();
            assert!(retrieved.is_some());
            assert_eq!(retrieved.unwrap().full_path, test_item.full_path);
        }
    }
}
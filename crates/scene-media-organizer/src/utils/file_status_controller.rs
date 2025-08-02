use anyhow::Result;
use shared::sqlite::generic_db::GenericDb;

pub struct FileStatusController {
    db: GenericDb
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
}
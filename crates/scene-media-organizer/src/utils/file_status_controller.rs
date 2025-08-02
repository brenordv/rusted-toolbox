use anyhow::Result;
use shared::sqlite::generic_db::GenericDb;

pub struct FileStatusController {
    db: GenericDb
}

impl FileStatusController {
    pub fn new(db_path: String) -> Result<Self> {
        let db = GenericDb::new(db_path)?;
        
        
        
        Ok(Self { db })
    }
}
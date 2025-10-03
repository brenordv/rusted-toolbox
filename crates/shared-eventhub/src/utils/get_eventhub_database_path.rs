use crate::utils::extract_eventhub_endpoint_from_connection_string::extract_eventhub_endpoint_from_connection_string;
use shared::system::resolve_path_with_base::resolve_path_with_base;
use std::path::PathBuf;

pub fn get_eventhub_database_path(
    connection_string: &str,
    base_data_folder: &str,
    database_path: &str,
) -> Result<PathBuf, anyhow::Error> {
    // Extract endpoint from connection string for database filename
    let endpoint = extract_eventhub_endpoint_from_connection_string(connection_string)?;

    // Create the endpoint-specific database path
    let db_base_dir = resolve_path_with_base(base_data_folder, database_path);
    let db_path = db_base_dir.join(format!("{}.db", endpoint));

    // Ensure database directory exists
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(db_path)
}

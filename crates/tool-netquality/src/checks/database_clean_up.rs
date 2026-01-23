use rusqlite::Connection;
use tracing::{info, warn};
use crate::persistence::db;

pub fn run_database_cleanup(mut connection: &mut Connection) {
    match db::cleanup_old_activity(&mut connection) {
        Ok(stats) => {
            info!(
                            "Database cleanup complete: {} sessions, {} connectivity, {} speed rows removed.",
                            stats.sessions_deleted, stats.connectivity_deleted, stats.speed_deleted
                        );
        }
        Err(error) => {
            warn!("Database cleanup failed: {error:#}");
        }
    }
}
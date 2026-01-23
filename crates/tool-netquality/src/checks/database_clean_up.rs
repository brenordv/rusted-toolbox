use crate::persistence::db;
use rusqlite::Connection;
use tracing::{info, warn};

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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration as ChronoDuration, Utc};
    use rusqlite::params;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn create_temp_db_path() -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("netquality-cleanup-test-{}.db", nanos))
    }

    #[test]
    fn database_cleanup_removes_old_rows() {
        let path = create_temp_db_path();
        let mut conn = db::create_database(&path).expect("create database");

        let old_timestamp = (Utc::now() - ChronoDuration::days(400)).to_rfc3339();
        let new_timestamp = Utc::now().to_rfc3339();

        conn.execute(
            r#"
            INSERT INTO activity_connectivity (timestamp, url, result, elapsed_time, success)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![old_timestamp, "https://old.example.com", "204", 10, 1],
        )
        .expect("insert old connectivity");
        let old_connectivity_id = conn.last_insert_rowid();

        conn.execute(
            r#"
            INSERT INTO activity_speed (
                timestamp,
                download_speed,
                upload_speed,
                download_threshold,
                upload_threshold,
                success,
                elapsed_time
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![old_timestamp, 12.5, 6.0, "Slow", "Slow", 1, 100],
        )
        .expect("insert old speed");
        let old_speed_id = conn.last_insert_rowid();

        conn.execute(
            r#"
            INSERT INTO sessions (parent_session_id, connectivity_id, speed_id)
            VALUES (?1, ?2, ?3)
            "#,
            params![Option::<i64>::None, old_connectivity_id, old_speed_id],
        )
        .expect("insert old session");

        conn.execute(
            r#"
            INSERT INTO activity_connectivity (timestamp, url, result, elapsed_time, success)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![new_timestamp, "https://new.example.com", "204", 15, 1],
        )
        .expect("insert new connectivity");
        let new_connectivity_id = conn.last_insert_rowid();

        conn.execute(
            r#"
            INSERT INTO activity_speed (
                timestamp,
                download_speed,
                upload_speed,
                download_threshold,
                upload_threshold,
                success,
                elapsed_time
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![new_timestamp, 120.0, 30.0, "Expected", "Medium", 1, 120],
        )
        .expect("insert new speed");
        let new_speed_id = conn.last_insert_rowid();

        conn.execute(
            r#"
            INSERT INTO sessions (parent_session_id, connectivity_id, speed_id)
            VALUES (?1, ?2, ?3)
            "#,
            params![Option::<i64>::None, new_connectivity_id, new_speed_id],
        )
        .expect("insert new session");

        run_database_cleanup(&mut conn);

        let session_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))
            .expect("count sessions");
        let connectivity_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM activity_connectivity",
                [],
                |row| row.get(0),
            )
            .expect("count connectivity");
        let speed_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM activity_speed", [], |row| row.get(0))
            .expect("count speed");

        assert_eq!(session_count, 1);
        assert_eq!(connectivity_count, 1);
        assert_eq!(speed_count, 1);

        drop(conn);
        let _ = fs::remove_file(path);
    }
}

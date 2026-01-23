use crate::models::{ConnectivityResult, SpeedResult};
use anyhow::Result;
use chrono::{Duration as ChronoDuration, Utc};
use rusqlite::{params, Connection};
use std::path::Path;

const CLEANUP_RETENTION_DAYS: i64 = 365;

#[derive(Debug, Clone, Copy)]
pub(crate) struct CleanupStats {
    pub(crate) sessions_deleted: usize,
    pub(crate) connectivity_deleted: usize,
    pub(crate) speed_deleted: usize,
}

pub(crate) fn create_database(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).ok();
        }
    }

    let conn = Connection::open(path)?;
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS activity_connectivity (
            activity_id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL,
            url TEXT NOT NULL,
            result TEXT NOT NULL,
            elapsed_time INTEGER NOT NULL,
            success INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS activity_speed (
            activity_id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL,
            download_speed REAL NOT NULL,
            upload_speed REAL,
            download_threshold TEXT NOT NULL,
            upload_threshold TEXT,
            success INTEGER NOT NULL,
            elapsed_time INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS sessions (
            session_id INTEGER PRIMARY KEY AUTOINCREMENT,
            parent_session_id INTEGER,
            connectivity_id INTEGER,
            speed_id INTEGER,
            FOREIGN KEY(connectivity_id) REFERENCES activity_connectivity(activity_id),
            FOREIGN KEY(speed_id) REFERENCES activity_speed(activity_id),
            FOREIGN KEY(parent_session_id) REFERENCES sessions(session_id)
        );
        CREATE VIEW IF NOT EXISTS session_activity_view AS
        SELECT
            s.session_id,
            s.parent_session_id,
            c.timestamp AS connectivity_timestamp,
            c.url AS connectivity_url,
            c.result AS connectivity_result,
            c.elapsed_time AS connectivity_elapsed_time,
            c.success AS connectivity_success,
            sp.timestamp AS speed_timestamp,
            sp.download_speed,
            sp.upload_speed,
            sp.download_threshold,
            sp.upload_threshold,
            sp.success AS speed_success,
            sp.elapsed_time AS speed_elapsed_time
        FROM sessions s
        LEFT JOIN activity_connectivity c ON c.activity_id = s.connectivity_id
        LEFT JOIN activity_speed sp ON sp.activity_id = s.speed_id;
        "#,
    )?;

    Ok(conn)
}

pub(crate) fn insert_connectivity_activity(
    conn: &Connection,
    result: &ConnectivityResult,
) -> Result<i64> {
    conn.execute(
        r#"
        INSERT INTO activity_connectivity (timestamp, url, result, elapsed_time, success)
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
        params![
            result.timestamp.to_rfc3339(),
            result.url,
            result.result,
            result.elapsed_ms,
            if result.success { 1 } else { 0 }
        ],
    )?;

    Ok(conn.last_insert_rowid())
}

pub(crate) fn insert_speed_activity(conn: &Connection, result: &SpeedResult) -> Result<i64> {
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
        params![
            result.timestamp.to_rfc3339(),
            result.download_mbps,
            result.upload_mbps,
            result.download_threshold.label(),
            result.upload_threshold.map(|threshold| threshold.label()),
            if result.success { 1 } else { 0 },
            result.elapsed_ms
        ],
    )?;

    Ok(conn.last_insert_rowid())
}

pub(crate) fn insert_session(
    conn: &Connection,
    parent_session_id: Option<i64>,
    connectivity_id: Option<i64>,
    speed_id: Option<i64>,
) -> Result<i64> {
    conn.execute(
        r#"
        INSERT INTO sessions (parent_session_id, connectivity_id, speed_id)
        VALUES (?1, ?2, ?3)
        "#,
        params![parent_session_id, connectivity_id, speed_id],
    )?;

    Ok(conn.last_insert_rowid())
}

pub(crate) fn cleanup_old_activity(conn: &mut Connection) -> Result<CleanupStats> {
    let cutoff = (Utc::now() - ChronoDuration::days(CLEANUP_RETENTION_DAYS)).to_rfc3339();
    let tx = conn.transaction()?;

    let sessions_deleted = tx.execute(
        r#"
        DELETE FROM sessions
        WHERE session_id IN (
            SELECT s.session_id
            FROM sessions s
            LEFT JOIN activity_connectivity c ON c.activity_id = s.connectivity_id
            LEFT JOIN activity_speed sp ON sp.activity_id = s.speed_id
            WHERE (c.timestamp IS NULL OR c.timestamp < ?1)
              AND (sp.timestamp IS NULL OR sp.timestamp < ?1)
        )
        "#,
        params![cutoff],
    )?;

    let connectivity_deleted = tx.execute(
        r#"
        DELETE FROM activity_connectivity
        WHERE timestamp < ?1
          AND activity_id NOT IN (
              SELECT connectivity_id
              FROM sessions
              WHERE connectivity_id IS NOT NULL
          )
        "#,
        params![cutoff],
    )?;

    let speed_deleted = tx.execute(
        r#"
        DELETE FROM activity_speed
        WHERE timestamp < ?1
          AND activity_id NOT IN (
              SELECT speed_id
              FROM sessions
              WHERE speed_id IS NOT NULL
          )
        "#,
        params![cutoff],
    )?;

    tx.commit()?;

    Ok(CleanupStats {
        sessions_deleted,
        connectivity_deleted,
        speed_deleted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ThresholdCategory;
    use chrono::Utc;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn create_temp_db_path() -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("netquality-test-{}.db", nanos))
    }

    fn create_test_connection() -> (Connection, std::path::PathBuf) {
        let path = create_temp_db_path();
        let conn = create_database(&path).expect("create_database should succeed");
        (conn, path)
    }

    fn build_connectivity_result() -> ConnectivityResult {
        ConnectivityResult {
            timestamp: Utc::now(),
            url: "https://example.com/health".to_string(),
            result: "204".to_string(),
            elapsed_ms: 42,
            success: true,
        }
    }

    fn build_speed_result() -> SpeedResult {
        SpeedResult {
            timestamp: Utc::now(),
            download_mbps: 120.5,
            upload_mbps: Some(24.9),
            download_threshold: ThresholdCategory::Expected,
            upload_threshold: Some(ThresholdCategory::MediumFast),
            elapsed_ms: 812,
            success: true,
        }
    }

    #[test]
    fn insert_activity_rows_roundtrip() {
        let (conn, path) = create_test_connection();

        let connectivity = build_connectivity_result();
        let connectivity_id =
            insert_connectivity_activity(&conn, &connectivity).expect("insert connectivity");

        let speed = build_speed_result();
        let speed_id = insert_speed_activity(&conn, &speed).expect("insert speed");

        let (stored_url, stored_result, stored_success): (String, String, i64) = conn
            .query_row(
                "SELECT url, result, success FROM activity_connectivity WHERE activity_id = ?1",
                params![connectivity_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("query connectivity");

        assert_eq!(stored_url, connectivity.url);
        assert_eq!(stored_result, connectivity.result);
        assert_eq!(stored_success, 1);

        let (stored_download, stored_upload, stored_success): (f64, Option<f64>, i64) = conn
            .query_row(
                "SELECT download_speed, upload_speed, success FROM activity_speed WHERE activity_id = ?1",
                params![speed_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("query speed");

        assert_eq!(stored_download, speed.download_mbps);
        assert_eq!(stored_upload, speed.upload_mbps);
        assert_eq!(stored_success, 1);

        drop(conn);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn parent_session_id_is_persisted_and_visible_in_view() {
        let (conn, path) = create_test_connection();

        let connectivity_id = insert_connectivity_activity(&conn, &build_connectivity_result())
            .expect("insert connectivity");
        let speed_id = insert_speed_activity(&conn, &build_speed_result()).expect("insert speed");

        let parent_session_id = insert_session(&conn, None, Some(connectivity_id), Some(speed_id))
            .expect("insert parent session");

        let child_session_id = insert_session(&conn, Some(parent_session_id), None, None)
            .expect("insert child session");

        let stored_parent: Option<i64> = conn
            .query_row(
                "SELECT parent_session_id FROM sessions WHERE session_id = ?1",
                params![child_session_id],
                |row| row.get(0),
            )
            .expect("query sessions");

        assert_eq!(stored_parent, Some(parent_session_id));

        let view_parent: Option<i64> = conn
            .query_row(
                "SELECT parent_session_id FROM session_activity_view WHERE session_id = ?1",
                params![child_session_id],
                |row| row.get(0),
            )
            .expect("query session_activity_view");

        assert_eq!(view_parent, Some(parent_session_id));

        drop(conn);
        let _ = fs::remove_file(path);
    }
}

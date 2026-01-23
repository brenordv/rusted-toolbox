use crate::models::{ConnectivityResult, SpeedResult};
use anyhow::Result;
use rusqlite::{params, Connection};
use std::path::Path;

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
            connectivity_id INTEGER,
            speed_id INTEGER,
            FOREIGN KEY(connectivity_id) REFERENCES activity_connectivity(activity_id),
            FOREIGN KEY(speed_id) REFERENCES activity_speed(activity_id)
        );
        CREATE VIEW IF NOT EXISTS session_activity_view AS
        SELECT
            s.session_id,
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
    connectivity_id: Option<i64>,
    speed_id: Option<i64>,
) -> Result<i64> {
    conn.execute(
        r#"
        INSERT INTO sessions (connectivity_id, speed_id)
        VALUES (?1, ?2)
        "#,
        params![connectivity_id, speed_id],
    )?;

    Ok(conn.last_insert_rowid())
}
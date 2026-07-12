//! Scene-free normal-domain Session persistence for the unified Run runtime.
//!
//! The legacy `sessions.scene` column remains until the final copy-transform
//! cutover. New normal-domain sessions write an empty compatibility value and
//! never expose or read it as a routing input.

use crate::error::AppResult;
use crate::storage::db::Database;

/// Opaque identity of one normal-domain conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NormalSession {
    /// SQLite primary key used by Run and message foreign keys.
    pub(crate) session_id: i64,
    /// Opaque client-facing key with no scene or document meaning.
    pub(crate) session_key: String,
}

/// Repository for scene-free normal-domain sessions.
pub(crate) struct NormalSessionRepository;

impl NormalSessionRepository {
    /// Create one normal-domain session without an implicit scene or note target.
    pub(crate) fn create(db: &Database) -> AppResult<NormalSession> {
        let session_key = format!("run_session:{}", uuid::Uuid::new_v4());
        let now = chrono::Utc::now().to_rfc3339();
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO sessions
                 (session_key, scene, note_path, created_at, updated_at)
                 VALUES (?1, '', NULL, ?2, ?2)",
                rusqlite::params![session_key, now],
            )?;
            Ok(NormalSession {
                session_id: conn.last_insert_rowid(),
                session_key,
            })
        })
    }

    /// Resolve an opaque normal-domain session key without reading legacy bindings.
    pub(crate) fn get(db: &Database, session_key: &str) -> AppResult<Option<NormalSession>> {
        db.with_read_conn(|conn| {
            let result = conn.query_row(
                "SELECT id, session_key FROM sessions
                 WHERE session_key = ?1 AND scene = '' AND note_path IS NULL",
                [session_key],
                |row| {
                    Ok(NormalSession {
                        session_id: row.get(0)?,
                        session_key: row.get(1)?,
                    })
                },
            );
            match result {
                Ok(session) => Ok(Some(session)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(error) => Err(error.into()),
            }
        })
    }
}

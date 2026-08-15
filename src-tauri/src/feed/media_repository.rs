//! Persisted state for RSS image/document cache entries.
//!
//! The migration-069 `feed_media` table is the item-level source of truth for
//! ready/failed state, retry backoff and last access. File mtime remains the
//! eviction trigger for legacy and not-yet-recorded files; this repository is
//! intentionally stateless and receives a SQLite connection like every other
//! feed repository.

use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use crate::error::AppResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum FeedMediaKind {
    Image,
    Document,
}

impl FeedMediaKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Document => "document",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "image" => Some(Self::Image),
            "document" => Some(Self::Document),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct FeedMediaKey {
    pub kind: FeedMediaKind,
    pub cache_key: String,
}

fn rfc3339(now: DateTime<Utc>) -> String {
    now.to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub(crate) struct FeedMediaRepository;

impl FeedMediaRepository {
    pub(crate) fn record_ready(
        conn: &Connection,
        item_id: &str,
        kind: FeedMediaKind,
        cache_key: &str,
        size_bytes: u64,
        now: DateTime<Utc>,
    ) -> AppResult<()> {
        let now_str = rfc3339(now);
        conn.execute(
            "INSERT INTO feed_media (
                 item_id, source_url_hash, media_kind, cache_key, size_bytes,
                 state, failure_count, retry_after, last_accessed_at
             )
             VALUES (?1, ?2, ?3, ?2, ?4, 'ready', 0, NULL, ?5)
             ON CONFLICT(item_id, source_url_hash) DO UPDATE SET
                 media_kind = excluded.media_kind,
                 cache_key = excluded.cache_key,
                 size_bytes = excluded.size_bytes,
                 state = 'ready',
                 failure_count = 0,
                 retry_after = NULL,
                 last_accessed_at = excluded.last_accessed_at",
            params![
                item_id,
                cache_key,
                kind.as_str(),
                size_bytes as i64,
                now_str
            ],
        )?;
        Ok(())
    }

    pub(crate) fn record_failed(
        conn: &Connection,
        item_id: &str,
        kind: FeedMediaKind,
        cache_key: &str,
        retry_after: Option<DateTime<Utc>>,
        now: DateTime<Utc>,
    ) -> AppResult<()> {
        let now_str = rfc3339(now);
        let retry_after = retry_after.map(rfc3339);
        conn.execute(
            "INSERT INTO feed_media (
                 item_id, source_url_hash, media_kind, cache_key,
                 state, failure_count, retry_after, last_accessed_at
             )
             VALUES (?1, ?2, ?3, ?2, 'failed', 1, ?4, ?5)
             ON CONFLICT(item_id, source_url_hash) DO UPDATE SET
                 media_kind = excluded.media_kind,
                 cache_key = excluded.cache_key,
                 state = 'failed',
                 failure_count = feed_media.failure_count + 1,
                 retry_after = excluded.retry_after,
                 last_accessed_at = excluded.last_accessed_at",
            params![
                item_id,
                cache_key,
                kind.as_str(),
                retry_after.as_deref(),
                now_str
            ],
        )?;
        Ok(())
    }

    pub(crate) fn is_retry_blocked(
        conn: &Connection,
        item_id: &str,
        cache_key: &str,
        now: DateTime<Utc>,
    ) -> AppResult<bool> {
        let blocked = conn
            .query_row(
                "SELECT retry_after > ?3
                 FROM feed_media
                 WHERE item_id = ?1 AND source_url_hash = ?2
                   AND state = 'failed' AND retry_after IS NOT NULL",
                params![item_id, cache_key, rfc3339(now)],
                |row| row.get::<_, bool>(0),
            )
            .optional()?
            .unwrap_or(false);
        Ok(blocked)
    }

    pub(crate) fn reset_all(conn: &Connection) -> AppResult<()> {
        conn.execute("DELETE FROM feed_media", [])?;
        Ok(())
    }

    pub(crate) fn delete_for_item(conn: &Connection, item_id: &str) -> AppResult<()> {
        conn.execute("DELETE FROM feed_media WHERE item_id = ?1", [item_id])?;
        Ok(())
    }

    pub(crate) fn keys_for_deleted_items_due(
        conn: &Connection,
        now: DateTime<Utc>,
    ) -> AppResult<Vec<FeedMediaKey>> {
        let mut statement = conn.prepare(
            "SELECT fm.media_kind, fm.cache_key
             FROM feed_media fm
             JOIN feed_items i ON i.id = fm.item_id
             WHERE i.deleted_at IS NOT NULL AND i.purge_after <= ?1",
        )?;
        let rows = statement.query_map([rfc3339(now)], |row| {
            Ok(FeedMediaKey {
                kind: FeedMediaKind::parse(&row.get::<_, String>(0)?)
                    .expect("feed_media.media_kind CHECK constraint"),
                cache_key: row.get(1)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub(crate) fn keys_for_deleted_items_all(conn: &Connection) -> AppResult<Vec<FeedMediaKey>> {
        let mut statement = conn.prepare(
            "SELECT fm.media_kind, fm.cache_key
             FROM feed_media fm
             JOIN feed_items i ON i.id = fm.item_id
             WHERE i.deleted_at IS NOT NULL
               AND COALESCE(i.deletion_reason, 'retention') != 'source_removed'",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(FeedMediaKey {
                kind: FeedMediaKind::parse(&row.get::<_, String>(0)?)
                    .expect("feed_media.media_kind CHECK constraint"),
                cache_key: row.get(1)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub(crate) fn keys_for_expired_sources(
        conn: &Connection,
        now: DateTime<Utc>,
    ) -> AppResult<Vec<FeedMediaKey>> {
        let mut statement = conn.prepare(
            "SELECT fm.media_kind, fm.cache_key
             FROM feed_media fm
             JOIN feed_items i ON i.id = fm.item_id
             JOIN feed_sources s ON s.id = i.source_id
             WHERE s.deleted_at IS NOT NULL AND s.purge_after <= ?1",
        )?;
        let rows = statement.query_map([rfc3339(now)], |row| {
            Ok(FeedMediaKey {
                kind: FeedMediaKind::parse(&row.get::<_, String>(0)?)
                    .expect("feed_media.media_kind CHECK constraint"),
                cache_key: row.get(1)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub(crate) fn keys_for_source(
        conn: &Connection,
        source_id: &str,
    ) -> AppResult<Vec<FeedMediaKey>> {
        let mut statement = conn.prepare(
            "SELECT fm.media_kind, fm.cache_key
             FROM feed_media fm
             JOIN feed_items i ON i.id = fm.item_id
             WHERE i.source_id = ?1",
        )?;
        let rows = statement.query_map([source_id], |row| {
            Ok(FeedMediaKey {
                kind: FeedMediaKind::parse(&row.get::<_, String>(0)?)
                    .expect("feed_media.media_kind CHECK constraint"),
                cache_key: row.get(1)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub(crate) fn is_cache_key_referenced(conn: &Connection, cache_key: &str) -> AppResult<bool> {
        let referenced = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM feed_media WHERE cache_key = ?1)",
            [cache_key],
            |row| row.get(0),
        )?;
        Ok(referenced)
    }
}

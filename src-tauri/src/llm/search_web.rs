use crate::error::AppResult;
use crate::storage::db::Database;

const MAX_SEARCH_CACHE_ROWS: usize = 512;

pub fn cleanup_expired_search_cache(db: &Database) -> AppResult<usize> {
    db.with_conn(|conn| {
        let deleted = conn.execute(
            "DELETE FROM search_cache WHERE expires_at < datetime('now')",
            [],
        )?;
        Ok(deleted)
    })
    .and_then(|expired| prune_search_cache_lru(db, MAX_SEARCH_CACHE_ROWS).map(|lru| expired + lru))
}

fn prune_search_cache_lru(db: &Database, max_rows: usize) -> AppResult<usize> {
    db.with_conn(|conn| {
        let row_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM search_cache", [], |row| row.get(0))?;
        let overflow = row_count.saturating_sub(max_rows as i64);
        if overflow == 0 {
            return Ok(0);
        }
        let deleted = conn.execute(
            "DELETE FROM search_cache
             WHERE cache_key IN (
               SELECT cache_key FROM search_cache
               ORDER BY datetime(created_at) ASC, cache_key ASC
               LIMIT ?1
             )",
            [overflow],
        )?;
        Ok(deleted)
    })
}

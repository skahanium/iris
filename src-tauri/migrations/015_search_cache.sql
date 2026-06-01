CREATE TABLE IF NOT EXISTS search_cache (
    cache_key    TEXT PRIMARY KEY,
    query_hash   TEXT NOT NULL,
    backend      TEXT NOT NULL,
    body         TEXT NOT NULL,
    created_at   TEXT NOT NULL,
    expires_at   TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_search_cache_expires ON search_cache(expires_at);

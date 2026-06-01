CREATE TABLE IF NOT EXISTS web_page_cache (
    url_hash     TEXT PRIMARY KEY,
    url          TEXT NOT NULL,
    title        TEXT,
    body_text    TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    fetched_at   TEXT NOT NULL,
    expires_at   TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_web_page_cache_expires ON web_page_cache(expires_at);

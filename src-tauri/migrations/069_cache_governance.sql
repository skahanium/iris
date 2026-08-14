-- 069_cache_governance.sql — 可重建内容的访问时间、RSS 媒体状态与离线固定标记。

ALTER TABLE web_page_cache ADD COLUMN last_accessed_at TEXT;
UPDATE web_page_cache SET last_accessed_at = fetched_at WHERE last_accessed_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_web_page_cache_last_accessed
    ON web_page_cache(last_accessed_at);

ALTER TABLE feed_items ADD COLUMN offline_media_at TEXT;

CREATE TABLE IF NOT EXISTS feed_media (
    item_id TEXT NOT NULL REFERENCES feed_items(id) ON DELETE CASCADE,
    source_url_hash TEXT NOT NULL,
    media_kind TEXT NOT NULL CHECK (media_kind IN ('image', 'document')),
    cache_key TEXT NOT NULL,
    size_bytes INTEGER,
    state TEXT NOT NULL DEFAULT 'unknown' CHECK (state IN ('unknown', 'ready', 'failed')),
    failure_count INTEGER NOT NULL DEFAULT 0,
    retry_after TEXT,
    last_accessed_at TEXT,
    offline_pinned_at TEXT,
    PRIMARY KEY (item_id, source_url_hash)
);

CREATE INDEX IF NOT EXISTS idx_feed_media_cache_key ON feed_media(cache_key);
CREATE INDEX IF NOT EXISTS idx_feed_media_reclaim ON feed_media(offline_pinned_at, last_accessed_at);

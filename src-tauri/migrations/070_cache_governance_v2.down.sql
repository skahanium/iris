DROP INDEX IF EXISTS idx_feed_media_kind_cache_key;
DROP INDEX IF EXISTS idx_feed_media_state_access;

ALTER TABLE web_page_cache ADD COLUMN url TEXT NOT NULL DEFAULT '';
ALTER TABLE web_page_cache ADD COLUMN content_hash TEXT NOT NULL DEFAULT '';

ALTER TABLE feed_items ADD COLUMN offline_media_at TEXT;
ALTER TABLE feed_media ADD COLUMN offline_pinned_at TEXT;

CREATE INDEX IF NOT EXISTS idx_feed_media_reclaim
    ON feed_media(offline_pinned_at, last_accessed_at);

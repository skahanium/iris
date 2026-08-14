DROP INDEX IF EXISTS idx_feed_media_reclaim;
DROP INDEX IF EXISTS idx_feed_media_cache_key;
DROP TABLE IF EXISTS feed_media;
ALTER TABLE feed_items DROP COLUMN offline_media_at;
DROP INDEX IF EXISTS idx_web_page_cache_last_accessed;
ALTER TABLE web_page_cache DROP COLUMN last_accessed_at;

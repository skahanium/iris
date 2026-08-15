-- 070_cache_governance_v2.sql
-- Activate cache governance with only columns that have real producers/consumers.
-- Offline pinning was never implemented; remove its dead columns until a product
-- migration introduces the feature together with its command surface.

DROP INDEX IF EXISTS idx_feed_media_reclaim;
ALTER TABLE feed_media DROP COLUMN offline_pinned_at;
ALTER TABLE feed_items DROP COLUMN offline_media_at;

-- The web page cache never reads the stored URL or content_hash back. Keeping
-- them only persisted sensitive query strings with no governance benefit.
ALTER TABLE web_page_cache DROP COLUMN url;
ALTER TABLE web_page_cache DROP COLUMN content_hash;

CREATE INDEX IF NOT EXISTS idx_feed_media_state_access
    ON feed_media(state, last_accessed_at);
CREATE INDEX IF NOT EXISTS idx_feed_media_kind_cache_key
    ON feed_media(media_kind, cache_key);

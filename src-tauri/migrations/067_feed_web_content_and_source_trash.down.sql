-- 回滚前先恢复来源及仅因退订而软删除的文章，避免下层 schema 丢失可见数据。
UPDATE feed_items
SET deleted_at = NULL, purge_after = NULL, deletion_reason = NULL
WHERE deletion_reason = 'source_removed';
UPDATE feed_sources SET deleted_at = NULL, purge_after = NULL;

DROP INDEX IF EXISTS idx_feed_items_deletion_reason;
DROP INDEX IF EXISTS idx_feed_sources_trash;
DROP INDEX IF EXISTS idx_feed_sources_active_due;

ALTER TABLE feed_items DROP COLUMN primary_document_url;
ALTER TABLE feed_items DROP COLUMN primary_document_kind;
ALTER TABLE feed_items DROP COLUMN fulltext_extraction_version;
ALTER TABLE feed_items DROP COLUMN deletion_reason;
ALTER TABLE feed_sources DROP COLUMN purge_after;
ALTER TABLE feed_sources DROP COLUMN deleted_at;

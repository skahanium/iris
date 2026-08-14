-- 067_feed_web_content_and_source_trash.sql — 通用网页提取版本、主文档与来源软删除。
-- 仅扩展 RSS 应用状态；不触碰 Markdown、笔记索引或 Agent/RAG。

ALTER TABLE feed_sources ADD COLUMN deleted_at TEXT;
ALTER TABLE feed_sources ADD COLUMN purge_after TEXT;

ALTER TABLE feed_items ADD COLUMN deletion_reason TEXT
    CHECK (deletion_reason IS NULL OR deletion_reason IN ('retention', 'source_removed'));
ALTER TABLE feed_items ADD COLUMN fulltext_extraction_version INTEGER NOT NULL DEFAULT 0;
ALTER TABLE feed_items ADD COLUMN primary_document_kind TEXT
    CHECK (primary_document_kind IS NULL OR primary_document_kind = 'pdf');
ALTER TABLE feed_items ADD COLUMN primary_document_url TEXT;

CREATE INDEX idx_feed_sources_active_due
    ON feed_sources(deleted_at, is_enabled, next_fetch_at);
CREATE INDEX idx_feed_sources_trash
    ON feed_sources(deleted_at, purge_after) WHERE deleted_at IS NOT NULL;
CREATE INDEX idx_feed_items_deletion_reason
    ON feed_items(source_id, deletion_reason, deleted_at);

-- 旧网页正文保留到用户再次打开单篇文章；版本 0 会触发按需重取，
-- 不把历史文章静默加入后台队列。
UPDATE feed_items SET fulltext_extraction_version = 0
WHERE content_origin = 'web';

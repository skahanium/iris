-- 064_feed_retention_fulltext.sql — RSS 历史边界、可恢复清理与网页正文缓存。
-- 只扩展应用状态表；不触碰用户 Markdown、笔记回收站或 AI/RAG 数据。

ALTER TABLE feed_sources ADD COLUMN history_boundary_external_key TEXT;
ALTER TABLE feed_sources ADD COLUMN history_boundary_published_at TEXT;

ALTER TABLE feed_items ADD COLUMN expires_at TEXT;
ALTER TABLE feed_items ADD COLUMN deleted_at TEXT;
ALTER TABLE feed_items ADD COLUMN purge_after TEXT;
ALTER TABLE feed_items ADD COLUMN content_origin TEXT NOT NULL DEFAULT 'feed'
    CHECK (content_origin IN ('feed', 'web'));
ALTER TABLE feed_items ADD COLUMN fulltext_status TEXT NOT NULL DEFAULT 'not_requested'
    CHECK (fulltext_status IN ('not_requested', 'pending', 'fetching', 'ready', 'failed'));
ALTER TABLE feed_items ADD COLUMN fulltext_markdown TEXT;

CREATE INDEX idx_feed_items_retention
    ON feed_items(deleted_at, purge_after, expires_at);
CREATE INDEX idx_feed_items_fulltext_pending
    ON feed_items(source_id, fulltext_status, deleted_at)
    WHERE deleted_at IS NULL AND fulltext_status IN ('pending', 'fetching');

-- 为既有来源建立同一条历史边界。升级后即使上游仍返回完整归档，也只会
-- 检查边界之后的项目，避免每次同步重复扫描已软删除的旧历史。
WITH ranked AS (
    SELECT source_id, external_key, published_at,
           ROW_NUMBER() OVER (
               PARTITION BY source_id
               ORDER BY COALESCE(published_at, received_at) DESC, row_id DESC
           ) AS position
    FROM feed_items
)
UPDATE feed_sources
SET history_boundary_external_key = (
        SELECT external_key FROM ranked
        WHERE ranked.source_id = feed_sources.id AND position <= 50
        ORDER BY position DESC LIMIT 1
    ),
    history_boundary_published_at = (
        SELECT published_at FROM ranked
        WHERE ranked.source_id = feed_sources.id AND position <= 50
        ORDER BY position DESC LIMIT 1
    )
WHERE EXISTS (SELECT 1 FROM ranked WHERE ranked.source_id = feed_sources.id);

-- 现有完整历史源一次性收敛：保留每源最新 50 篇和全部收藏，其余先进入
-- RSS 回收站。软删除项不会被后续同步复活。
WITH ranked AS (
    SELECT row_id,
           ROW_NUMBER() OVER (
               PARTITION BY source_id
               ORDER BY COALESCE(published_at, received_at) DESC, row_id DESC
           ) AS position
    FROM feed_items
)
UPDATE feed_items
SET deleted_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
    purge_after = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '+30 days')
WHERE row_id IN (SELECT row_id FROM ranked WHERE position > 50)
  AND starred_at IS NULL;

UPDATE feed_items
SET expires_at = CASE
    WHEN starred_at IS NOT NULL THEN NULL
    WHEN archived_at IS NOT NULL THEN strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '+30 days')
    ELSE strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '+7 days')
END
WHERE deleted_at IS NULL;

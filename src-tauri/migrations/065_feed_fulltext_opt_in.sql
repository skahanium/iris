-- 065_feed_fulltext_opt_in.sql — 增加来源级网页正文补全开关。
-- 066 会将既有来源迁移为默认开启；新来源由 Repository 显式写入 1。

ALTER TABLE feed_sources ADD COLUMN fulltext_enabled INTEGER NOT NULL DEFAULT 0
    CHECK (fulltext_enabled IN (0, 1));

-- 如果开发版已执行过 064 的早期自动排队逻辑，关闭它而不丢弃已经成功
-- 提取的正文。只有用户随后在来源设置中明确开启时，新的摘要条目才会入队。
UPDATE feed_items
SET fulltext_status = 'not_requested'
WHERE fulltext_status IN ('pending', 'fetching');

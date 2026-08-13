-- 065_feed_fulltext_opt_in.sql — 网页正文补全改为来源级显式选择。
-- 普通 RSS 阅读默认只保存 Feed 给出的内容；不按域名或网站结构启用抓取。

ALTER TABLE feed_sources ADD COLUMN fulltext_enabled INTEGER NOT NULL DEFAULT 0
    CHECK (fulltext_enabled IN (0, 1));

-- 如果开发版已执行过 064 的早期自动排队逻辑，关闭它而不丢弃已经成功
-- 提取的正文。只有用户随后在来源设置中明确开启时，新的摘要条目才会入队。
UPDATE feed_items
SET fulltext_status = 'not_requested'
WHERE fulltext_status IN ('pending', 'fetching');

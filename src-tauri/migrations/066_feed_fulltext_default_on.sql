-- 066_feed_fulltext_default_on.sql — 网页正文补全改为默认阅读能力。
-- 升级仅打开来源的后续自动补全；既有文章继续保持 not_requested，直到用户
-- 实际打开某篇文章时才按需请求网页正文，避免静默批量抓取历史。

UPDATE feed_sources SET fulltext_enabled = 1;

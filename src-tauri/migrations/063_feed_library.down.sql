-- 063_feed_library.down.sql — 按 trigger → FTS → feed_items → feed_sources 顺序回滚，
-- 不修改任何其他表。

DROP TRIGGER IF EXISTS feed_items_fts_au;
DROP TRIGGER IF EXISTS feed_items_fts_ad;
DROP TRIGGER IF EXISTS feed_items_fts_ai;
DROP TABLE IF EXISTS feed_items_fts;
DROP TABLE IF EXISTS feed_items;
DROP TABLE IF EXISTS feed_sources;

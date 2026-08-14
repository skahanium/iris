-- 068_feed_image_authorization.sql — 单篇 RSS 图片显式授权。
-- 仅保存用户对当前文章内容的加载决定；图片二进制始终位于应用缓存，
-- 不进入 Vault、笔记索引或 Agent/RAG。

ALTER TABLE feed_items ADD COLUMN images_authorized_at TEXT;

CREATE INDEX idx_feed_items_images_authorized
    ON feed_items(images_authorized_at)
    WHERE images_authorized_at IS NOT NULL;

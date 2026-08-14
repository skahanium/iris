DROP INDEX IF EXISTS idx_feed_items_images_authorized;
ALTER TABLE feed_items DROP COLUMN images_authorized_at;

-- Down: remove files.is_locked column (SQLite 3.35+)
ALTER TABLE files DROP COLUMN is_locked;

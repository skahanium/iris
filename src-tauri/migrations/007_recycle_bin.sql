CREATE TABLE IF NOT EXISTS recycle_bin (
    id            TEXT PRIMARY KEY,
    original_path TEXT NOT NULL,
    title         TEXT NOT NULL,
    deleted_at    TEXT NOT NULL,
    expires_at    TEXT NOT NULL,
    trash_rel_dir TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_recycle_bin_expires ON recycle_bin(expires_at);

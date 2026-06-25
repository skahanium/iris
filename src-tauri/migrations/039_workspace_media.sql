-- 039: Workspace media index for fast Quick Open and navigator listing.
CREATE TABLE IF NOT EXISTS workspace_media (
    path       TEXT PRIMARY KEY,
    title      TEXT NOT NULL,
    media_kind TEXT NOT NULL,
    mime_type  TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_workspace_media_path
    ON workspace_media(path);
CREATE INDEX IF NOT EXISTS idx_workspace_media_updated_at
    ON workspace_media(updated_at);

-- 062 down: restore only the historical schemas so earlier migrations can be
-- rolled back. The duplicate graph and expired search-result cache contents
-- are intentionally not restored.

CREATE TABLE IF NOT EXISTS block_links (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    source_file_id     INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    source_anchor_key  TEXT,
    target_file_id     INTEGER REFERENCES files(id) ON DELETE CASCADE,
    target_anchor_key  TEXT,
    link_type          TEXT NOT NULL,
    confidence         REAL NOT NULL DEFAULT 1.0,
    is_confirmed       INTEGER NOT NULL DEFAULT 0,
    created_by         TEXT NOT NULL,
    context_hash       TEXT,
    created_at         TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_block_links_source ON block_links(source_file_id);
CREATE INDEX IF NOT EXISTS idx_block_links_target ON block_links(target_file_id);
CREATE INDEX IF NOT EXISTS idx_block_links_type ON block_links(link_type);

CREATE TABLE IF NOT EXISTS search_cache (
    cache_key            TEXT PRIMARY KEY,
    query_hash           TEXT NOT NULL,
    backend              TEXT NOT NULL,
    body                 TEXT NOT NULL,
    created_at           TEXT NOT NULL,
    expires_at           TEXT NOT NULL,
    vault_id             TEXT,
    provider_id          TEXT NOT NULL DEFAULT 'native.default',
    provider_kind        TEXT NOT NULL DEFAULT 'native',
    provider_config_hash TEXT NOT NULL DEFAULT 'legacy',
    broker_version       TEXT NOT NULL DEFAULT 'legacy'
);

CREATE INDEX IF NOT EXISTS idx_search_cache_expires ON search_cache(expires_at);
CREATE INDEX IF NOT EXISTS idx_search_cache_vault_id ON search_cache(vault_id);

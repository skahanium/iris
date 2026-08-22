-- 071 down: restore UNIQUE(key). Rows with the same key across scopes are
-- deduplicated to the first inserted row; this is the pre-071 schema contract.

CREATE TABLE IF NOT EXISTS ai_memories_old (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    key TEXT NOT NULL UNIQUE,
    content TEXT NOT NULL,
    scope TEXT NOT NULL DEFAULT 'global',
    source TEXT NOT NULL DEFAULT 'user_confirmed',
    vault_id TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT OR IGNORE INTO ai_memories_old (id, key, content, scope, source, vault_id, created_at, updated_at)
SELECT id, key, content, scope, source, vault_id, created_at, updated_at
FROM ai_memories;

DROP TABLE ai_memories;
ALTER TABLE ai_memories_old RENAME TO ai_memories;

CREATE INDEX IF NOT EXISTS idx_ai_memories_vault_id ON ai_memories(vault_id);

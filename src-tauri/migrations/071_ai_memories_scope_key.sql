-- 071: ai_memories unique constraint becomes (scope, key).
-- Existing rows keep their ids and metadata; duplicates that were previously
-- impossible under UNIQUE(key) are not introduced.

CREATE TABLE IF NOT EXISTS ai_memories_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    key TEXT NOT NULL,
    content TEXT NOT NULL,
    scope TEXT NOT NULL DEFAULT 'global',
    source TEXT NOT NULL DEFAULT 'user_confirmed',
    vault_id TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(scope, key)
);

INSERT INTO ai_memories_new (id, key, content, scope, source, vault_id, created_at, updated_at)
SELECT id, key, content, scope, source, vault_id, created_at, updated_at
FROM ai_memories;

DROP TABLE ai_memories;
ALTER TABLE ai_memories_new RENAME TO ai_memories;

CREATE INDEX IF NOT EXISTS idx_ai_memories_vault_id ON ai_memories(vault_id);

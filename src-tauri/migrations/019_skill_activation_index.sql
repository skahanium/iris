-- Skill activation index: keyword cache for fast skill matching.
-- When sqlite-vec is available, a best-effort vec_skill_descriptions
-- virtual table can be created for vector reranking.
CREATE TABLE IF NOT EXISTS skill_activation_index (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    skill_name TEXT NOT NULL,
    scope TEXT NOT NULL CHECK (scope IN ('Global', 'Vault')),
    description TEXT,
    keywords TEXT,
    embedding_json TEXT,
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(skill_name, scope)
);

CREATE INDEX IF NOT EXISTS idx_skill_activation_name
    ON skill_activation_index(skill_name);

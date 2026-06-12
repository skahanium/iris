-- Phase4 skill closed-loop diagnostics and per-skill runtime storage.
CREATE TABLE IF NOT EXISTS skill_diagnostics (
    skill_name TEXT NOT NULL,
    scope TEXT NOT NULL CHECK (scope IN ('Global', 'Vault')),
    last_matched_at TEXT,
    last_used_at TEXT,
    last_activation_score REAL,
    last_blocked_reason TEXT,
    last_resource_status TEXT,
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (skill_name, scope)
);

CREATE TABLE IF NOT EXISTS skill_storage (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    skill_name TEXT NOT NULL,
    scope TEXT NOT NULL CHECK (scope IN ('Global', 'Vault')),
    storage_key TEXT NOT NULL,
    content_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(skill_name, scope, storage_key)
);

CREATE INDEX IF NOT EXISTS idx_skill_diagnostics_updated
    ON skill_diagnostics(updated_at);

CREATE INDEX IF NOT EXISTS idx_skill_storage_skill
    ON skill_storage(skill_name, scope);

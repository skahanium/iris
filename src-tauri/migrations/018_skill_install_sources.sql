-- Skill install sources: tracks where each skill was installed from.
CREATE TABLE IF NOT EXISTS skill_install_sources (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    skill_name TEXT NOT NULL,
    scope TEXT NOT NULL CHECK (scope IN ('Global', 'Vault')),
    source_type TEXT NOT NULL CHECK (source_type IN ('url', 'git', 'local')),
    source_url TEXT,
    installed_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(skill_name, scope)
);

CREATE INDEX IF NOT EXISTS idx_skill_install_sources_name
    ON skill_install_sources(skill_name);

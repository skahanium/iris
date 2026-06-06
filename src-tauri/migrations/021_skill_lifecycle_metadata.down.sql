DROP TABLE IF EXISTS scheduled_tasks;
DROP TABLE IF EXISTS ai_memories;

CREATE TABLE IF NOT EXISTS skill_install_sources_rollback (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    skill_name TEXT NOT NULL,
    scope TEXT NOT NULL CHECK (scope IN ('Global', 'Vault')),
    source_type TEXT NOT NULL CHECK (source_type IN ('url', 'git', 'local')),
    source_url TEXT,
    installed_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(skill_name, scope)
);

INSERT INTO skill_install_sources_rollback (id, skill_name, scope, source_type, source_url, installed_at)
SELECT id, skill_name, scope, source_type, source_url, installed_at
FROM skill_install_sources;

DROP TABLE skill_install_sources;
ALTER TABLE skill_install_sources_rollback RENAME TO skill_install_sources;

CREATE INDEX IF NOT EXISTS idx_skill_install_sources_name
    ON skill_install_sources(skill_name);

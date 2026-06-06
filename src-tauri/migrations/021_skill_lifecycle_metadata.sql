-- Skill lifecycle metadata: update tracking and installed content hash.
ALTER TABLE skill_install_sources ADD COLUMN updated_at TEXT;
ALTER TABLE skill_install_sources ADD COLUMN content_hash TEXT;
ALTER TABLE skill_install_sources ADD COLUMN version_ref TEXT;

UPDATE skill_install_sources
SET updated_at = COALESCE(updated_at, installed_at);

CREATE TABLE IF NOT EXISTS ai_memories (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    key TEXT NOT NULL UNIQUE,
    content TEXT NOT NULL,
    scope TEXT NOT NULL DEFAULT 'global',
    source TEXT NOT NULL DEFAULT 'user_confirmed',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS scheduled_tasks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    title TEXT NOT NULL,
    prompt TEXT NOT NULL,
    schedule TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

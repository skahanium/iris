-- 035: Skill trust profiles.
-- Stores bounded install-time trust metadata only. Raw SKILL.md content stays on disk.

CREATE TABLE IF NOT EXISTS skill_trust_profiles (
    skill_name                    TEXT NOT NULL,
    scope                         TEXT NOT NULL CHECK(scope IN ('Global', 'Vault')),
    source_type                   TEXT NOT NULL CHECK(source_type IN ('registry', 'git', 'url', 'local')),
    source_url                    TEXT,
    integrity_hash                TEXT,
    declared_capabilities_json    TEXT NOT NULL DEFAULT '[]',
    requested_tools_json          TEXT NOT NULL DEFAULT '[]',
    risk_level                    TEXT NOT NULL CHECK(risk_level IN ('low', 'medium', 'high')),
    high_risk                     INTEGER NOT NULL DEFAULT 0,
    sha256_locked                 INTEGER NOT NULL DEFAULT 0,
    allowed_tools_narrowing_only  INTEGER NOT NULL DEFAULT 1,
    warnings_json                 TEXT NOT NULL DEFAULT '[]',
    created_at                    TEXT NOT NULL,
    updated_at                    TEXT NOT NULL,
    PRIMARY KEY(skill_name, scope, source_type)
);

CREATE INDEX IF NOT EXISTS idx_skill_trust_profiles_risk
    ON skill_trust_profiles(scope, risk_level, updated_at);

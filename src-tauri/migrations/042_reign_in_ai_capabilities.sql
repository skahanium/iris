DROP TABLE IF EXISTS skill_runtime_requirements;
DROP TABLE IF EXISTS skill_storage;
DROP TABLE IF EXISTS skill_diagnostics;
DROP TABLE IF EXISTS skill_trust_profiles;
DROP TABLE IF EXISTS skill_install_sources;
DROP TABLE IF EXISTS mcp_health_events;
DROP TABLE IF EXISTS mcp_tool_inventory;
DROP TABLE IF EXISTS mcp_runtime_profiles;
DROP TABLE IF EXISTS mcp_server_catalog;

CREATE TABLE IF NOT EXISTS web_evidence_providers (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('native', 'mcp')),
    enabled INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
    transport_kind TEXT NOT NULL CHECK (transport_kind IN ('native', 'stdio', 'https')),
    transport_config_json TEXT NOT NULL DEFAULT '{}',
    credential_refs_json TEXT NOT NULL DEFAULT '{}',
    web_search_mapping_json TEXT,
    web_fetch_mapping_json TEXT,
    provider_config_hash TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_web_evidence_providers_enabled
    ON web_evidence_providers(enabled, kind);

ALTER TABLE search_cache ADD COLUMN provider_id TEXT NOT NULL DEFAULT 'native.default';
ALTER TABLE search_cache ADD COLUMN provider_kind TEXT NOT NULL DEFAULT 'native';
ALTER TABLE search_cache ADD COLUMN provider_config_hash TEXT NOT NULL DEFAULT 'legacy';
ALTER TABLE search_cache ADD COLUMN broker_version TEXT NOT NULL DEFAULT 'legacy';

ALTER TABLE web_page_cache ADD COLUMN provider_id TEXT NOT NULL DEFAULT 'native.fetch';
ALTER TABLE web_page_cache ADD COLUMN provider_kind TEXT NOT NULL DEFAULT 'native';
ALTER TABLE web_page_cache ADD COLUMN provider_config_hash TEXT NOT NULL DEFAULT 'legacy';
ALTER TABLE web_page_cache ADD COLUMN broker_version TEXT NOT NULL DEFAULT 'legacy';

ALTER TABLE session_evidence ADD COLUMN provider_id TEXT;
ALTER TABLE session_evidence ADD COLUMN provider_kind TEXT;
ALTER TABLE session_evidence ADD COLUMN raw_result_hash TEXT;
ALTER TABLE session_evidence ADD COLUMN extraction_method TEXT;
ALTER TABLE session_evidence ADD COLUMN conflict_group TEXT;
ALTER TABLE session_evidence ADD COLUMN conflict_note TEXT;

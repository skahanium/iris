CREATE TABLE IF NOT EXISTS mcp_server_catalog (
    id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    transport TEXT NOT NULL CHECK (transport IN ('stdio', 'https', 'sse')),
    command TEXT,
    args_json TEXT NOT NULL DEFAULT '[]',
    url TEXT,
    env_schema_json TEXT NOT NULL DEFAULT '{}',
    capability_tags_json TEXT NOT NULL DEFAULT '[]',
    source TEXT NOT NULL DEFAULT 'user',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS mcp_runtime_profiles (
    id TEXT PRIMARY KEY,
    server_id TEXT NOT NULL,
    vault_scope_hash TEXT,
    display_name TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
    transport_config_json TEXT NOT NULL DEFAULT '{}',
    env_bindings_json TEXT NOT NULL DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'unknown' CHECK (status IN ('unknown', 'ready', 'degraded', 'unavailable', 'blocked')),
    last_checked_at TEXT,
    last_error TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS mcp_tool_inventory (
    profile_id TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    schema_hash TEXT NOT NULL,
    capability_mapping_json TEXT NOT NULL DEFAULT '[]',
    description TEXT,
    last_discovered_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (profile_id, tool_name)
);

CREATE TABLE IF NOT EXISTS mcp_health_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    profile_id TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('unknown', 'ready', 'degraded', 'unavailable', 'blocked')),
    reason_code TEXT NOT NULL,
    message TEXT,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS skill_runtime_requirements (
    skill_name TEXT NOT NULL,
    scope TEXT NOT NULL CHECK (scope IN ('Global', 'Vault')),
    manifest_hash TEXT,
    kind TEXT NOT NULL,
    runtime_kind TEXT NOT NULL CHECK (runtime_kind IN ('not_applicable', 'mcp', 'unavailable')),
    required_profiles_json TEXT NOT NULL DEFAULT '[]',
    required_capabilities_json TEXT NOT NULL DEFAULT '[]',
    workspace_contract_json TEXT NOT NULL DEFAULT '{}',
    degradation_policy_json TEXT NOT NULL DEFAULT '{}',
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (skill_name, scope)
);

DROP TABLE IF EXISTS web_evidence_providers;

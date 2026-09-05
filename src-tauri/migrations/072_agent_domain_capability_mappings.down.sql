-- 072 down: restore the 059 read-only MCP schema.
--
-- Domain rows are removed first, then only external.read bindings/snapshots are
-- copied back into the 059 column set so the old CHECK cannot be violated.

PRAGMA foreign_keys=OFF;

DELETE FROM mcp_capability_bindings
WHERE capability = 'web.domain.read';

DELETE FROM agent_run_mcp_tool_snapshots
WHERE capability = 'web.domain.read';

CREATE TABLE IF NOT EXISTS mcp_capability_bindings_059 (
    id                      TEXT PRIMARY KEY,
    provider_id             TEXT NOT NULL REFERENCES web_evidence_providers(id) ON DELETE CASCADE,
    exposed_name            TEXT NOT NULL UNIQUE,
    mcp_tool_name           TEXT NOT NULL,
    input_schema_json       TEXT NOT NULL,
    argument_mapping_json   TEXT NOT NULL,
    output_policy_json      TEXT NOT NULL,
    capability              TEXT NOT NULL CHECK (capability = 'external.read'),
    risk_class              TEXT NOT NULL CHECK (risk_class = 'read_only'),
    read_only               INTEGER NOT NULL CHECK (read_only = 1),
    user_trusted            INTEGER NOT NULL CHECK (user_trusted = 1),
    provider_config_hash    TEXT NOT NULL,
    binding_config_hash     TEXT NOT NULL,
    created_at              TEXT NOT NULL,
    updated_at              TEXT NOT NULL,
    UNIQUE(provider_id, mcp_tool_name)
);

INSERT INTO mcp_capability_bindings_059
    (id, provider_id, exposed_name, mcp_tool_name, input_schema_json,
     argument_mapping_json, output_policy_json, capability, risk_class,
     read_only, user_trusted, provider_config_hash, binding_config_hash,
     created_at, updated_at)
SELECT
    id, provider_id, exposed_name, mcp_tool_name, input_schema_json,
    argument_mapping_json, output_policy_json, capability, risk_class,
    read_only, user_trusted, provider_config_hash, binding_config_hash,
    created_at, updated_at
FROM mcp_capability_bindings
WHERE capability = 'external.read';

DROP TABLE mcp_capability_bindings;
ALTER TABLE mcp_capability_bindings_059 RENAME TO mcp_capability_bindings;

CREATE INDEX IF NOT EXISTS idx_mcp_capability_bindings_provider
    ON mcp_capability_bindings(provider_id, updated_at);

CREATE TABLE IF NOT EXISTS agent_run_mcp_tool_snapshots_059 (
    run_id                  TEXT NOT NULL REFERENCES agent_runs(run_id) ON DELETE CASCADE,
    binding_id              TEXT NOT NULL,
    provider_id             TEXT NOT NULL,
    exposed_name            TEXT NOT NULL,
    mcp_tool_name           TEXT NOT NULL,
    input_schema_json       TEXT NOT NULL,
    argument_mapping_json   TEXT NOT NULL,
    output_policy_json      TEXT NOT NULL,
    capability              TEXT NOT NULL CHECK (capability = 'external.read'),
    risk_class              TEXT NOT NULL CHECK (risk_class = 'read_only'),
    read_only               INTEGER NOT NULL CHECK (read_only = 1),
    user_trusted            INTEGER NOT NULL CHECK (user_trusted = 1),
    provider_config_hash    TEXT NOT NULL,
    provider_launch_hash    TEXT NOT NULL,
    transport_kind          TEXT NOT NULL CHECK (transport_kind IN ('stdio', 'https')),
    transport_config_json   TEXT NOT NULL,
    credential_refs_json    TEXT NOT NULL,
    binding_config_hash     TEXT NOT NULL,
    frozen_at               TEXT NOT NULL,
    snapshot_integrity_hash TEXT NOT NULL,
    PRIMARY KEY (run_id, binding_id),
    UNIQUE(run_id, exposed_name)
);

INSERT INTO agent_run_mcp_tool_snapshots_059
    (run_id, binding_id, provider_id, exposed_name, mcp_tool_name,
     input_schema_json, argument_mapping_json, output_policy_json, capability,
     risk_class, read_only, user_trusted, provider_config_hash,
     provider_launch_hash, transport_kind, transport_config_json,
     credential_refs_json, binding_config_hash, frozen_at,
     snapshot_integrity_hash)
SELECT
    run_id, binding_id, provider_id, exposed_name, mcp_tool_name,
    input_schema_json, argument_mapping_json, output_policy_json, capability,
    risk_class, read_only, user_trusted, provider_config_hash,
    provider_launch_hash, transport_kind, transport_config_json,
    credential_refs_json, binding_config_hash, frozen_at,
    snapshot_integrity_hash
FROM agent_run_mcp_tool_snapshots
WHERE capability = 'external.read';

DROP TABLE agent_run_mcp_tool_snapshots;
ALTER TABLE agent_run_mcp_tool_snapshots_059 RENAME TO agent_run_mcp_tool_snapshots;

CREATE INDEX IF NOT EXISTS idx_agent_run_mcp_tool_snapshots_run
    ON agent_run_mcp_tool_snapshots(run_id, exposed_name);

PRAGMA foreign_keys=ON;

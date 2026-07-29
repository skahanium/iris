-- 059: explicit, per-Run read-only MCP tool bindings.
--
-- Provider enablement and config hash remain live only as a fail-closed
-- revocation check. Dispatch consumes the frozen transport/config/credential
-- references, schema, mapping and output policy accepted for that Run and is
-- intentionally not linked back to the mutable binding with a foreign key.

CREATE TABLE IF NOT EXISTS mcp_capability_bindings (
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

CREATE INDEX IF NOT EXISTS idx_mcp_capability_bindings_provider
    ON mcp_capability_bindings(provider_id, updated_at);

CREATE TABLE IF NOT EXISTS agent_run_mcp_tool_snapshots (
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
    PRIMARY KEY (run_id, binding_id),
    UNIQUE(run_id, exposed_name)
);

CREATE INDEX IF NOT EXISTS idx_agent_run_mcp_tool_snapshots_run
    ON agent_run_mcp_tool_snapshots(run_id, exposed_name);

ALTER TABLE agent_run_evidence RENAME TO agent_run_evidence_058;

CREATE TABLE agent_run_evidence (
    run_id              TEXT NOT NULL REFERENCES agent_runs(run_id) ON DELETE CASCADE,
    evidence_id         INTEGER NOT NULL REFERENCES session_evidence(id) ON DELETE CASCADE,
    registration_source TEXT NOT NULL CHECK (
        registration_source IN ('context', 'web_search', 'external_tool')
    ),
    registered_at       TEXT NOT NULL,
    PRIMARY KEY (run_id, evidence_id, registration_source)
);

INSERT INTO agent_run_evidence
    (run_id, evidence_id, registration_source, registered_at)
SELECT run_id, evidence_id, registration_source, registered_at
FROM agent_run_evidence_058;

DROP TABLE agent_run_evidence_058;

CREATE INDEX idx_agent_run_evidence_web_verification
    ON agent_run_evidence(run_id, registration_source, evidence_id);

DROP INDEX IF EXISTS idx_agent_run_evidence_web_verification;

DELETE FROM session_evidence
WHERE extraction_method = 'mcp_tool_output_v1';

ALTER TABLE agent_run_evidence RENAME TO agent_run_evidence_059;

CREATE TABLE agent_run_evidence (
    run_id              TEXT NOT NULL REFERENCES agent_runs(run_id) ON DELETE CASCADE,
    evidence_id         INTEGER NOT NULL REFERENCES session_evidence(id) ON DELETE CASCADE,
    registration_source TEXT NOT NULL CHECK (
        registration_source IN ('context', 'web_search')
    ),
    registered_at       TEXT NOT NULL,
    PRIMARY KEY (run_id, evidence_id, registration_source)
);

INSERT INTO agent_run_evidence
    (run_id, evidence_id, registration_source, registered_at)
SELECT run_id, evidence_id, registration_source, registered_at
FROM agent_run_evidence_059
WHERE registration_source IN ('context', 'web_search');

DROP TABLE agent_run_evidence_059;

CREATE INDEX idx_agent_run_evidence_web_verification
    ON agent_run_evidence(run_id, registration_source, evidence_id);

DROP INDEX IF EXISTS idx_agent_run_mcp_tool_snapshots_run;
DROP TABLE IF EXISTS agent_run_mcp_tool_snapshots;

DROP INDEX IF EXISTS idx_mcp_capability_bindings_provider;
DROP TABLE IF EXISTS mcp_capability_bindings;

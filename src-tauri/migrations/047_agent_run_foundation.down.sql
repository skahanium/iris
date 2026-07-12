DROP INDEX IF EXISTS idx_session_evidence_origin_run;

ALTER TABLE session_evidence DROP COLUMN bounded_excerpt;
ALTER TABLE session_evidence DROP COLUMN stale;
ALTER TABLE session_evidence DROP COLUMN material_role;
ALTER TABLE session_evidence DROP COLUMN origin_run_id;

ALTER TABLE session_messages DROP COLUMN citation_map_json;
ALTER TABLE session_messages DROP COLUMN evidence_refs_json;
ALTER TABLE session_messages DROP COLUMN explicit_references_json;
ALTER TABLE session_messages DROP COLUMN turn_id;

DROP INDEX IF EXISTS idx_agent_run_events_run;
DROP TABLE IF EXISTS agent_run_events;

DROP INDEX IF EXISTS idx_agent_run_steps_run;
DROP TABLE IF EXISTS agent_run_steps;

DROP INDEX IF EXISTS idx_agent_runs_status;
DROP INDEX IF EXISTS idx_agent_runs_session;
DROP TABLE IF EXISTS agent_runs;

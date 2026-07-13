-- 051: single-cutover schema for the unified Agent Run control plane.
-- The migration is executed by storage::migrate::apply_agent_harness_cutover so
-- ContextPacket metadata can be safely converted into evidence-ledger IDs before
-- this copy-transform-swap script removes legacy columns and tables.

CREATE TABLE sessions__cutover (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    session_key      TEXT NOT NULL UNIQUE,
    vault_id         TEXT,
    title            TEXT,
    retention_policy TEXT NOT NULL DEFAULT 'user_clearable',
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL
);

INSERT INTO sessions__cutover
    (id, session_key, vault_id, title, retention_policy, created_at, updated_at)
SELECT id, session_key, vault_id, title, retention_policy, created_at, updated_at
FROM sessions;

CREATE TABLE session_messages__cutover (
    id                       INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id               INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    seq                      INTEGER NOT NULL,
    role                     TEXT NOT NULL,
    content                  TEXT NOT NULL,
    content_parts            TEXT,
    tool_calls               JSON,
    turn_id                  TEXT,
    explicit_references_json TEXT,
    evidence_refs_json       TEXT,
    citation_map_json        TEXT,
    content_hash             TEXT,
    vault_id                 TEXT,
    created_at               TEXT NOT NULL,
    UNIQUE(session_id, seq)
);

INSERT INTO session_messages__cutover
    (id, session_id, seq, role, content, content_parts, tool_calls, turn_id,
     explicit_references_json, evidence_refs_json, citation_map_json,
     content_hash, vault_id, created_at)
SELECT id, session_id, seq, role, content, content_parts, tool_calls,
       COALESCE(turn_id, 'legacy-message:' || id),
       explicit_references_json,
       COALESCE(evidence_refs_json, '[]'),
       citation_map_json,
       content_hash, vault_id, created_at
FROM session_messages;

DROP TABLE session_messages;
DROP TABLE sessions;
ALTER TABLE sessions__cutover RENAME TO sessions;
ALTER TABLE session_messages__cutover RENAME TO session_messages;

CREATE TABLE agent_harness_cutover_map (
    legacy_request_id TEXT PRIMARY KEY,
    run_id            TEXT NOT NULL UNIQUE,
    client_request_id TEXT NOT NULL UNIQUE,
    session_id        INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    legacy_kind       TEXT NOT NULL
);

INSERT INTO agent_harness_cutover_map
    (legacy_request_id, run_id, client_request_id, session_id, legacy_kind)
SELECT task.request_id,
       'legacy-task:' || task.task_id,
       'legacy-task:' || task.task_id,
       task.session_id,
       'task'
FROM agent_tasks AS task;

INSERT INTO sessions
    (session_key, retention_policy, created_at, updated_at)
SELECT 'legacy-trace:' || trace.request_id,
       'user_clearable',
       trace.created_at,
       COALESCE(trace.created_at, datetime('now'))
FROM ai_traces AS trace
WHERE NOT EXISTS (
    SELECT 1 FROM agent_harness_cutover_map AS map
    WHERE map.legacy_request_id = trace.request_id
)
ON CONFLICT(session_key) DO NOTHING;

INSERT INTO agent_harness_cutover_map
    (legacy_request_id, run_id, client_request_id, session_id, legacy_kind)
SELECT trace.request_id,
       'legacy-trace:' || trace.request_id,
       'legacy-trace:' || trace.request_id,
       session.id,
       'trace'
FROM ai_traces AS trace
JOIN sessions AS session
  ON session.session_key = 'legacy-trace:' || trace.request_id
WHERE NOT EXISTS (
    SELECT 1 FROM agent_harness_cutover_map AS map
    WHERE map.legacy_request_id = trace.request_id
);

INSERT INTO sessions
    (session_key, retention_policy, created_at, updated_at)
SELECT 'legacy-audit:' || request_id,
       'user_clearable',
       datetime('now'),
       datetime('now')
FROM (
    SELECT request_id FROM tool_audit
    UNION
    SELECT request_id FROM agent_permission_audit
)
WHERE NOT EXISTS (
    SELECT 1 FROM agent_harness_cutover_map AS map
    WHERE map.legacy_request_id = request_id
)
ON CONFLICT(session_key) DO NOTHING;

INSERT INTO agent_harness_cutover_map
    (legacy_request_id, run_id, client_request_id, session_id, legacy_kind)
SELECT orphan.request_id,
       'legacy-audit:' || orphan.request_id,
       'legacy-audit:' || orphan.request_id,
       session.id,
       'audit'
FROM (
    SELECT request_id FROM tool_audit
    UNION
    SELECT request_id FROM agent_permission_audit
) AS orphan
JOIN sessions AS session
  ON session.session_key = 'legacy-audit:' || orphan.request_id
WHERE NOT EXISTS (
    SELECT 1 FROM agent_harness_cutover_map AS map
    WHERE map.legacy_request_id = orphan.request_id
);

INSERT OR IGNORE INTO agent_runs
    (run_id, client_request_id, session_id, turn_id, status, state_version,
     effect, effort, security_domain, risk, envelope_json, goal_summary,
     budget_policy_json, provider_route_summary_json, stage_metrics_json,
     token_input, token_output, error_code, safe_error_message,
     created_at, updated_at, completed_at, explicit_action_json)
SELECT map.run_id,
       map.client_request_id,
       map.session_id,
       'legacy-turn:' || map.run_id,
       CASE
           WHEN task.status = 'completed' THEN 'completed'
           WHEN task.status IN ('failed', 'error') THEN 'failed'
           WHEN task.status IN ('cancelled', 'canceled') THEN 'cancelled'
           ELSE 'cancelled'
       END,
       2,
       'answer',
       'durable',
       'normal',
       'read_only',
       '{"legacy":true}',
       task.user_goal_summary,
       task.budget_policy_json,
       json_object('legacyTraceRequestId', task.request_id, 'provider', trace.provider, 'modelSlot', trace.model_slot, 'latencyMs', trace.latency_ms),
       json_object('legacyTraceStatus', trace.status, 'legacyToolNames', trace.tool_names, 'legacyPacketIds', trace.packet_ids),
       COALESCE(trace.token_input, 0),
       COALESCE(trace.token_output, 0),
       CASE
           WHEN task.status IN ('running', 'paused', 'awaiting', 'awaiting_confirmation')
             THEN 'cancelled_legacy'
           ELSE task.error_code
       END,
       CASE
           WHEN task.status IN ('running', 'paused', 'awaiting', 'awaiting_confirmation')
             THEN 'Legacy task safely cancelled during cutover'
           WHEN task.status IN ('failed', 'error')
             THEN '旧版任务执行失败'
           ELSE NULL
       END,
       task.created_at,
       task.updated_at,
       CASE
           WHEN task.status IN ('running', 'paused', 'awaiting', 'awaiting_confirmation')
             THEN task.updated_at
           ELSE task.completed_at
       END,
       NULL
FROM agent_tasks AS task
JOIN agent_harness_cutover_map AS map
  ON map.legacy_request_id = task.request_id
LEFT JOIN ai_traces AS trace
  ON trace.request_id = task.request_id;

INSERT OR IGNORE INTO agent_runs
    (run_id, client_request_id, session_id, turn_id, status, state_version,
     effect, effort, security_domain, risk, envelope_json, goal_summary,
     budget_policy_json, provider_route_summary_json, stage_metrics_json,
     token_input, token_output, error_code, safe_error_message,
     created_at, updated_at, completed_at, explicit_action_json)
SELECT map.run_id,
       map.client_request_id,
       map.session_id,
       'legacy-turn:' || map.run_id,
       CASE
           WHEN trace.status = 'completed' THEN 'completed'
           WHEN trace.status IN ('failed', 'error') THEN 'failed'
           ELSE 'cancelled'
       END,
       2,
       'answer',
       'durable',
       'normal',
       'read_only',
       '{"legacy":true}',
       '',
       '{}',
       json_object('legacyTraceRequestId', trace.request_id, 'provider', trace.provider, 'modelSlot', trace.model_slot, 'latencyMs', trace.latency_ms),
       json_object('legacyTraceStatus', trace.status, 'legacyToolNames', trace.tool_names, 'legacyPacketIds', trace.packet_ids),
       COALESCE(trace.token_input, 0),
       COALESCE(trace.token_output, 0),
       CASE
           WHEN trace.status IN ('completed', 'failed', 'error') THEN trace.error_code
           ELSE 'cancelled_legacy'
       END,
       CASE
           WHEN trace.status IN ('failed', 'error') THEN '旧版请求执行失败'
           WHEN trace.status = 'completed' THEN NULL
           ELSE 'Legacy request safely cancelled during cutover'
       END,
       trace.created_at,
       trace.created_at,
       trace.created_at,
       NULL
FROM ai_traces AS trace
JOIN agent_harness_cutover_map AS map
  ON map.legacy_request_id = trace.request_id
WHERE map.legacy_kind = 'trace';

INSERT OR IGNORE INTO agent_runs
    (run_id, client_request_id, session_id, turn_id, status, state_version,
     effect, effort, security_domain, risk, envelope_json, goal_summary,
     budget_policy_json, provider_route_summary_json, stage_metrics_json,
     token_input, token_output, error_code, safe_error_message,
     created_at, updated_at, completed_at, explicit_action_json)
SELECT map.run_id,
       map.client_request_id,
       map.session_id,
       'legacy-turn:' || map.run_id,
       'cancelled',
       2,
       'answer',
       'durable',
       'normal',
       'read_only',
       '{"legacy":true}',
       '',
       '{}',
       '{}',
       '{}',
       0, 0,
       'cancelled_legacy',
       'Legacy audit record safely archived',
       datetime('now'), datetime('now'), datetime('now'), NULL
FROM agent_harness_cutover_map AS map
WHERE map.legacy_kind = 'audit';

INSERT OR IGNORE INTO agent_run_steps
    (run_id, step_seq, kind, status, input_summary, output_summary,
     resume_state_json, evidence_refs_json, created_at, updated_at)
SELECT map.run_id,
       step.step_seq,
       step.kind,
       CASE
           WHEN task.status IN ('running', 'paused', 'awaiting', 'awaiting_confirmation')
             THEN 'cancelled'
           ELSE step.status
       END,
       substr(step.input_summary, 1, 500),
       substr(step.output_summary, 1, 500),
       '{}',
       '[]',
       step.created_at,
       step.updated_at
FROM agent_task_steps AS step
JOIN agent_tasks AS task ON task.task_id = step.task_id
JOIN agent_harness_cutover_map AS map ON map.legacy_request_id = task.request_id;

INSERT OR IGNORE INTO agent_run_events
    (run_id, event_seq, state_version, event_type, payload_json, created_at)
SELECT run.run_id,
       1,
       0,
       'accepted',
       '{"kind":"accepted","turnId":"' || replace(run.turn_id, '"', '') ||
       '","sessionKey":"' || replace(session.session_key, '"', '') || '"}',
       run.created_at
FROM agent_runs AS run
JOIN sessions AS session ON session.id = run.session_id
WHERE run.run_id IN (SELECT run_id FROM agent_harness_cutover_map);

INSERT OR IGNORE INTO agent_run_events
    (run_id, event_seq, state_version, event_type, payload_json, created_at)
SELECT run.run_id,
       2,
       1,
       'stage_changed',
       CASE
           WHEN EXISTS (
               SELECT 1 FROM writing_states AS writing
               WHERE writing.request_id = map.legacy_request_id
           )
           OR EXISTS (
               SELECT 1 FROM research_states AS research
               WHERE research.request_id = map.legacy_request_id
           )
           OR EXISTS (
               SELECT 1 FROM deliberation_states AS deliberation
               WHERE deliberation.request_id = map.legacy_request_id
           )
             THEN '{"kind":"stage_changed","state":"verifying","stage":"历史写作、研究或审议状态已归档"}'
           ELSE '{"kind":"stage_changed","state":"verifying","stage":"历史执行状态已归档"}'
       END,
       run.created_at
FROM agent_runs AS run
JOIN agent_harness_cutover_map AS map ON map.run_id = run.run_id;

INSERT OR IGNORE INTO agent_run_events
    (run_id, event_seq, state_version, event_type, payload_json, created_at)
SELECT run_id,
       3,
       2,
       CASE status
           WHEN 'completed' THEN 'completed'
           WHEN 'failed' THEN 'failed'
           ELSE 'cancelled'
       END,
       CASE status
           WHEN 'completed' THEN '{"kind":"completed","messageId":null}'
           WHEN 'failed' THEN '{"kind":"failed","code":"agent_run_persistence_failed","message":"旧版任务执行失败"}'
           ELSE '{"kind":"cancelled","reason":"旧版未完成任务已安全取消"}'
       END,
       COALESCE(completed_at, updated_at)
FROM agent_runs
WHERE run_id IN (SELECT run_id FROM agent_harness_cutover_map);

CREATE TABLE tool_audit__cutover (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id            TEXT NOT NULL REFERENCES agent_runs(run_id) ON DELETE CASCADE,
    run_step          INTEGER NOT NULL,
    tool_name         TEXT NOT NULL,
    arguments_summary TEXT,
    result_summary    TEXT,
    success           INTEGER NOT NULL DEFAULT 0,
    duration_ms       INTEGER,
    subagent_depth    INTEGER NOT NULL DEFAULT 0,
    created_at        TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO tool_audit__cutover
    (id, run_id, run_step, tool_name, arguments_summary, result_summary,
     success, duration_ms, subagent_depth, created_at)
SELECT audit.id, map.run_id, audit.harness_round, audit.tool_name,
       audit.arguments_summary, audit.result_summary, audit.success,
       audit.duration_ms, audit.subagent_depth, audit.created_at
FROM tool_audit AS audit
JOIN agent_harness_cutover_map AS map ON map.legacy_request_id = audit.request_id;

CREATE TABLE agent_permission_audit__cutover (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id          TEXT NOT NULL REFERENCES agent_runs(run_id) ON DELETE CASCADE,
    skill_id        TEXT,
    tool_name       TEXT NOT NULL,
    permission_name TEXT NOT NULL,
    decision        TEXT NOT NULL,
    scope_summary   TEXT NOT NULL,
    risk_level      TEXT NOT NULL,
    result_status   TEXT NOT NULL,
    created_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO agent_permission_audit__cutover
    (id, run_id, skill_id, tool_name, permission_name, decision,
     scope_summary, risk_level, result_status, created_at)
SELECT audit.id, map.run_id, audit.skill_id, audit.tool_name,
       audit.permission_name, audit.decision, audit.scope_summary,
       audit.risk_level, audit.result_status, audit.created_at
FROM agent_permission_audit AS audit
JOIN agent_harness_cutover_map AS map ON map.legacy_request_id = audit.request_id;

DROP TABLE agent_permission_audit;
DROP TABLE tool_audit;
DROP TABLE agent_task_events;
DROP TABLE agent_task_steps;
DROP TABLE agent_tasks;
DROP TABLE deliberation_states;
DROP TABLE writing_states;
DROP TABLE research_states;
DROP TABLE ai_traces;

ALTER TABLE tool_audit__cutover RENAME TO tool_audit;
ALTER TABLE agent_permission_audit__cutover RENAME TO agent_permission_audit;

CREATE INDEX idx_sessions_vault_id ON sessions(vault_id);
CREATE INDEX idx_sessions_updated_at ON sessions(updated_at);
CREATE INDEX idx_session_messages_session ON session_messages(session_id, seq);
CREATE INDEX idx_session_messages_vault_id ON session_messages(vault_id);
CREATE INDEX idx_tool_audit_run_id ON tool_audit(run_id);
CREATE INDEX idx_tool_audit_tool_name ON tool_audit(tool_name);
CREATE INDEX idx_agent_permission_audit_run ON agent_permission_audit(run_id);
CREATE INDEX idx_agent_permission_audit_permission ON agent_permission_audit(permission_name);
CREATE INDEX idx_agent_permission_audit_tool ON agent_permission_audit(tool_name);

DROP TABLE agent_harness_cutover_map;

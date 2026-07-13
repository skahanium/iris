-- 051 down: restore a legacy-readable schema without reactivating cancelled work.
CREATE TABLE sessions__legacy (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    session_key      TEXT NOT NULL UNIQUE,
    scene            TEXT NOT NULL DEFAULT 'legacy',
    note_path        TEXT,
    retention_policy TEXT NOT NULL DEFAULT 'user_clearable',
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL,
    title            TEXT,
    vault_id         TEXT
);

INSERT INTO sessions__legacy
    (id, session_key, scene, note_path, retention_policy, created_at, updated_at, title, vault_id)
SELECT id, session_key, 'legacy', NULL, retention_policy, created_at, updated_at, title, vault_id
FROM sessions;

CREATE TABLE session_messages__legacy (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id       INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    seq              INTEGER NOT NULL,
    role             TEXT NOT NULL,
    content          TEXT NOT NULL,
    tool_calls       JSON,
    content_hash     TEXT,
    created_at       TEXT NOT NULL,
    content_parts            TEXT,
    evidence_packets         TEXT,
    vault_id                 TEXT,
    turn_id                  TEXT,
    explicit_references_json TEXT,
    evidence_refs_json       TEXT,
    citation_map_json        TEXT
);

INSERT INTO session_messages__legacy
    (id, session_id, seq, role, content, tool_calls, content_hash, created_at,
     content_parts, evidence_packets, vault_id, turn_id, explicit_references_json,
     evidence_refs_json, citation_map_json)
SELECT message.id,
       message.session_id,
       message.seq,
       message.role,
       message.content,
       message.tool_calls,
       message.content_hash,
       message.created_at,
       message.content_parts,
       COALESCE((
           SELECT json_group_array(json_object(
               'id', evidence.id,
               'citation_label', evidence.citation_label,
               'source_type', evidence.source_type,
               'title', evidence.title,
               'source_path', evidence.source_path,
               'url', evidence.url
           ))
           FROM session_evidence AS evidence
           WHERE evidence.session_id = message.session_id
             AND evidence.message_seq_first = message.seq
       ), '[]'),
       message.vault_id,
       message.turn_id,
       message.explicit_references_json,
       message.evidence_refs_json,
       message.citation_map_json
FROM session_messages AS message;

CREATE TABLE ai_traces (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    request_id   TEXT NOT NULL UNIQUE,
    scene        TEXT NOT NULL,
    model_slot   TEXT,
    provider     TEXT,
    tool_names   JSON,
    packet_ids   JSON,
    latency_ms   INTEGER,
    token_input  INTEGER,
    token_output INTEGER,
    status       TEXT NOT NULL,
    error_code   TEXT,
    created_at   TEXT NOT NULL,
    checkpoint   TEXT
);

INSERT INTO ai_traces
    (request_id, scene, token_input, token_output, status, error_code, created_at, checkpoint)
SELECT client_request_id,
       'legacy',
       token_input,
       token_output,
       CASE
           WHEN status = 'completed' THEN 'completed'
           WHEN status = 'failed' THEN 'failed'
           ELSE 'cancelled'
       END,
       error_code,
       created_at,
       NULL
FROM agent_runs;

CREATE TABLE agent_tasks (
    task_id            TEXT PRIMARY KEY,
    request_id         TEXT NOT NULL UNIQUE,
    session_id         INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    kind               TEXT NOT NULL,
    status             TEXT NOT NULL,
    user_goal_summary  TEXT NOT NULL DEFAULT '',
    budget_policy_json TEXT NOT NULL DEFAULT '{}',
    created_at         TEXT NOT NULL,
    updated_at         TEXT NOT NULL,
    completed_at       TEXT,
    error_code         TEXT,
    error_message      TEXT
);

INSERT INTO agent_tasks
    (task_id, request_id, session_id, kind, status, user_goal_summary,
     budget_policy_json, created_at, updated_at, completed_at, error_code, error_message)
SELECT substr(run_id, 13),
       client_request_id,
       session_id,
       'legacy',
       CASE
           WHEN status = 'completed' THEN 'completed'
           WHEN status = 'failed' THEN 'failed'
           ELSE 'cancelled'
       END,
       goal_summary,
       budget_policy_json,
       created_at,
       updated_at,
       completed_at,
       error_code,
       safe_error_message
FROM agent_runs
WHERE run_id LIKE 'legacy-task:%';

CREATE TABLE agent_task_steps (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id             TEXT NOT NULL REFERENCES agent_tasks(task_id) ON DELETE CASCADE,
    step_seq            INTEGER NOT NULL,
    kind                TEXT NOT NULL,
    status              TEXT NOT NULL,
    input_summary       TEXT NOT NULL DEFAULT '',
    output_summary      TEXT NOT NULL DEFAULT '',
    checkpoint_json     TEXT NOT NULL DEFAULT '{}',
    evidence_packet_ids TEXT NOT NULL DEFAULT '[]',
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    UNIQUE(task_id, step_seq)
);

INSERT INTO agent_task_steps
    (id, task_id, step_seq, kind, status, input_summary, output_summary,
     checkpoint_json, evidence_packet_ids, created_at, updated_at)
SELECT step.id,
       substr(step.run_id, 13),
       step.step_seq,
       step.kind,
       step.status,
       step.input_summary,
       step.output_summary,
       '{}',
       '[]',
       step.created_at,
       step.updated_at
FROM agent_run_steps AS step
WHERE step.run_id LIKE 'legacy-task:%';

CREATE TABLE agent_task_events (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id      TEXT NOT NULL REFERENCES agent_tasks(task_id) ON DELETE CASCADE,
    event_type   TEXT NOT NULL,
    message      TEXT NOT NULL DEFAULT '',
    payload_json TEXT NOT NULL DEFAULT '{}',
    created_at   TEXT NOT NULL
);

INSERT INTO agent_task_events
    (task_id, event_type, message, payload_json, created_at)
SELECT substr(run_id, 13),
       'terminal',
       CASE
           WHEN status = 'completed' THEN 'Legacy task completed'
           WHEN status = 'failed' THEN '旧版任务失败'
           ELSE 'Legacy task safely cancelled'
       END,
       '{}',
       COALESCE(completed_at, updated_at)
FROM agent_runs
WHERE run_id LIKE 'legacy-task:%';

CREATE TABLE deliberation_states (
    request_id          TEXT PRIMARY KEY REFERENCES ai_traces(request_id) ON DELETE CASCADE,
    session_id          INTEGER REFERENCES sessions(id) ON DELETE SET NULL,
    current_goal        TEXT NOT NULL,
    plan_outline_json   TEXT NOT NULL,
    assumptions_json    TEXT NOT NULL,
    open_questions_json TEXT NOT NULL,
    evidence_gaps_json  TEXT NOT NULL,
    verification_json   TEXT NOT NULL,
    status              TEXT NOT NULL,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL
);

CREATE TABLE writing_states (
    request_id             TEXT PRIMARY KEY REFERENCES ai_traces(request_id) ON DELETE CASCADE,
    target_path            TEXT NOT NULL,
    draft_version_hash     TEXT NOT NULL,
    document_goal          TEXT NOT NULL,
    audience               TEXT NOT NULL DEFAULT '',
    genre                  TEXT NOT NULL DEFAULT '',
    structure_outline_json TEXT NOT NULL DEFAULT '[]',
    key_arguments_json     TEXT NOT NULL DEFAULT '[]',
    material_packet_ids_json TEXT NOT NULL DEFAULT '[]',
    citation_labels_json   TEXT NOT NULL DEFAULT '[]',
    style_constraints_json TEXT NOT NULL DEFAULT '[]',
    revision_records_json  TEXT NOT NULL DEFAULT '[]',
    created_at             TEXT NOT NULL,
    updated_at             TEXT NOT NULL
);

CREATE TABLE research_states (
    request_id                   TEXT PRIMARY KEY REFERENCES ai_traces(request_id) ON DELETE CASCADE,
    research_question            TEXT NOT NULL,
    sub_questions_json           TEXT NOT NULL DEFAULT '[]',
    sources_json                 TEXT NOT NULL DEFAULT '[]',
    credibility_summary          TEXT NOT NULL DEFAULT '',
    freshness_summary            TEXT NOT NULL DEFAULT '',
    conflicts_json               TEXT NOT NULL DEFAULT '[]',
    counter_arguments_json       TEXT NOT NULL DEFAULT '[]',
    evidence_gaps_json           TEXT NOT NULL DEFAULT '[]',
    preliminary_conclusions_json TEXT NOT NULL DEFAULT '[]',
    created_at                   TEXT NOT NULL,
    updated_at                   TEXT NOT NULL
);

CREATE TABLE tool_audit__legacy (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    request_id        TEXT NOT NULL REFERENCES ai_traces(request_id),
    harness_round     INTEGER NOT NULL,
    tool_name         TEXT NOT NULL,
    arguments_summary TEXT,
    result_summary    TEXT,
    success           INTEGER NOT NULL DEFAULT 0,
    duration_ms       INTEGER,
    scene             TEXT,
    subagent_depth    INTEGER NOT NULL DEFAULT 0,
    created_at        TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO tool_audit__legacy
    (id, request_id, harness_round, tool_name, arguments_summary, result_summary,
     success, duration_ms, scene, subagent_depth, created_at)
SELECT audit.id,
       run.client_request_id,
       audit.run_step,
       audit.tool_name,
       audit.arguments_summary,
       audit.result_summary,
       audit.success,
       audit.duration_ms,
       NULL,
       audit.subagent_depth,
       audit.created_at
FROM tool_audit AS audit
JOIN agent_runs AS run ON run.run_id = audit.run_id;

CREATE TABLE agent_permission_audit__legacy (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    request_id      TEXT NOT NULL REFERENCES ai_traces(request_id),
    skill_id        TEXT,
    tool_name       TEXT NOT NULL,
    permission_name TEXT NOT NULL,
    decision        TEXT NOT NULL,
    scope_summary   TEXT NOT NULL,
    risk_level      TEXT NOT NULL,
    result_status   TEXT NOT NULL,
    created_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO agent_permission_audit__legacy
    (id, request_id, skill_id, tool_name, permission_name, decision,
     scope_summary, risk_level, result_status, created_at)
SELECT audit.id,
       run.client_request_id,
       audit.skill_id,
       audit.tool_name,
       audit.permission_name,
       audit.decision,
       audit.scope_summary,
       audit.risk_level,
       audit.result_status,
       audit.created_at
FROM agent_permission_audit AS audit
JOIN agent_runs AS run ON run.run_id = audit.run_id;

DROP TABLE agent_permission_audit;
DROP TABLE tool_audit;
DROP TABLE session_messages;
DROP TABLE sessions;

ALTER TABLE sessions__legacy RENAME TO sessions;
ALTER TABLE session_messages__legacy RENAME TO session_messages;
ALTER TABLE tool_audit__legacy RENAME TO tool_audit;
ALTER TABLE agent_permission_audit__legacy RENAME TO agent_permission_audit;

CREATE INDEX idx_sessions_vault_id ON sessions(vault_id);
CREATE INDEX idx_sessions_updated_at ON sessions(updated_at);
CREATE INDEX idx_session_messages_session ON session_messages(session_id, seq);
CREATE INDEX idx_session_messages_vault_id ON session_messages(vault_id);
CREATE INDEX idx_ai_traces_created ON ai_traces(created_at);
CREATE INDEX idx_ai_traces_created_prune ON ai_traces(created_at);
CREATE INDEX idx_agent_tasks_session ON agent_tasks(session_id);
CREATE INDEX idx_agent_tasks_status ON agent_tasks(status);
CREATE INDEX idx_agent_tasks_updated_at ON agent_tasks(updated_at);
CREATE INDEX idx_agent_task_steps_task ON agent_task_steps(task_id, step_seq);
CREATE INDEX idx_agent_task_events_task ON agent_task_events(task_id, id);
CREATE INDEX idx_tool_audit_request_id ON tool_audit(request_id);
CREATE INDEX idx_tool_audit_tool_name ON tool_audit(tool_name);
CREATE INDEX idx_agent_permission_audit_request ON agent_permission_audit(request_id);
CREATE INDEX idx_agent_permission_audit_permission ON agent_permission_audit(permission_name);
CREATE INDEX idx_agent_permission_audit_tool ON agent_permission_audit(tool_name);
CREATE INDEX idx_deliberation_states_session ON deliberation_states(session_id, updated_at);
CREATE INDEX idx_writing_states_target ON writing_states(target_path, updated_at);
CREATE INDEX idx_research_states_updated ON research_states(updated_at);

-- 032: Agent task runtime persistence.
-- Stores task state and summaries only; conversation bodies remain in session_messages.

CREATE TABLE IF NOT EXISTS agent_tasks (
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

CREATE INDEX IF NOT EXISTS idx_agent_tasks_session ON agent_tasks(session_id);
CREATE INDEX IF NOT EXISTS idx_agent_tasks_status ON agent_tasks(status);
CREATE INDEX IF NOT EXISTS idx_agent_tasks_updated_at ON agent_tasks(updated_at);

CREATE TABLE IF NOT EXISTS agent_task_steps (
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

CREATE INDEX IF NOT EXISTS idx_agent_task_steps_task ON agent_task_steps(task_id, step_seq);

CREATE TABLE IF NOT EXISTS agent_task_events (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id      TEXT NOT NULL REFERENCES agent_tasks(task_id) ON DELETE CASCADE,
    event_type   TEXT NOT NULL,
    message      TEXT NOT NULL DEFAULT '',
    payload_json TEXT NOT NULL DEFAULT '{}',
    created_at   TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_agent_task_events_task ON agent_task_events(task_id, id);

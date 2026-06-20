-- 033: Conversation memory and deliberation state
-- Stores bounded summaries/status only; raw prompts, full checkpoints, and note bodies stay out.

CREATE TABLE IF NOT EXISTS conversation_summaries (
    id                   INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id           INTEGER NOT NULL UNIQUE REFERENCES sessions(id) ON DELETE CASCADE,
    seq_start            INTEGER NOT NULL,
    seq_end              INTEGER NOT NULL,
    content_hash         TEXT NOT NULL,
    goal_summary         TEXT NOT NULL DEFAULT '',
    preference_summary   TEXT NOT NULL DEFAULT '',
    decision_summary     TEXT NOT NULL DEFAULT '',
    open_threads_summary TEXT NOT NULL DEFAULT '',
    created_at           TEXT NOT NULL,
    updated_at           TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_conversation_summaries_session
    ON conversation_summaries(session_id, seq_end);

CREATE TABLE IF NOT EXISTS deliberation_states (
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

CREATE INDEX IF NOT EXISTS idx_deliberation_states_session
    ON deliberation_states(session_id, updated_at);

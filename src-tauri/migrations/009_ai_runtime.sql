-- 009: AI Runtime Foundation tables
-- sessions + session_messages: 可删除会话缓存
-- ai_traces: 追踪元数据（不含笔记全文）
-- user_profile: 显式偏好和规则
-- knowledge_deposits: 待整理 AI 收件箱
-- files 扩展: genre, content_hash
-- chunks 扩展: embedding_model

-- ─── sessions ───
CREATE TABLE IF NOT EXISTS sessions (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    session_key      TEXT NOT NULL UNIQUE,
    scene            TEXT NOT NULL,
    note_path        TEXT,
    retention_policy TEXT NOT NULL DEFAULT 'user_clearable',
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS session_messages (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id    INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    seq           INTEGER NOT NULL,
    role          TEXT NOT NULL,
    content       TEXT NOT NULL,
    tool_calls    JSON,
    content_hash  TEXT,
    created_at    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_session_messages_session ON session_messages(session_id, seq);

-- ─── ai_traces ───
CREATE TABLE IF NOT EXISTS ai_traces (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    request_id      TEXT NOT NULL UNIQUE,
    scene           TEXT NOT NULL,
    model_slot      TEXT,
    provider        TEXT,
    tool_names      JSON,
    packet_ids      JSON,
    latency_ms      INTEGER,
    token_input     INTEGER,
    token_output    INTEGER,
    status          TEXT NOT NULL,
    error_code      TEXT,
    created_at      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_ai_traces_created ON ai_traces(created_at);

-- ─── user_profile ───
CREATE TABLE IF NOT EXISTS user_profile (
    key        TEXT PRIMARY KEY,
    value      JSON NOT NULL,
    source     TEXT NOT NULL,
    confidence REAL NOT NULL DEFAULT 1.0,
    is_active  INTEGER NOT NULL DEFAULT 1,
    updated_at TEXT NOT NULL
);

-- ─── knowledge_deposits ───
CREATE TABLE IF NOT EXISTS knowledge_deposits (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id     INTEGER REFERENCES sessions(id) ON DELETE SET NULL,
    source_note    TEXT,
    deposit_type   TEXT NOT NULL,
    content        TEXT NOT NULL,
    status         TEXT NOT NULL DEFAULT 'inbox',
    target_path    TEXT,
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL
);

-- ─── files 扩展 ───
ALTER TABLE files ADD COLUMN genre TEXT;

-- ─── chunks 扩展 ───
ALTER TABLE chunks ADD COLUMN embedding_model TEXT;

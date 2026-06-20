-- 034: Writing and research state summaries.
-- Stores bounded collaboration/research state only. Raw note bodies, selections, and web pages stay out.

CREATE TABLE IF NOT EXISTS writing_states (
    request_id            TEXT PRIMARY KEY REFERENCES ai_traces(request_id) ON DELETE CASCADE,
    target_path           TEXT NOT NULL,
    draft_version_hash    TEXT NOT NULL,
    document_goal         TEXT NOT NULL,
    audience              TEXT NOT NULL DEFAULT '',
    genre                 TEXT NOT NULL DEFAULT '',
    structure_outline_json TEXT NOT NULL DEFAULT '[]',
    key_arguments_json    TEXT NOT NULL DEFAULT '[]',
    material_packet_ids_json TEXT NOT NULL DEFAULT '[]',
    citation_labels_json  TEXT NOT NULL DEFAULT '[]',
    style_constraints_json TEXT NOT NULL DEFAULT '[]',
    revision_records_json TEXT NOT NULL DEFAULT '[]',
    created_at            TEXT NOT NULL,
    updated_at            TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_writing_states_target
    ON writing_states(target_path, updated_at);

CREATE TABLE IF NOT EXISTS research_states (
    request_id                  TEXT PRIMARY KEY REFERENCES ai_traces(request_id) ON DELETE CASCADE,
    research_question           TEXT NOT NULL,
    sub_questions_json          TEXT NOT NULL DEFAULT '[]',
    sources_json                TEXT NOT NULL DEFAULT '[]',
    credibility_summary         TEXT NOT NULL DEFAULT '',
    freshness_summary           TEXT NOT NULL DEFAULT '',
    conflicts_json              TEXT NOT NULL DEFAULT '[]',
    counter_arguments_json      TEXT NOT NULL DEFAULT '[]',
    evidence_gaps_json          TEXT NOT NULL DEFAULT '[]',
    preliminary_conclusions_json TEXT NOT NULL DEFAULT '[]',
    created_at                  TEXT NOT NULL,
    updated_at                  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_research_states_updated
    ON research_states(updated_at);

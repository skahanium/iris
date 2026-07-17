-- Persist immutable normal-domain retrieval boundaries and inline display annotations.
ALTER TABLE session_messages
    ADD COLUMN context_scope_json TEXT NOT NULL DEFAULT '[]';

ALTER TABLE session_messages
    ADD COLUMN display_mentions_json TEXT NOT NULL DEFAULT '[]';

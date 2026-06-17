ALTER TABLE sessions ADD COLUMN vault_id TEXT;
ALTER TABLE session_messages ADD COLUMN vault_id TEXT;
ALTER TABLE ai_memories ADD COLUMN vault_id TEXT;
ALTER TABLE knowledge_deposits ADD COLUMN vault_id TEXT;
ALTER TABLE user_profile ADD COLUMN vault_id TEXT;
ALTER TABLE web_page_cache ADD COLUMN vault_id TEXT;
ALTER TABLE search_cache ADD COLUMN vault_id TEXT;

CREATE INDEX IF NOT EXISTS idx_sessions_vault_id ON sessions(vault_id);
CREATE INDEX IF NOT EXISTS idx_session_messages_vault_id ON session_messages(vault_id);
CREATE INDEX IF NOT EXISTS idx_ai_memories_vault_id ON ai_memories(vault_id);
CREATE INDEX IF NOT EXISTS idx_knowledge_deposits_vault_id ON knowledge_deposits(vault_id);
CREATE INDEX IF NOT EXISTS idx_user_profile_vault_id ON user_profile(vault_id);
CREATE INDEX IF NOT EXISTS idx_web_page_cache_vault_id ON web_page_cache(vault_id);
CREATE INDEX IF NOT EXISTS idx_search_cache_vault_id ON search_cache(vault_id);

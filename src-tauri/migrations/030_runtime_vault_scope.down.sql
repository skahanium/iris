DROP INDEX IF EXISTS idx_search_cache_vault_id;
DROP INDEX IF EXISTS idx_web_page_cache_vault_id;
DROP INDEX IF EXISTS idx_user_profile_vault_id;
DROP INDEX IF EXISTS idx_knowledge_deposits_vault_id;
DROP INDEX IF EXISTS idx_ai_memories_vault_id;
DROP INDEX IF EXISTS idx_session_messages_vault_id;
DROP INDEX IF EXISTS idx_sessions_vault_id;

ALTER TABLE search_cache DROP COLUMN vault_id;
ALTER TABLE web_page_cache DROP COLUMN vault_id;
ALTER TABLE user_profile DROP COLUMN vault_id;
ALTER TABLE knowledge_deposits DROP COLUMN vault_id;
ALTER TABLE ai_memories DROP COLUMN vault_id;
ALTER TABLE session_messages DROP COLUMN vault_id;
ALTER TABLE sessions DROP COLUMN vault_id;

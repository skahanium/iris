-- 009 down: remove AI Runtime tables and column extensions

DROP TABLE IF EXISTS knowledge_deposits;
DROP TABLE IF EXISTS session_messages;
DROP TABLE IF EXISTS sessions;
DROP TABLE IF EXISTS ai_traces;
DROP TABLE IF EXISTS user_profile;

-- sqlite 不支持 DROP COLUMN before 3.35，但 rusqlite bundled 支持
ALTER TABLE files DROP COLUMN genre;

ALTER TABLE chunks DROP COLUMN embedding_model;

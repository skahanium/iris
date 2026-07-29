-- 058: durable Agent intake identity and prompt-contract metadata.
--
-- All fields are summary-shaped. Full prompts, note bodies, provider payloads
-- and credentials must never enter this ledger.

ALTER TABLE agent_runs ADD COLUMN intake_fingerprint TEXT;
ALTER TABLE agent_runs ADD COLUMN prompt_profile_snapshot_json TEXT;
ALTER TABLE agent_runs ADD COLUMN prompt_contract_version INTEGER;
ALTER TABLE agent_runs ADD COLUMN prompt_contract_hash TEXT;

-- Conversation summaries are derived from message history. They must be
-- rebuilt from the committed-turn projection after this contract changes.
DELETE FROM conversation_summaries;
